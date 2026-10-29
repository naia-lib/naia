use crate::webrtc::error::{Error, Result};
use crate::webrtc::ice_transport::ice_candidate::*;
use crate::webrtc::ice_transport::ice_gatherer_state::RTCIceGathererState;
use crate::webrtc::ice_transport::ice_parameters::RTCIceParameters;

use crate::webrtc::ice::agent::Agent;
use crate::webrtc::ice::candidate::{Candidate, CandidateType};

use crate::webrtc::ice::mdns::MulticastDnsMode;
use async_lock::Mutex;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

pub(crate) type OnLocalCandidateHdlrFn = Box<
    dyn (FnMut(Option<RTCIceCandidate>) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>)
        + Send
        + Sync,
>;

pub(crate) type OnICEGathererStateChangeHdlrFn = Box<
    dyn (FnMut(RTCIceGathererState) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>)
        + Send
        + Sync,
>;

pub(crate) type OnGatheringCompleteHdlrFn =
    Box<dyn (FnMut() -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>) + Send + Sync>;

/// ICEGatherer gathers local host, server reflexive and relay
/// candidates, as well as enabling the retrieval of local Interactive
/// Connectivity Establishment (ICE) parameters which can be
/// exchanged in signaling.
#[derive(Default)]
pub(crate) struct RTCIceGatherer {
    pub(crate) state: Arc<AtomicU8>, //ICEGathererState,
    pub(crate) agent: Mutex<Option<Arc<crate::webrtc::ice::agent::Agent>>>,

    pub(crate) on_local_candidate_handler: Arc<Mutex<Option<OnLocalCandidateHdlrFn>>>,
    pub(crate) on_state_change_handler: Arc<Mutex<Option<OnICEGathererStateChangeHdlrFn>>>,

    // Used for gathering_complete_promise
    pub(crate) on_gathering_complete_handler: Arc<Mutex<Option<OnGatheringCompleteHdlrFn>>>,
}

impl RTCIceGatherer {
    pub(crate) fn new() -> Self {
        RTCIceGatherer {
            state: Arc::new(AtomicU8::new(RTCIceGathererState::New as u8)),
            ..Default::default()
        }
    }

    pub(crate) async fn create_agent(&self) -> Result<()> {
        {
            let agent = self.agent.lock().await;
            if agent.is_some() || self.state() != RTCIceGathererState::New {
                return Ok(());
            }
        }

        let mut mdns_mode = MulticastDnsMode::Unspecified;
        if mdns_mode != crate::webrtc::ice::mdns::MulticastDnsMode::Disabled
            && mdns_mode != crate::webrtc::ice::mdns::MulticastDnsMode::QueryAndGather
        {
            // If enum is in state we don't recognized default to MulticastDNSModeQueryOnly
            mdns_mode = crate::webrtc::ice::mdns::MulticastDnsMode::QueryOnly;
        }

        let mut config = crate::webrtc::ice::agent::agent_config::AgentConfig {
            lite: false,
            disconnected_timeout: None,
            failed_timeout: None,
            keepalive_interval: None,
            candidate_types: Vec::new(),
            host_acceptance_min_wait: None,
            srflx_acceptance_min_wait: None,
            prflx_acceptance_min_wait: None,
            relay_acceptance_min_wait: None,
            nat_1to1_ip_candidate_type: CandidateType::Unspecified,
            multicast_dns_mode: mdns_mode,
            //TODO: TCPMux:                 self.setting_engine.iceTCPMux,
            //TODO: ProxyDialer:            self.setting_engine.iceProxyDialer,
            ..Default::default()
        };

        let requested_network_types = crate::webrtc::ice::network_type::supported_network_types();

        config.network_types.extend(requested_network_types);

        {
            let mut agent = self.agent.lock().await;
            *agent = Some(Arc::new(
                crate::webrtc::ice::agent::Agent::new(config).await?,
            ));
        }

        Ok(())
    }

