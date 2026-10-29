pub(crate) mod agent_config;
pub(crate) mod agent_gather;
pub(crate) mod agent_internal;
pub(crate) mod agent_selector;
pub(crate) mod agent_transport;

use crate::webrtc::ice::candidate::*;
use crate::webrtc::ice::error::*;
use crate::webrtc::ice::external_ip_mapper::*;
use crate::webrtc::ice::mdns::*;
use crate::webrtc::ice::network_type::*;
use crate::webrtc::ice::state::*;
use agent_config::*;
use agent_internal::*;

use crate::webrtc::stun::{
    agent::*, attributes::*, fingerprint::*, integrity::*, message::*, xoraddr::*,
};
use crate::webrtc::util::{vnet::net::*, Buffer};
use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr};

use crate::webrtc::ice::agent::agent_gather::GatherCandidatesInternalParams;
use crate::webrtc::ice::rand::*;
use async_lock::Mutex;
use futures_util::{select, FutureExt};
use smol::channel;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::SystemTime;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub(crate) struct BindingRequest {
    pub(crate) timestamp: Instant,
    pub(crate) transaction_id: TransactionId,
    pub(crate) destination: SocketAddr,
    pub(crate) is_use_candidate: bool,
}

impl Default for BindingRequest {
    fn default() -> Self {
        Self {
            timestamp: Instant::now(),
            transaction_id: TransactionId::default(),
            destination: SocketAddr::new(Ipv4Addr::new(0, 0, 0, 0).into(), 0),
            is_use_candidate: false,
        }
    }
}

pub(crate) type OnConnectionStateChangeHdlrFn = Box<
    dyn (FnMut(ConnectionState) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>)
        + Send
        + Sync,
>;
pub(crate) type OnSelectedCandidatePairChangeHdlrFn = Box<
    dyn (FnMut(
            &Arc<dyn Candidate + Send + Sync>,
            &Arc<dyn Candidate + Send + Sync>,
        ) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>)
        + Send
        + Sync,
>;
pub(crate) type OnCandidateHdlrFn = Box<
    dyn (FnMut(
            Option<Arc<dyn Candidate + Send + Sync>>,
        ) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>)
        + Send
        + Sync,
>;
pub(crate) type GatherCandidateCancelFn = Box<dyn Fn() + Send + Sync>;

pub(crate) struct ChanReceivers {
    chan_state_rx: channel::Receiver<ConnectionState>,
    chan_candidate_rx: channel::Receiver<Option<Arc<dyn Candidate + Send + Sync>>>,
    chan_candidate_pair_rx: channel::Receiver<()>,
}

/// Represents the ICE agent.
pub(crate) struct Agent {
    pub(crate) internal: Arc<AgentInternal>,

    pub(crate) interface_filter: Arc<Option<InterfaceFilterFn>>,
    pub(crate) mdns_mode: MulticastDnsMode,
    pub(crate) mdns_name: String,
    pub(crate) net: Arc<Net>,

    // 1:1 D-NAT IP address mapping
    pub(crate) ext_ip_mapper: Arc<Option<ExternalIpMapper>>,
    pub(crate) gathering_state: Arc<AtomicU8>, //GatheringState,
    pub(crate) candidate_types: Vec<CandidateType>,
    pub(crate) network_types: Vec<NetworkType>,

    pub(crate) gather_candidate_cancel: Option<GatherCandidateCancelFn>,
}

