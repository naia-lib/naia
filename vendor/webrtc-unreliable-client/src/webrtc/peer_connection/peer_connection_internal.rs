use crate::webrtc::peer_connection::*;
use futures_util::future::pending;
use std::sync::atomic::AtomicIsize;

pub(crate) struct PeerConnectionInternal {
    /// a value containing the last known greater mid value
    /// we internally generate mids as numbers. Needed since JSEP
    /// requires that when reusing a media section a new unique mid
    /// should be defined (see JSEP 3.4.1).
    pub(crate) greater_mid: AtomicIsize,
    pub(crate) sdp_origin: Mutex<crate::webrtc::sdp::description::session::Origin>,
    pub(crate) last_offer: Mutex<String>,
    pub(crate) last_answer: Mutex<String>,

    pub(crate) is_closed: Arc<AtomicBool>,

    /// ops is an operations queue which will ensure the enqueued actions are
    /// executed in order. It is used for asynchronously, but serially processing
    /// remote and local descriptions
    pub(crate) ops: Arc<Operations>,
    pub(crate) signaling_state: Arc<AtomicU8>,

    pub(crate) ice_transport: Arc<RTCIceTransport>,
    pub(crate) dtls_transport: Arc<RTCDtlsTransport>,
    pub(crate) on_peer_connection_state_change_handler:
        Arc<Mutex<Option<OnPeerConnectionStateChangeHdlrFn>>>,
    pub(crate) peer_connection_state: Arc<AtomicU8>,
    pub(crate) ice_connection_state: Arc<AtomicU8>,

    pub(crate) sctp_transport: Arc<RTCSctpTransport>,

    pub(crate) on_signaling_state_change_handler: Arc<Mutex<Option<OnSignalingStateChangeHdlrFn>>>,
    pub(crate) on_ice_connection_state_change_handler:
        Arc<Mutex<Option<OnICEConnectionStateChangeHdlrFn>>>,
    pub(crate) on_data_channel_handler: Arc<Mutex<Option<OnDataChannelHdlrFn>>>,

    pub(crate) ice_gatherer: Arc<RTCIceGatherer>,

    pub(crate) current_local_description: Arc<Mutex<Option<RTCSessionDescription>>>,
    pub(crate) current_remote_description: Arc<Mutex<Option<RTCSessionDescription>>>,
    pub(crate) pending_local_description: Arc<Mutex<Option<RTCSessionDescription>>>,
    pub(crate) pending_remote_description: Arc<Mutex<Option<RTCSessionDescription>>>,
}

impl PeerConnectionInternal {
    pub(crate) async fn new() -> Result<Arc<Self>> {
        let mut pc = PeerConnectionInternal {
            greater_mid: AtomicIsize::new(-1),
            sdp_origin: Mutex::new(Default::default()),
            last_offer: Mutex::new("".to_owned()),
            last_answer: Mutex::new("".to_owned()),
            ops: Arc::new(Operations::new()),
            is_closed: Arc::new(AtomicBool::new(false)),
            signaling_state: Arc::new(AtomicU8::new(RTCSignalingState::Stable as u8)),
            ice_transport: Arc::new(Default::default()),
            dtls_transport: Arc::new(Default::default()),
            ice_connection_state: Arc::new(AtomicU8::new(RTCIceConnectionState::New as u8)),
            sctp_transport: Arc::new(Default::default()),
            on_signaling_state_change_handler: Arc::new(Default::default()),
            on_ice_connection_state_change_handler: Arc::new(Default::default()),
            on_data_channel_handler: Arc::new(Default::default()),
            ice_gatherer: Arc::new(Default::default()),
            current_local_description: Arc::new(Default::default()),
            current_remote_description: Arc::new(Default::default()),
            pending_local_description: Arc::new(Default::default()),
            peer_connection_state: Arc::new(AtomicU8::new(RTCPeerConnectionState::New as u8)),
            on_peer_connection_state_change_handler: Arc::new(Default::default()),
            pending_remote_description: Arc::new(Default::default()),
        };

        // Create the ice gatherer
        pc.ice_gatherer = Arc::new(API::new_ice_gatherer()?);

        // Create the ice transport
        pc.ice_transport = pc.create_ice_transport().await;

        // Create the DTLS transport
        pc.dtls_transport = Arc::new(API::new_dtls_transport(Arc::clone(&pc.ice_transport))?);

        // Create the SCTP transport
        pc.sctp_transport = Arc::new(API::new_sctp_transport(Arc::clone(&pc.dtls_transport))?);

        // Wire up the on datachannel handler
        let on_data_channel_handler = Arc::clone(&pc.on_data_channel_handler);
        pc.sctp_transport
            .on_data_channel(Box::new(move |d: Arc<RTCDataChannel>| {
                let on_data_channel_handler2 = Arc::clone(&on_data_channel_handler);
                Box::pin(async move {
                    let mut handler = on_data_channel_handler2.lock().await;
                    if let Some(f) = &mut *handler {
                        f(d).await;
                    }
                })
            }))
            .await;

        Ok(Arc::new(pc))
    }

