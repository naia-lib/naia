use crate::webrtc::ice::candidate::CandidateType;
use std::fmt;

/// ICECandidateType represents the type of the ICE candidate used.
#[derive(Debug, Copy, Clone, PartialEq, Default)]
pub(crate) enum RTCIceCandidateType {
    #[default]
    Unspecified,

    /// ICECandidateTypeHost indicates that the candidate is of Host type as
    /// described in <https://tools.ietf.org/html/rfc8445#section-5.1.1.1>. A
    /// candidate obtained by binding to a specific port from an IP address on
    /// the host. This includes IP addresses on physical interfaces and logical
    /// ones, such as ones obtained through VPNs.
    Host,
}

const ICE_CANDIDATE_TYPE_HOST_STR: &str = "host";

///  takes a string and converts it into ICECandidateType
impl From<&str> for RTCIceCandidateType {
    fn from(raw: &str) -> Self {
        match raw {
            ICE_CANDIDATE_TYPE_HOST_STR => RTCIceCandidateType::Host,
            _ => RTCIceCandidateType::Unspecified,
        }
    }
}

impl From<CandidateType> for RTCIceCandidateType {
    fn from(candidate_type: CandidateType) -> Self {
        match candidate_type {
            CandidateType::Host => RTCIceCandidateType::Host,
            _ => RTCIceCandidateType::Unspecified,
        }
    }
}

impl fmt::Display for RTCIceCandidateType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            RTCIceCandidateType::Host => write!(f, "{}", ICE_CANDIDATE_TYPE_HOST_STR),
            _ => write!(f, "{}", crate::webrtc::UNSPECIFIED_STR),
        }
    }
}
