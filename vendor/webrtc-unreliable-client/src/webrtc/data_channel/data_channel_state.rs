use std::fmt;

/// DataChannelState indicates the state of a data channel.
#[derive(Debug, Copy, Clone, PartialEq, Default)]
pub(crate) enum RTCDataChannelState {
    #[default]
    Unspecified = 0,

    /// DataChannelStateConnecting indicates that the data channel is being
    /// established. This is the initial state of DataChannel, whether created
    /// with create_data_channel, or dispatched as a part of an DataChannelEvent.
    Connecting,

    /// DataChannelStateOpen indicates that the underlying data transport is
    /// established and communication is possible.
    Open,

    /// DataChannelStateClosing indicates that the procedure to close down the
    /// underlying data transport has started.
    Closing,

    /// DataChannelStateClosed indicates that the underlying data transport
    /// has been closed or could not be established.
    Closed,
}

const DATA_CHANNEL_STATE_CONNECTING_STR: &str = "connecting";
const DATA_CHANNEL_STATE_OPEN_STR: &str = "open";
const DATA_CHANNEL_STATE_CLOSING_STR: &str = "closing";
const DATA_CHANNEL_STATE_CLOSED_STR: &str = "closed";

impl From<u8> for RTCDataChannelState {
    fn from(v: u8) -> Self {
        match v {
            1 => RTCDataChannelState::Connecting,
            2 => RTCDataChannelState::Open,
            3 => RTCDataChannelState::Closing,
            4 => RTCDataChannelState::Closed,
            _ => RTCDataChannelState::Unspecified,
        }
    }
}

impl From<&str> for RTCDataChannelState {
    fn from(raw: &str) -> Self {
        match raw {
            DATA_CHANNEL_STATE_CONNECTING_STR => RTCDataChannelState::Connecting,
            DATA_CHANNEL_STATE_OPEN_STR => RTCDataChannelState::Open,
            DATA_CHANNEL_STATE_CLOSING_STR => RTCDataChannelState::Closing,
            DATA_CHANNEL_STATE_CLOSED_STR => RTCDataChannelState::Closed,
            _ => RTCDataChannelState::Unspecified,
        }
    }
}

impl fmt::Display for RTCDataChannelState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            RTCDataChannelState::Connecting => DATA_CHANNEL_STATE_CONNECTING_STR,
            RTCDataChannelState::Open => DATA_CHANNEL_STATE_OPEN_STR,
            RTCDataChannelState::Closing => DATA_CHANNEL_STATE_CLOSING_STR,
            RTCDataChannelState::Closed => DATA_CHANNEL_STATE_CLOSED_STR,
            RTCDataChannelState::Unspecified => crate::webrtc::UNSPECIFIED_STR,
        };
        write!(f, "{}", s)
    }
}