    pub(crate) async fn maybe_start_sctp(
        self: &Arc<Self>,
        remote_desc: Arc<RTCSessionDescription>,
    ) -> Result<()> {
        let dtls_transport = Arc::clone(&self.dtls_transport);
        // No idea why, but this code here that doesn't do anything is necessary for this app to function ...
        smol::spawn(async move {
            let _holder = Arc::clone(&dtls_transport);
            pending::<()>().await;
        })
        .detach();

        if let Some(parsed) = &remote_desc.parsed {
            if have_application_media_section(parsed) {
                self.start_sctp().await;
            }
        }

        Ok(())
    }

    /// Start SCTP subsystem
    async fn start_sctp(&self) {
        // Start sctp
        if let Err(err) = self.sctp_transport.start().await {
            log::warn!("Failed to start SCTP: {}", err);
            if let Err(err) = self.sctp_transport.stop().await {
                log::warn!("Failed to stop SCTPTransport: {}", err);
            }

            return;
        }

        // DataChannels that need to be opened now that SCTP is available
        // make a copy we may have incoming DataChannels mutating this while we open
        let data_channels = {
            let data_channels = self.sctp_transport.data_channels.lock().await;
            data_channels.clone()
        };

        let mut opened_dc_count = 0;
        for d in data_channels {
            if d.ready_state() == RTCDataChannelState::Connecting {
                if let Err(err) = d.open(Arc::clone(&self.sctp_transport)).await {
                    log::warn!("failed to open data channel: {}", err);
                    continue;
                }
                opened_dc_count += 1;
            }
        }

        self.sctp_transport
            .data_channels_opened
            .fetch_add(opened_dc_count, Ordering::SeqCst);
    }

    pub(crate) async fn remote_description(self: &Arc<Self>) -> Option<RTCSessionDescription> {
        let pending_remote_description = self.pending_remote_description.lock().await;
        if pending_remote_description.is_some() {
            pending_remote_description.clone()
        } else {
            let current_remote_description = self.current_remote_description.lock().await;
            current_remote_description.clone()
        }
    }

    /// Start all transports. PeerConnection now has enough state
    pub(crate) async fn start_transports(
        self: &Arc<Self>,
        ice_role: RTCIceRole,
        dtls_role: DTLSRole,
        remote_ufrag: String,
        remote_pwd: String,
    ) {
        // Start the ice transport
        if let Err(err) = self
            .ice_transport
            .start(
                &RTCIceParameters {
                    username_fragment: remote_ufrag,
                    password: remote_pwd,
                },
                Some(ice_role),
            )
            .await
        {
            log::warn!("Failed to start manager ice: {}", err);
            return;
        }

        // Start the dtls_transport transport
        let _ = dtls_role;
        let result = self.dtls_transport.start().await;
        RTCPeerConnection::update_connection_state(
            &self.on_peer_connection_state_change_handler,
            &self.is_closed,
            &self.peer_connection_state,
            self.ice_connection_state.load(Ordering::SeqCst).into(),
            self.dtls_transport.state(),
        )
        .await;
        if let Err(err) = result {
            log::warn!("Failed to start manager dtls: {}", err);
        }
    }

    pub(crate) async fn close(&self) {
        if self.is_closed.load(Ordering::SeqCst) {
            return;
        }

        self.is_closed.store(true, Ordering::SeqCst);

        // ICE
        self.ice_transport.close().await;

        // SCTP
        self.sctp_transport.stop().await.unwrap();

        // DTLS
        let mut dtls_conn = self.dtls_transport.conn.lock().await;
        if let Some(conn) = &mut *dtls_conn {
            conn.close().await.unwrap();
        }
    }

    /// generate_unmatched_sdp generates an SDP that doesn't take remote state into account
    /// This is used for the initial call for CreateOffer
    pub(crate) async fn generate_unmatched_sdp(
        &self,
        use_identity: bool,
    ) -> Result<SessionDescription> {
        let d = SessionDescription::new_jsep_session_description(use_identity);

        let ice_params = self.ice_gatherer.get_local_parameters().await?;

        let candidates = self.ice_gatherer.get_local_candidates().await?;

        let mut media_sections = vec![];

        if self
            .sctp_transport
            .data_channels_requested
            .load(Ordering::SeqCst)
            != 0
        {
            media_sections.push(MediaSection {
                id: format!("{}", media_sections.len()),
                data: true,
            });
        }

        let dtls_fingerprints = if let Some(cert) = self.dtls_transport.certificates.first() {
            cert.get_fingerprints()?
        } else {
            return Err(Error::ErrNonCertificate);
        };

        let params = PopulateSdpParams {
            is_icelite: false,
            connection_role: DEFAULT_DTLS_ROLE_OFFER.to_connection_role(),
            ice_gathering_state: self.ice_gathering_state(),
        };
        populate_sdp(
            d,
            &dtls_fingerprints,
            &candidates,
            &ice_params,
            &media_sections,
            params,
        )
        .await
    }

