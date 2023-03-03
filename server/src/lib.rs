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

pub use naia_server_socket::ServerAddrs;

pub use naia_shared::{
    default_channels, DespawnEntityEvent, EntityRef, InsertComponentEvent, Random,
    RemoveComponentEvent, SpawnEntityEvent, UpdateComponentEvent,
};

mod cache_map;
mod connection;
mod entity_mut;
mod entity_scope_map;
mod error;
mod events;
mod room;
mod server;
mod server_config;
mod user;
mod user_scope;

pub use connection::tick_buffer_messages::TickBufferMessages;
pub use entity_mut::EntityMut;
pub use error::NaiaServerError;
pub use events::{
    AuthEvent, ConnectEvent, DisconnectEvent, ErrorEvent, Events, MessageEvent, TickEvent,
};
pub use room::{RoomKey, RoomMut, RoomRef};
pub use server::Server;
pub use server_config::ServerConfig;
pub use user::{User, UserKey, UserMut, UserRef};
pub use user_scope::UserScopeMut;

pub mod internal {
    pub use crate::connection::handshake_manager::{HandshakeManager, HandshakeResult};
}
