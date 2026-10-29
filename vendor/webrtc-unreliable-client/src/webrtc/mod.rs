mod crates;

// re-export sub-crates
pub(crate) use crates::dtls;
pub(crate) use crates::ice;
pub(crate) use crates::sctp;
pub(crate) use crates::sdp;
pub(crate) use crates::stun;
pub(crate) use crates::util;
pub(crate) use data_channel::internal;

pub(crate) mod api;
pub(crate) mod data_channel;
pub(crate) mod dtls_transport;
pub(crate) mod error;
pub(crate) mod ice_transport;
pub(crate) mod mux;
pub(crate) mod peer_connection;
pub(crate) mod sctp_transport;

pub(crate) const UNSPECIFIED_STR: &str = "Unspecified";

/// Equal to UDP MTU
pub(crate) const RECEIVE_MTU: usize = 1460;