    /// generate_matched_sdp generates a SDP and takes the remote state into account
    /// this is used everytime we have a remote_description
    pub(crate) async fn generate_matched_sdp(
        &self,
        use_identity: bool,
        include_unmatched: bool,
        connection_role: ConnectionRole,
    ) -> Result<SessionDescription> {
        let d = SessionDescription::new_jsep_session_description(use_identity);

        let ice_params = self.ice_gatherer.get_local_parameters().await?;
        let candidates = self.ice_gatherer.get_local_candidates().await?;

        let remote_description = {
            let pending_remote_description = self.pending_remote_description.lock().await;
            if pending_remote_description.is_some() {
                pending_remote_description.clone()
            } else {
                let current_remote_description = self.current_remote_description.lock().await;
                current_remote_description.clone()
            }
        };

        let detected_plan_b = description_is_plan_b(remote_description.as_ref())?;
        let mut media_sections = vec![];

        // If we are offering also include unmatched local transceivers
        if include_unmatched
            && self
                .sctp_transport
                .data_channels_requested
                .load(Ordering::SeqCst)
                != 0
        {
            if detected_plan_b {
                media_sections.push(MediaSection {
                    id: "data".to_owned(),
                    data: true,
                });
            } else {
                media_sections.push(MediaSection {
                    id: format!("{}", media_sections.len()),
                    data: true,
                });
            }
        }

        let dtls_fingerprints = if let Some(cert) = self.dtls_transport.certificates.first() {
            cert.get_fingerprints()?
        } else {
            return Err(Error::ErrNonCertificate);
        };

        let params = PopulateSdpParams {
            is_icelite: false,
            connection_role,
            ice_gathering_state: self.ice_gathering_state(),
        };
        populate_sdp(
            d,
            &dtls_fingerprints,
            &candidates,
            &ice_params,
            &media_sections,
            params,
        )
        .await
    }

    pub(crate) fn ice_gathering_state(&self) -> RTCIceGatheringState {
        match self.ice_gatherer.state() {
            RTCIceGathererState::New => RTCIceGatheringState::New,
            RTCIceGathererState::Gathering => RTCIceGatheringState::Gathering,
            _ => RTCIceGatheringState::Complete,
        }
    }

    pub(crate) async fn create_ice_transport(&self) -> Arc<RTCIceTransport> {
        let ice_transport = Arc::new(API::new_ice_transport(Arc::clone(&self.ice_gatherer)));

        let ice_connection_state = Arc::clone(&self.ice_connection_state);
        let peer_connection_state = Arc::clone(&self.peer_connection_state);
        let is_closed = Arc::clone(&self.is_closed);
        let dtls_transport = Arc::clone(&self.dtls_transport);
        let on_ice_connection_state_change_handler =
            Arc::clone(&self.on_ice_connection_state_change_handler);
        let on_peer_connection_state_change_handler =
            Arc::clone(&self.on_peer_connection_state_change_handler);

        ice_transport
            .on_connection_state_change(Box::new(move |state: RTCIceTransportState| {
                let cs = match state {
                    RTCIceTransportState::New => RTCIceConnectionState::New,
                    RTCIceTransportState::Checking => RTCIceConnectionState::Checking,
                    RTCIceTransportState::Connected => RTCIceConnectionState::Connected,
                    RTCIceTransportState::Completed => RTCIceConnectionState::Completed,
                    RTCIceTransportState::Failed => RTCIceConnectionState::Failed,
                    RTCIceTransportState::Disconnected => RTCIceConnectionState::Disconnected,
                    RTCIceTransportState::Closed => RTCIceConnectionState::Closed,
                    _ => {
                        log::warn!("on_connection_state_change: unhandled ICE state: {}", state);
                        return Box::pin(async {});
                    }
                };

                let ice_connection_state2 = Arc::clone(&ice_connection_state);
                let on_ice_connection_state_change_handler2 =
                    Arc::clone(&on_ice_connection_state_change_handler);
                let on_peer_connection_state_change_handler2 =
                    Arc::clone(&on_peer_connection_state_change_handler);
                let is_closed2 = Arc::clone(&is_closed);
                let dtls_transport_state = dtls_transport.state();
                let peer_connection_state2 = Arc::clone(&peer_connection_state);
                Box::pin(async move {
                    RTCPeerConnection::do_ice_connection_state_change(
                        &on_ice_connection_state_change_handler2,
                        &ice_connection_state2,
                        cs,
                    )
                    .await;

                    RTCPeerConnection::update_connection_state(
                        &on_peer_connection_state_change_handler2,
                        &is_closed2,
                        &peer_connection_state2,
                        cs,
                        dtls_transport_state,
                    )
                    .await;
                })
            }))
            .await;

        ice_transport
    }
}
