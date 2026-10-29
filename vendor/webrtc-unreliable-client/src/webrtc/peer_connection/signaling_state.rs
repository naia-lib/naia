use crate::webrtc::error::{Error, Result};
use crate::webrtc::peer_connection::sdp::sdp_type::RTCSdpType;

use std::fmt;

#[derive(Debug, Copy, Clone, PartialEq, Default)]
pub(crate) enum StateChangeOp {
    #[default]
    SetLocal,
    SetRemote,
}

impl fmt::Display for StateChangeOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            StateChangeOp::SetLocal => write!(f, "SetLocal"),
            StateChangeOp::SetRemote => write!(f, "SetRemote"),
            //_ => write!(f, UNSPECIFIED_STR),
        }
    }
}

/// SignalingState indicates the signaling state of the offer/answer process.
#[derive(Debug, Copy, Clone, PartialEq, Default)]
pub(crate) enum RTCSignalingState {
    #[default]
    Unspecified = 0,

    /// SignalingStateStable indicates there is no offer/answer exchange in
    /// progress. This is also the initial state, in which case the local and
    /// remote descriptions are nil.
    Stable,

    /// SignalingStateHaveLocalOffer indicates that a local description, of
    /// type "offer", has been successfully applied.
    HaveLocalOffer,

    /// SignalingStateHaveRemoteOffer indicates that a remote description, of
    /// type "offer", has been successfully applied.
    HaveRemoteOffer,

    /// SignalingStateHaveLocalPranswer indicates that a remote description
    /// of type "offer" has been successfully applied and a local description
    /// of type "pranswer" has been successfully applied.
    HaveLocalPranswer,

    /// SignalingStateHaveRemotePranswer indicates that a local description
    /// of type "offer" has been successfully applied and a remote description
    /// of type "pranswer" has been successfully applied.
    HaveRemotePranswer,

    /// SignalingStateClosed indicates The PeerConnection has been closed.
    Closed,
}

const SIGNALING_STATE_STABLE_STR: &str = "stable";
const SIGNALING_STATE_HAVE_LOCAL_OFFER_STR: &str = "have-local-offer";
const SIGNALING_STATE_HAVE_REMOTE_OFFER_STR: &str = "have-remote-offer";
const SIGNALING_STATE_HAVE_LOCAL_PRANSWER_STR: &str = "have-local-pranswer";
const SIGNALING_STATE_HAVE_REMOTE_PRANSWER_STR: &str = "have-remote-pranswer";
const SIGNALING_STATE_CLOSED_STR: &str = "closed";

impl From<&str> for RTCSignalingState {
    fn from(raw: &str) -> Self {
        match raw {
            SIGNALING_STATE_STABLE_STR => RTCSignalingState::Stable,
            SIGNALING_STATE_HAVE_LOCAL_OFFER_STR => RTCSignalingState::HaveLocalOffer,
            SIGNALING_STATE_HAVE_REMOTE_OFFER_STR => RTCSignalingState::HaveRemoteOffer,
            SIGNALING_STATE_HAVE_LOCAL_PRANSWER_STR => RTCSignalingState::HaveLocalPranswer,
            SIGNALING_STATE_HAVE_REMOTE_PRANSWER_STR => RTCSignalingState::HaveRemotePranswer,
            SIGNALING_STATE_CLOSED_STR => RTCSignalingState::Closed,
            _ => RTCSignalingState::Unspecified,
        }
    }
}

impl fmt::Display for RTCSignalingState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            RTCSignalingState::Stable => write!(f, "{}", SIGNALING_STATE_STABLE_STR),
            RTCSignalingState::HaveLocalOffer => {
                write!(f, "{}", SIGNALING_STATE_HAVE_LOCAL_OFFER_STR)
            }
            RTCSignalingState::HaveRemoteOffer => {
                write!(f, "{}", SIGNALING_STATE_HAVE_REMOTE_OFFER_STR)
            }
            RTCSignalingState::HaveLocalPranswer => {
                write!(f, "{}", SIGNALING_STATE_HAVE_LOCAL_PRANSWER_STR)
            }
            RTCSignalingState::HaveRemotePranswer => {
                write!(f, "{}", SIGNALING_STATE_HAVE_REMOTE_PRANSWER_STR)
            }
            RTCSignalingState::Closed => write!(f, "{}", SIGNALING_STATE_CLOSED_STR),
            _ => write!(f, "{}", crate::webrtc::UNSPECIFIED_STR),
        }
    }
}