impl Agent {
    /// Creates a new Agent.
    pub(crate) async fn new(config: AgentConfig) -> Result<Self> {
        let mut mdns_name = config.multicast_dns_host_name.clone();
        if mdns_name.is_empty() {
            mdns_name = generate_multicast_dns_name();
        }

        if !mdns_name.ends_with(".local") || mdns_name.split('.').count() != 2 {
            return Err(Error::ErrInvalidMulticastDnshostName);
        }

        let mut mdns_mode = config.multicast_dns_mode;
        if mdns_mode == MulticastDnsMode::Unspecified {
            mdns_mode = MulticastDnsMode::QueryOnly;
        }

        let (mut ai, chan_receivers) = AgentInternal::new(&config);
        let (chan_state_rx, chan_candidate_rx, chan_candidate_pair_rx) = (
            chan_receivers.chan_state_rx,
            chan_receivers.chan_candidate_rx,
            chan_receivers.chan_candidate_pair_rx,
        );

        config.init_with_defaults(&mut ai);

        let candidate_types = if config.candidate_types.is_empty() {
            default_candidate_types()
        } else {
            config.candidate_types.clone()
        };

        if ai.lite.load(Ordering::SeqCst)
            && (candidate_types.len() != 1 || candidate_types[0] != CandidateType::Host)
        {
            return Err(Error::ErrLiteUsingNonHostCandidates);
        }

        let ext_ip_mapper = match config.init_ext_ip_mapping(mdns_mode, &candidate_types) {
            Ok(ext_ip_mapper) => ext_ip_mapper,
            Err(err) => {
                return Err(err);
            }
        };

        let net = Arc::new(Net::new());

        let agent = Self {
            internal: Arc::new(ai),
            interface_filter: Arc::clone(&config.interface_filter),
            mdns_mode,
            mdns_name,
            net,
            ext_ip_mapper: Arc::new(ext_ip_mapper),
            gathering_state: Arc::new(AtomicU8::new(0)), //GatheringState::New,
            candidate_types,
            network_types: config.network_types.clone(),

            gather_candidate_cancel: None, //TODO: add cancel
        };

        agent
            .internal
            .start_on_connection_state_change_routine(
                chan_state_rx,
                chan_candidate_rx,
                chan_candidate_pair_rx,
            )
            .await;

        // Restart is also used to initialize the agent for the first time
        if let Err(err) = agent.restart(config.local_ufrag, config.local_pwd).await {
            let _ = agent.close().await;
            return Err(err);
        }

        Ok(agent)
    }

    /// Sets a handler that is fired when the connection state changes.
    pub(crate) async fn on_connection_state_change(&self, f: OnConnectionStateChangeHdlrFn) {
        let mut on_connection_state_change_hdlr =
            self.internal.on_connection_state_change_hdlr.lock().await;
        *on_connection_state_change_hdlr = Some(f);
    }

    /// Sets a handler that is fired when the final candidate pair is selected.
    pub(crate) async fn on_selected_candidate_pair_change(
        &self,
        f: OnSelectedCandidatePairChangeHdlrFn,
    ) {
        let mut on_selected_candidate_pair_change_hdlr = self
            .internal
            .on_selected_candidate_pair_change_hdlr
            .lock()
            .await;
        *on_selected_candidate_pair_change_hdlr = Some(f);
    }

    /// Sets a handler that is fired when new candidates gathered. When the gathering process
    /// complete the last candidate is nil.
    pub(crate) async fn on_candidate(&self, f: OnCandidateHdlrFn) {
        let mut on_candidate_hdlr = self.internal.on_candidate_hdlr.lock().await;
        *on_candidate_hdlr = Some(f);
    }

    /// Adds a new remote candidate.
    pub(crate) async fn add_remote_candidate(
        &self,
        c: &Arc<dyn Candidate + Send + Sync>,
    ) -> Result<()> {
        // If we have a mDNS Candidate lets fully resolve it before adding it locally
        if c.candidate_type() == CandidateType::Host && c.address().ends_with(".local") {
            if self.mdns_mode == MulticastDnsMode::Disabled {
                log::warn!(
                    "remote mDNS candidate added, but mDNS is disabled: ({})",
                    c.address()
                );
                return Ok(());
            }

            if c.candidate_type() != CandidateType::Host {
                return Err(Error::ErrAddressParseFailed);
            }
        } else {
            let ai = Arc::clone(&self.internal);
            let candidate = Arc::clone(c);
            smol::spawn(async move {
                ai.add_remote_candidate(&candidate).await;
            })
            .detach();
        }

        Ok(())
    }

    /// Returns the local candidates.
    pub(crate) async fn get_local_candidates(
        &self,
    ) -> Result<Vec<Arc<dyn Candidate + Send + Sync>>> {
        let mut res = vec![];

        {
            let local_candidates = self.internal.local_candidates.lock().await;
            for candidates in local_candidates.values() {
                for candidate in candidates {
                    res.push(Arc::clone(candidate));
                }
            }
        }

        Ok(res)
    }

