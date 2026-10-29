use std::fmt;

/// ICEGatheringState describes the state of the candidate gathering process.
#[derive(Debug, Copy, Clone, PartialEq, Default)]
pub(crate) enum RTCIceGatheringState {
    #[default]
    Unspecified,

    /// ICEGatheringStateNew indicates that any of the ICETransports are
    /// in the "new" gathering state and none of the transports are in the
    /// "gathering" state, or there are no transports.
    New,

    /// ICEGatheringStateGathering indicates that any of the ICETransports
    /// are in the "gathering" state.
    Gathering,

    /// ICEGatheringStateComplete indicates that at least one ICETransport
    /// exists, and all ICETransports are in the "completed" gathering state.
    Complete,
}

const ICE_GATHERING_STATE_NEW_STR: &str = "new";
const ICE_GATHERING_STATE_GATHERING_STR: &str = "gathering";
const ICE_GATHERING_STATE_COMPLETE_STR: &str = "complete";

/// takes a string and converts it to ICEGatheringState
impl From<&str> for RTCIceGatheringState {
    fn from(raw: &str) -> Self {
        match raw {
            ICE_GATHERING_STATE_NEW_STR => RTCIceGatheringState::New,
            ICE_GATHERING_STATE_GATHERING_STR => RTCIceGatheringState::Gathering,
            ICE_GATHERING_STATE_COMPLETE_STR => RTCIceGatheringState::Complete,
            _ => RTCIceGatheringState::Unspecified,
        }
    }
}

impl fmt::Display for RTCIceGatheringState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            RTCIceGatheringState::New => write!(f, "{}", ICE_GATHERING_STATE_NEW_STR),
            RTCIceGatheringState::Gathering => write!(f, "{}", ICE_GATHERING_STATE_GATHERING_STR),
            RTCIceGatheringState::Complete => {
                write!(f, "{}", ICE_GATHERING_STATE_COMPLETE_STR)
            }
            _ => write!(f, "{}", crate::webrtc::UNSPECIFIED_STR),
        }
    }
}