impl From<u8> for RTCSignalingState {
    fn from(v: u8) -> Self {
        match v {
            1 => RTCSignalingState::Stable,
            2 => RTCSignalingState::HaveLocalOffer,
            3 => RTCSignalingState::HaveRemoteOffer,
            4 => RTCSignalingState::HaveLocalPranswer,
            5 => RTCSignalingState::HaveRemotePranswer,
            6 => RTCSignalingState::Closed,
            _ => RTCSignalingState::Unspecified,
        }
    }
}

pub(crate) fn check_next_signaling_state(
    cur: RTCSignalingState,
    next: RTCSignalingState,
    op: StateChangeOp,
    sdp_type: RTCSdpType,
) -> Result<RTCSignalingState> {
    // Special case for rollbacks
    if sdp_type == RTCSdpType::Rollback && cur == RTCSignalingState::Stable {
        return Err(Error::ErrSignalingStateCannotRollback);
    }

    // 4.3.1 valid state transitions
    match cur {
        RTCSignalingState::Stable => {
            match op {
                StateChangeOp::SetLocal => {
                    // stable->SetLocal(offer)->have-local-offer
                    if sdp_type == RTCSdpType::Offer && next == RTCSignalingState::HaveLocalOffer {
                        return Ok(next);
                    }
                }
                StateChangeOp::SetRemote => {
                    // stable->SetRemote(offer)->have-remote-offer
                    if sdp_type == RTCSdpType::Offer && next == RTCSignalingState::HaveRemoteOffer {
                        return Ok(next);
                    }
                }
            }
        }
        RTCSignalingState::HaveLocalOffer => {
            if op == StateChangeOp::SetRemote {
                match sdp_type {
                    // have-local-offer->SetRemote(answer)->stable
                    RTCSdpType::Answer => {
                        if next == RTCSignalingState::Stable {
                            return Ok(next);
                        }
                    }
                    // have-local-offer->SetRemote(pranswer)->have-remote-pranswer
                    RTCSdpType::Pranswer if next == RTCSignalingState::HaveRemotePranswer => {
                        return Ok(next);
                    }
                    _ => {}
                }
            }
        }
        RTCSignalingState::HaveRemotePranswer => {
            if op == StateChangeOp::SetRemote && sdp_type == RTCSdpType::Answer {
                // have-remote-pranswer->SetRemote(answer)->stable
                if next == RTCSignalingState::Stable {
                    return Ok(next);
                }
            }
        }
        RTCSignalingState::HaveRemoteOffer => {
            if op == StateChangeOp::SetLocal {
                match sdp_type {
                    // have-remote-offer->SetLocal(answer)->stable
                    RTCSdpType::Answer => {
                        if next == RTCSignalingState::Stable {
                            return Ok(next);
                        }
                    }
                    // have-remote-offer->SetLocal(pranswer)->have-local-pranswer
                    RTCSdpType::Pranswer if next == RTCSignalingState::HaveLocalPranswer => {
                        return Ok(next);
                    }
                    _ => {}
                }
            }
        }
        RTCSignalingState::HaveLocalPranswer => {
            if op == StateChangeOp::SetLocal && sdp_type == RTCSdpType::Answer {
                // have-local-pranswer->SetLocal(answer)->stable
                if next == RTCSignalingState::Stable {
                    return Ok(next);
                }
            }
        }
        _ => {
            return Err(Error::ErrSignalingStateProposedTransitionInvalid);
        }
    };

    Err(Error::ErrSignalingStateProposedTransitionInvalid)
}
