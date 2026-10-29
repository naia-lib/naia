use std::fmt;

/// PeerConnectionState indicates the state of the PeerConnection.
#[derive(Debug, Copy, Clone, PartialEq, Default)]
pub(crate) enum RTCPeerConnectionState {
    #[default]
    Unspecified,

    /// PeerConnectionStateNew indicates that any of the ICETransports or
    /// DTLSTransports are in the "new" state and none of the transports are
    /// in the "connecting", "checking", "failed" or "disconnected" state, or
    /// all transports are in the "closed" state, or there are no transports.
    New,

    /// PeerConnectionStateConnecting indicates that any of the
    /// ICETransports or DTLSTransports are in the "connecting" or
    /// "checking" state and none of them is in the "failed" state.
    Connecting,

    /// PeerConnectionStateConnected indicates that all ICETransports and
    /// DTLSTransports are in the "connected", "completed" or "closed" state
    /// and at least one of them is in the "connected" or "completed" state.
    Connected,

    /// PeerConnectionStateDisconnected indicates that any of the
    /// ICETransports or DTLSTransports are in the "disconnected" state
    /// and none of them are in the "failed" or "connecting" or "checking" state.
    Disconnected,

    /// PeerConnectionStateFailed indicates that any of the ICETransports
    /// or DTLSTransports are in a "failed" state.
    Failed,

    /// PeerConnectionStateClosed indicates the peer connection is closed
    /// and the isClosed member variable of PeerConnection is true.
    Closed,
}

const PEER_CONNECTION_STATE_NEW_STR: &str = "new";
const PEER_CONNECTION_STATE_CONNECTING_STR: &str = "connecting";
const PEER_CONNECTION_STATE_CONNECTED_STR: &str = "connected";
const PEER_CONNECTION_STATE_DISCONNECTED_STR: &str = "disconnected";
const PEER_CONNECTION_STATE_FAILED_STR: &str = "failed";
const PEER_CONNECTION_STATE_CLOSED_STR: &str = "closed";

impl From<&str> for RTCPeerConnectionState {
    fn from(raw: &str) -> Self {
        match raw {
            PEER_CONNECTION_STATE_NEW_STR => RTCPeerConnectionState::New,
            PEER_CONNECTION_STATE_CONNECTING_STR => RTCPeerConnectionState::Connecting,
            PEER_CONNECTION_STATE_CONNECTED_STR => RTCPeerConnectionState::Connected,
            PEER_CONNECTION_STATE_DISCONNECTED_STR => RTCPeerConnectionState::Disconnected,
            PEER_CONNECTION_STATE_FAILED_STR => RTCPeerConnectionState::Failed,
            PEER_CONNECTION_STATE_CLOSED_STR => RTCPeerConnectionState::Closed,
            _ => RTCPeerConnectionState::Unspecified,
        }
    }
}

impl From<u8> for RTCPeerConnectionState {
    fn from(v: u8) -> Self {
        match v {
            1 => RTCPeerConnectionState::New,
            2 => RTCPeerConnectionState::Connecting,
            3 => RTCPeerConnectionState::Connected,
            4 => RTCPeerConnectionState::Disconnected,
            5 => RTCPeerConnectionState::Failed,
            6 => RTCPeerConnectionState::Closed,
            _ => RTCPeerConnectionState::Unspecified,
        }
    }
}

impl fmt::Display for RTCPeerConnectionState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            RTCPeerConnectionState::New => PEER_CONNECTION_STATE_NEW_STR,
            RTCPeerConnectionState::Connecting => PEER_CONNECTION_STATE_CONNECTING_STR,
            RTCPeerConnectionState::Connected => PEER_CONNECTION_STATE_CONNECTED_STR,
            RTCPeerConnectionState::Disconnected => PEER_CONNECTION_STATE_DISCONNECTED_STR,
            RTCPeerConnectionState::Failed => PEER_CONNECTION_STATE_FAILED_STR,
            RTCPeerConnectionState::Closed => PEER_CONNECTION_STATE_CLOSED_STR,
            RTCPeerConnectionState::Unspecified => crate::webrtc::UNSPECIFIED_STR,
        };
        write!(f, "{}", s)
    }
}
