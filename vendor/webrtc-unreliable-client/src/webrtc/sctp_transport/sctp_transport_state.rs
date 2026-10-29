use std::fmt;

/// SCTPTransportState indicates the state of the SCTP transport.
#[derive(Debug, Copy, Clone, PartialEq, Default)]
pub(crate) enum RTCSctpTransportState {
    #[default]
    Unspecified,

    /// SCTPTransportStateConnecting indicates the SCTPTransport is in the
    /// process of negotiating an association. This is the initial state of the
    /// SCTPTransportState when an SCTPTransport is created.
    Connecting,

    /// SCTPTransportStateConnected indicates the negotiation of an
    /// association is completed.
    Connected,

    /// SCTPTransportStateClosed indicates a SHUTDOWN or ABORT chunk is
    /// received or when the SCTP association has been closed intentionally,
    /// such as by closing the peer connection or applying a remote description
    /// that rejects data or changes the SCTP port.
    Closed,
}

const SCTP_TRANSPORT_STATE_CONNECTING_STR: &str = "connecting";
const SCTP_TRANSPORT_STATE_CONNECTED_STR: &str = "connected";
const SCTP_TRANSPORT_STATE_CLOSED_STR: &str = "closed";

impl From<&str> for RTCSctpTransportState {
    fn from(raw: &str) -> Self {
        match raw {
            SCTP_TRANSPORT_STATE_CONNECTING_STR => RTCSctpTransportState::Connecting,
            SCTP_TRANSPORT_STATE_CONNECTED_STR => RTCSctpTransportState::Connected,
            SCTP_TRANSPORT_STATE_CLOSED_STR => RTCSctpTransportState::Closed,
            _ => RTCSctpTransportState::Unspecified,
        }
    }
}

impl From<u8> for RTCSctpTransportState {
    fn from(v: u8) -> Self {
        match v {
            1 => RTCSctpTransportState::Connecting,
            2 => RTCSctpTransportState::Connected,
            3 => RTCSctpTransportState::Closed,
            _ => RTCSctpTransportState::Unspecified,
        }
    }
}

impl fmt::Display for RTCSctpTransportState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            RTCSctpTransportState::Connecting => SCTP_TRANSPORT_STATE_CONNECTING_STR,
            RTCSctpTransportState::Connected => SCTP_TRANSPORT_STATE_CONNECTED_STR,
            RTCSctpTransportState::Closed => SCTP_TRANSPORT_STATE_CLOSED_STR,
            RTCSctpTransportState::Unspecified => crate::webrtc::UNSPECIFIED_STR,
        };
        write!(f, "{}", s)
    }
}