    /// Gather ICE candidates.
    pub(crate) async fn gather(&self) -> Result<()> {
        self.create_agent().await?;
        self.set_state(RTCIceGathererState::Gathering).await;

        if let Some(agent) = self.get_agent().await {
            let state = Arc::clone(&self.state);
            let on_local_candidate_handler = Arc::clone(&self.on_local_candidate_handler);
            let on_state_change_handler = Arc::clone(&self.on_state_change_handler);
            let on_gathering_complete_handler = Arc::clone(&self.on_gathering_complete_handler);

            agent
                .on_candidate(Box::new(
                    move |candidate: Option<Arc<dyn Candidate + Send + Sync>>| {
                        let state_clone = Arc::clone(&state);
                        let on_local_candidate_handler_clone =
                            Arc::clone(&on_local_candidate_handler);
                        let on_state_change_handler_clone = Arc::clone(&on_state_change_handler);
                        let on_gathering_complete_handler_clone =
                            Arc::clone(&on_gathering_complete_handler);

                        Box::pin(async move {
                            if let Some(cand) = candidate {
                                let c = RTCIceCandidate::from(&cand);

                                let mut on_local_candidate_handler =
                                    on_local_candidate_handler_clone.lock().await;
                                if let Some(handler) = &mut *on_local_candidate_handler {
                                    handler(Some(c)).await;
                                }
                            } else {
                                state_clone
                                    .store(RTCIceGathererState::Complete as u8, Ordering::SeqCst);

                                {
                                    let mut on_state_change_handler =
                                        on_state_change_handler_clone.lock().await;
                                    if let Some(handler) = &mut *on_state_change_handler {
                                        handler(RTCIceGathererState::Complete).await;
                                    }
                                }

                                {
                                    let mut on_gathering_complete_handler =
                                        on_gathering_complete_handler_clone.lock().await;
                                    if let Some(handler) = &mut *on_gathering_complete_handler {
                                        handler().await;
                                    }
                                }

                                {
                                    let mut on_local_candidate_handler =
                                        on_local_candidate_handler_clone.lock().await;
                                    if let Some(handler) = &mut *on_local_candidate_handler {
                                        handler(None).await;
                                    }
                                }
                            }
                        })
                    },
                ))
                .await;

            agent.gather_candidates().await?;
        }

        Ok(())
    }

    /// get_local_parameters returns the ICE parameters of the ICEGatherer.
    pub(crate) async fn get_local_parameters(&self) -> Result<RTCIceParameters> {
        self.create_agent().await?;

        let (frag, pwd) = if let Some(agent) = self.get_agent().await {
            agent.get_local_user_credentials().await
        } else {
            return Err(Error::ErrICEAgentNotExist);
        };

        Ok(RTCIceParameters {
            username_fragment: frag,
            password: pwd,
        })
    }

    /// get_local_candidates returns the sequence of valid local candidates associated with the ICEGatherer.
    pub(crate) async fn get_local_candidates(&self) -> Result<Vec<RTCIceCandidate>> {
        self.create_agent().await?;

        let ice_candidates = if let Some(agent) = self.get_agent().await {
            agent.get_local_candidates().await?
        } else {
            return Err(Error::ErrICEAgentNotExist);
        };

        Ok(rtc_ice_candidates_from_ice_candidates(&ice_candidates))
    }

    /// State indicates the current state of the ICE gatherer.
    pub(crate) fn state(&self) -> RTCIceGathererState {
        self.state.load(Ordering::SeqCst).into()
    }

    pub(crate) async fn set_state(&self, s: RTCIceGathererState) {
        self.state.store(s as u8, Ordering::SeqCst);

        let mut on_state_change_handler = self.on_state_change_handler.lock().await;
        if let Some(handler) = &mut *on_state_change_handler {
            handler(s).await;
        }
    }

    pub(crate) async fn get_agent(&self) -> Option<Arc<Agent>> {
        let agent = self.agent.lock().await;
        agent.clone()
    }
}
