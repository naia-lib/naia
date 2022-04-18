//! # Naia Server Socket
//! Provides an abstraction of a Socket capable of sending/receiving to many
//! clients, using either an underlying UdpSocket or a service that can
//! communicate via unreliable WebRTC datachannels

#![deny(
    trivial_casts,
    trivial_numeric_casts,
    unstable_features,
    unused_import_braces,
    unused_qualifications
)]

extern crate log;

#[macro_use]
extern crate cfg_if;

mod backends;
mod conditioned_packet_receiver;
mod error;
mod io;
mod packet_receiver;
mod packet_sender;
mod server_addrs;
mod socket;

/// Executor for Server
pub mod executor;

pub use error::NaiaServerSocketError;
pub use naia_socket_shared as shared;
pub use packet_receiver::PacketReceiver;
pub use packet_sender::PacketSender;
pub use server_addrs::ServerAddrs;
pub use socket::Socket;

cfg_if! {
    if #[cfg(all(feature = "use-udp", feature = "use-webrtc"))]
    {
        // Use both protocols...
        compile_error!("Naia Server Socket can only use UDP or WebRTC, you must pick one");
    }
    else if #[cfg(all(not(feature = "use-udp"), not(feature = "use-webrtc")))]
    {
        // Use no protocols...
        compile_error!("Naia Server Socket requires either the 'use-udp' or 'use-webrtc' feature to be enabled, you must pick one.");
    }
}
