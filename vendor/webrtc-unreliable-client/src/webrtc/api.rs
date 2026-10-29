use crate::webrtc::dtls_transport::RTCDtlsTransport;
use crate::webrtc::ice_transport::ice_gatherer::RTCIceGatherer;
use crate::webrtc::ice_transport::RTCIceTransport;
use crate::webrtc::peer_connection::certificate::RTCCertificate;

use crate::webrtc::error::Result;
use crate::webrtc::sctp_transport::RTCSctpTransport;

use rcgen::KeyPair;
use std::sync::Arc;

/// API bundles the global functions of the WebRTC and ORTC API.
/// Some of these functions are also exported globally using the
/// defaultAPI object. Note that the global version of the API
/// may be phased out in the future.
#[allow(clippy::upper_case_acronyms)]
pub(crate) struct API;

impl API {
    /// new_ice_gatherer creates a new ice gatherer.
    /// This constructor is part of the ORTC API. It is not
    /// meant to be used together with the basic WebRTC API.
    pub(crate) fn new_ice_gatherer() -> Result<RTCIceGatherer> {
        Ok(RTCIceGatherer::new())
    }

    /// new_ice_transport creates a new ice transport.
    /// This constructor is part of the ORTC API. It is not
    /// meant to be used together with the basic WebRTC API.
    pub(crate) fn new_ice_transport(gatherer: Arc<RTCIceGatherer>) -> RTCIceTransport {
        RTCIceTransport::new(gatherer)
    }

    /// new_dtls_transport creates a new dtls_transport transport.
    /// This constructor is part of the ORTC API. It is not
    /// meant to be used together with the basic WebRTC API.
    pub(crate) fn new_dtls_transport(
        ice_transport: Arc<RTCIceTransport>,
    ) -> Result<RTCDtlsTransport> {
        let kp = KeyPair::generate(&rcgen::PKCS_ECDSA_P256_SHA256)?;
        let cert = RTCCertificate::from_key_pair(kp)?;
        let certificates = vec![cert];

        Ok(RTCDtlsTransport::new(ice_transport, certificates))
    }

    /// new_sctp_transport creates a new SCTPTransport.
    /// This constructor is part of the ORTC API. It is not
    /// meant to be used together with the basic WebRTC API.
    pub(crate) fn new_sctp_transport(
        dtls_transport: Arc<RTCDtlsTransport>,
    ) -> Result<RTCSctpTransport> {
        Ok(RTCSctpTransport::new(dtls_transport))
    }
}