    /// Returns the local user credentials.
    pub(crate) async fn get_local_user_credentials(&self) -> (String, String) {
        let ufrag_pwd = self.internal.ufrag_pwd.lock().await;
        (ufrag_pwd.local_ufrag.clone(), ufrag_pwd.local_pwd.clone())
    }

    /// Cleans up the Agent.
    pub(crate) async fn close(&self) -> Result<()> {
        if let Some(gather_candidate_cancel) = &self.gather_candidate_cancel {
            gather_candidate_cancel();
        }

        //FIXME: deadlock here
        self.internal.close().await
    }

    /// Restarts the ICE Agent with the provided ufrag/pwd
    /// If no ufrag/pwd is provided the Agent will generate one itself.
    ///
    /// Restart must only be called when `GatheringState` is `GatheringStateComplete`
    /// a user must then call `GatherCandidates` explicitly to start generating new ones.
    pub(crate) async fn restart(&self, mut ufrag: String, mut pwd: String) -> Result<()> {
        if ufrag.is_empty() {
            ufrag = generate_ufrag();
        }
        if pwd.is_empty() {
            pwd = generate_pwd();
        }

        if ufrag.len() * 8 < 24 {
            return Err(Error::ErrLocalUfragInsufficientBits);
        }
        if pwd.len() * 8 < 128 {
            return Err(Error::ErrLocalPwdInsufficientBits);
        }

        if GatheringState::from(self.gathering_state.load(Ordering::SeqCst))
            == GatheringState::Gathering
        {
            return Err(Error::ErrRestartWhenGathering);
        }
        self.gathering_state
            .store(GatheringState::New as u8, Ordering::SeqCst);

        {
            let done_tx = self.internal.done_tx.lock().await;
            if done_tx.is_none() {
                return Err(Error::ErrClosed);
            }
        }

        // Clear all agent needed to take back to fresh state
        {
            let mut ufrag_pwd = self.internal.ufrag_pwd.lock().await;
            ufrag_pwd.local_ufrag = ufrag;
            ufrag_pwd.local_pwd = pwd;
            ufrag_pwd.remote_ufrag = String::new();
            ufrag_pwd.remote_pwd = String::new();
        }
        {
            let mut pending_binding_requests = self.internal.pending_binding_requests.lock().await;
            *pending_binding_requests = vec![];
        }

        {
            let mut checklist = self.internal.agent_conn.checklist.lock().await;
            *checklist = vec![];
        }

        self.internal.set_selected_pair(None).await;
        self.internal.delete_all_candidates().await;
        self.internal.start().await;

        // Restart is used by NewAgent. Accept/Connect should be used to move to checking
        // for new Agents
        if self.internal.connection_state.load(Ordering::SeqCst) != ConnectionState::New as u8 {
            self.internal
                .update_connection_state(ConnectionState::Checking)
                .await;
        }

        Ok(())
    }

    /// Initiates the trickle based gathering process.
    pub(crate) async fn gather_candidates(&self) -> Result<()> {
        if self.gathering_state.load(Ordering::SeqCst) != GatheringState::New as u8 {
            return Err(Error::ErrMultipleGatherAttempted);
        }

        {
            let on_candidate_hdlr = self.internal.on_candidate_hdlr.lock().await;
            if on_candidate_hdlr.is_none() {
                return Err(Error::ErrNoOnCandidateHandler);
            }
        }

        if let Some(gather_candidate_cancel) = &self.gather_candidate_cancel {
            gather_candidate_cancel(); // Cancel previous gathering routine
        }

        //TODO: a.gatherCandidateCancel = cancel

        let params = GatherCandidatesInternalParams {
            candidate_types: self.candidate_types.clone(),
            network_types: self.network_types.clone(),
            mdns_mode: self.mdns_mode,
            mdns_name: self.mdns_name.clone(),
            net: Arc::clone(&self.net),
            interface_filter: self.interface_filter.clone(),
            ext_ip_mapper: Arc::clone(&self.ext_ip_mapper),
            agent_internal: Arc::clone(&self.internal),
            gathering_state: Arc::clone(&self.gathering_state),
            chan_candidate_tx: Arc::clone(&self.internal.chan_candidate_tx),
        };
        smol::spawn(async move {
            Self::gather_candidates_internal(params).await;
        })
        .detach();

        Ok(())
    }
}
