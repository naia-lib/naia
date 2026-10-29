use std::fmt;

/// ConnectionRole indicates which of the end points should initiate the connection establishment
#[derive(Debug, Copy, Clone, PartialEq, Default)]
pub(crate) enum ConnectionRole {
    #[default]
    Unspecified,

    /// ConnectionRoleActive indicates the endpoint will initiate an outgoing connection.
    Active,

    /// ConnectionRolePassive indicates the endpoint will accept an incoming connection.
    Passive,

    /// ConnectionRoleActpass indicates the endpoint is willing to accept an incoming connection or to initiate an outgoing connection.
    Actpass,

    /// ConnectionRoleHoldconn indicates the endpoint does not want the connection to be established for the time being.
    Holdconn,
}

const CONNECTION_ROLE_ACTIVE_STR: &str = "active";
const CONNECTION_ROLE_PASSIVE_STR: &str = "passive";
const CONNECTION_ROLE_ACTPASS_STR: &str = "actpass";
const CONNECTION_ROLE_HOLDCONN_STR: &str = "holdconn";

impl fmt::Display for ConnectionRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            ConnectionRole::Active => CONNECTION_ROLE_ACTIVE_STR,
            ConnectionRole::Passive => CONNECTION_ROLE_PASSIVE_STR,
            ConnectionRole::Actpass => CONNECTION_ROLE_ACTPASS_STR,
            ConnectionRole::Holdconn => CONNECTION_ROLE_HOLDCONN_STR,
            _ => "Unspecified",
        };
        write!(f, "{}", s)
    }
}

impl From<u8> for ConnectionRole {
    fn from(v: u8) -> Self {
        match v {
            1 => ConnectionRole::Active,
            2 => ConnectionRole::Passive,
            3 => ConnectionRole::Actpass,
            4 => ConnectionRole::Holdconn,
            _ => ConnectionRole::Unspecified,
        }
    }
}

impl From<&str> for ConnectionRole {
    fn from(raw: &str) -> Self {
        match raw {
            CONNECTION_ROLE_ACTIVE_STR => ConnectionRole::Active,
            CONNECTION_ROLE_PASSIVE_STR => ConnectionRole::Passive,
            CONNECTION_ROLE_ACTPASS_STR => ConnectionRole::Actpass,
            CONNECTION_ROLE_HOLDCONN_STR => ConnectionRole::Holdconn,
            _ => ConnectionRole::Unspecified,
        }
    }
}

/// https://tools.ietf.org/html/draft-ietf-rtcweb-jsep-26#section-5.2.1
/// Session ID is recommended to be constructed by generating a 64-bit
/// quantity with the highest bit set to zero and the remaining 63-bits
/// being cryptographically random.
pub(crate) fn new_session_id() -> u64 {
    let c = u64::MAX ^ (1u64 << 63);
    rand::random::<u64>() & c
}
