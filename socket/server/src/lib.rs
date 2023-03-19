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

mod async_socket;
mod conditioned_packet_receiver;
mod error;
mod packet_receiver;
mod packet_sender;
mod server_addrs;
mod session;
mod socket;
mod transport;

/// Executor for Server
pub mod executor;

pub use error::NaiaServerSocketError;
pub use naia_socket_shared as shared;
pub use packet_receiver::PacketReceiver;
pub use packet_sender::PacketSender;
pub use server_addrs::ServerAddrs;
pub use socket::Socket;
pub use transport::WebRTCSocket;
