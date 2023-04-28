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

#[macro_use]
extern crate cfg_if;

pub mod transport;
pub mod shared {
    pub use naia_shared::{default_channels, Random, SocketConfig};
}
pub mod internal {
    pub use crate::connection::handshake_manager::{HandshakeManager, HandshakeResult};
}

mod cache_map;
mod connection;
mod error;
mod events;
mod room;
mod server;
mod server_config;
mod time_manager;
mod user;
mod user_scope;
mod world;

pub use connection::tick_buffer_messages::TickBufferMessages;
pub use error::NaiaServerError;
pub use events::{
    AuthEvent, ConnectEvent, DespawnEntityEvent, DisconnectEvent, ErrorEvent, Events,
    InsertComponentEvent, MessageEvent, RemoveComponentEvent, SpawnEntityEvent, TickEvent,
    UpdateComponentEvent, PublishEntityEvent,
};
pub use room::{RoomKey, RoomMut, RoomRef};
pub use server::Server;
pub use server_config::ServerConfig;
pub use user::{User, UserKey, UserMut, UserRef};
pub use user_scope::UserScopeMut;
pub use world::entity_mut::EntityMut;
pub use world::entity_owner::EntityOwner;
