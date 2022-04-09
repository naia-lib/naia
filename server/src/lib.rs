//! # Naia Server
//! A server that uses either UDP or WebRTC communication to send/receive
//! messages to/from connected clients, and syncs registered
//! Entities/Components to clients to whom they are in-scope.

#![deny(
    trivial_casts,
    trivial_numeric_casts,
    unstable_features,
    unused_import_braces
)]

extern crate cfg_if;

extern crate log;

#[cfg(all(feature = "use-udp", feature = "use-webrtc"))]
compile_error!("Naia Server can only use UDP or WebRTC, you must pick one");

#[cfg(all(not(feature = "use-udp"), not(feature = "use-webrtc")))]
compile_error!("Naia Server requires either the 'use-udp' or 'use-webrtc' feature to be enabled, you must pick one.");

pub use naia_server_socket::ServerAddrs;

pub use naia_shared as shared;

mod cache_map;
mod connection;
mod error;
mod event;
mod protocol;
mod room;
mod sequence_list;
mod server;
mod server_config;
mod tick;
mod user;
mod user_scope;

pub use error::NaiaServerError;
pub use event::Event;
pub use protocol::entity_ref::EntityRef;
pub use room::{RoomKey, RoomMut, RoomRef};
pub use server::Server;
pub use server_config::ServerConfig;
pub use user::{User, UserKey, UserMut, UserRef};
pub use user_scope::UserScopeMut;

pub mod internal {
    pub use crate::connection::handshake_manager::{HandshakeManager, HandshakeResult};
}
