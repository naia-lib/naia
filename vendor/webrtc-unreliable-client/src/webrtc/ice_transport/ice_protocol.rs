use std::fmt;

/// ICEProtocol indicates the transport protocol type that is used in the
/// ice.URL structure.
#[derive(Debug, Copy, Clone, PartialEq, Default)]
pub(crate) enum RTCIceProtocol {
    #[default]
    Unspecified,

    /// UDP indicates the URL uses a UDP transport.
    Udp,

    /// TCP indicates the URL uses a TCP transport.
    Tcp,
}

const ICE_PROTOCOL_UDP_STR: &str = "udp";
const ICE_PROTOCOL_TCP_STR: &str = "tcp";

/// takes a string and converts it to ICEProtocol
impl From<&str> for RTCIceProtocol {
    fn from(raw: &str) -> Self {
        if raw.to_uppercase() == ICE_PROTOCOL_UDP_STR.to_uppercase() {
            RTCIceProtocol::Udp
        } else if raw.to_uppercase() == ICE_PROTOCOL_TCP_STR.to_uppercase() {
            RTCIceProtocol::Tcp
        } else {
            RTCIceProtocol::Unspecified
        }
    }
}

impl fmt::Display for RTCIceProtocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            RTCIceProtocol::Udp => write!(f, "{}", ICE_PROTOCOL_UDP_STR),
            RTCIceProtocol::Tcp => write!(f, "{}", ICE_PROTOCOL_TCP_STR),
            _ => write!(f, "{}", crate::webrtc::UNSPECIFIED_STR),
        }
    }
}
