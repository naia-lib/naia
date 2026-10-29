use std::fmt;

/// ICERole describes the role ice.Agent is playing in selecting the
/// preferred the candidate pair.
#[derive(Debug, Copy, Clone, PartialEq, Default)]
pub(crate) enum RTCIceRole {
    #[default]
    Unspecified,

    /// ICERoleControlling indicates that the ICE agent that is responsible
    /// for selecting the final choice of candidate pairs and signaling them
    /// through STUN and an updated offer, if needed. In any session, one agent
    /// is always controlling. The other is the controlled agent.
    Controlling,

    /// ICERoleControlled indicates that an ICE agent that waits for the
    /// controlling agent to select the final choice of candidate pairs.
    Controlled,
}

const ICE_ROLE_CONTROLLING_STR: &str = "controlling";
const ICE_ROLE_CONTROLLED_STR: &str = "controlled";

impl From<&str> for RTCIceRole {
    fn from(raw: &str) -> Self {
        match raw {
            ICE_ROLE_CONTROLLING_STR => RTCIceRole::Controlling,
            ICE_ROLE_CONTROLLED_STR => RTCIceRole::Controlled,
            _ => RTCIceRole::Unspecified,
        }
    }
}

impl fmt::Display for RTCIceRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            RTCIceRole::Controlling => write!(f, "{}", ICE_ROLE_CONTROLLING_STR),
            RTCIceRole::Controlled => write!(f, "{}", ICE_ROLE_CONTROLLED_STR),
            _ => write!(f, "{}", crate::webrtc::UNSPECIFIED_STR),
        }
    }
}
