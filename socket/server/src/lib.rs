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

#[macro_use]
extern crate cfg_if;

extern crate log;

mod error;
mod io;
mod transports;
mod packet_receiver;
mod packet_sender;
mod conditioned_packet_receiver;

pub use error::NaiaServerSocketError;
pub use naia_socket_shared as shared;
pub use packet_receiver::PacketReceiver;
pub use packet_sender::PacketSender;


cfg_if! {
    if #[cfg(feature = "webrtc")] {
        pub use transports::webrtc::*;
    } else {
        compile_error!("a transport among ['webrtc'] must be enabled.");
    }
}

