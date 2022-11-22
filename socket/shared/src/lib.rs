//! # Naia Socket Shared
//! Common data types shared between Naia Server Socket & Naia Client Socket

#![deny(
    trivial_casts,
    trivial_numeric_casts,
    unstable_features,
    unused_import_braces,
    unused_qualifications
)]

#[macro_use]
extern crate cfg_if;

/// Logic shared between client & server sockets related to simulating network
/// conditions
pub mod link_condition_logic;

mod backends;
mod link_conditioner_config;
mod socket_config;
mod time_queue;
mod url_parse;

pub use backends::{Instant, Random};
pub use link_conditioner_config::LinkConditionerConfig;
pub use socket_config::SocketConfig;
pub use time_queue::TimeQueue;
pub use url_parse::{parse_server_url, url_to_socket_addr};

/// This enumeration is the list of the possible error outcomes for the `send` method of packet
/// senders.
#[derive(Debug, Eq, PartialEq)]
pub enum TrySendError<T> {
    /// The data could not be sent on the channel because the channel is
    /// currently full and sending would require blocking.
    Full(T),

    /// The receive half of the channel was explicitly closed or has been
    /// dropped.
    Closed(T),
}

impl<T: std::fmt::Debug> std::error::Error for TrySendError<T> {}

impl<T> std::fmt::Display for TrySendError<T> {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            fmt,
            "{}",
            match self {
                TrySendError::Full(..) => "no available capacity",
                TrySendError::Closed(..) => "channel closed",
            }
        )
    }
}

cfg_if! {
    if #[cfg(all(target_arch = "wasm32", feature = "wbindgen", feature = "mquad"))]
    {
        // Use both protocols...
        compile_error!("wasm target for 'naia_socket_shared' crate requires either the 'wbindgen' OR 'mquad' feature to be enabled, you must pick one.");
    }
    else if #[cfg(all(target_arch = "wasm32", not(feature = "wbindgen"), not(feature = "mquad")))]
    {
        // Use no protocols...
        compile_error!("wasm target for 'naia_socket_shared' crate requires either the 'wbindgen' or 'mquad' feature to be enabled, you must pick one.");
    }
}
