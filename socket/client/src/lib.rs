//! # Naia Client Socket
//! A Socket abstraction over either a UDP socket on native Linux, or a
//! unreliable WebRTC datachannel on the browser

#![deny(unstable_features, unused_import_braces, unused_qualifications)]

extern crate log;

#[macro_use]
extern crate cfg_if;

cfg_if! {
    if #[cfg(target_arch = "wasm32")] {
        mod wasm_utils;
    } else {}
}

mod backends;
mod conditioned_packet_receiver;
mod error;
mod io;
mod packet_receiver;
mod server_addr;

pub use naia_socket_shared as shared;

pub use backends::*;
pub use error::NaiaClientSocketError;
pub use packet_receiver::PacketReceiver;
pub use server_addr::ServerAddr;

cfg_if! {
    if #[cfg(all(target_arch = "wasm32", feature = "wbindgen", feature = "mquad"))]
    {
        // Use both protocols...
        compile_error!("Naia Client Socket on Wasm requires either the 'wbindgen' OR 'mquad' feature to be enabled, you must pick one.");
    }
    else if #[cfg(all(target_arch = "wasm32", not(feature = "wbindgen"), not(feature = "mquad")))]
    {
        // Use no protocols...
        compile_error!("Naia Client Socket on Wasm requires either the 'wbindgen' or 'mquad' feature to be enabled, you must pick one.");
    }
}
