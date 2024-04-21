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
    pub use naia_shared::{
        default_channels, BigMap, BigMapKey, BitReader, BitWrite, BitWriter, ConstBitLength,
        FileBitWriter, GlobalResponseId, Random, ResponseReceiveKey, Serde, SerdeErr,
        SignedInteger, SignedVariableInteger, SocketConfig, UnsignedInteger,
        UnsignedVariableInteger,
    };
}
pub mod internal {
    pub use crate::connection::handshake_manager::HandshakeManager;
}

pub use naia_shared::SerdeBevyServer as SerdeBevy;

mod cache_map;
mod connection;
mod error;
mod events;
mod request;
mod room;
mod server;
mod server_config;
mod time_manager;
mod user;
mod user_scope;
mod world;
mod handshake;

pub use connection::tick_buffer_messages::TickBufferMessages;
pub use error::NaiaServerError;
pub use events::{
    AuthEvent, ConnectEvent, DelegateEntityEvent, DespawnEntityEvent, DisconnectEvent,
    EntityAuthGrantEvent, EntityAuthResetEvent, ErrorEvent, Events, InsertComponentEvent,
    MessageEvent, PublishEntityEvent, RemoveComponentEvent, RequestEvent, SpawnEntityEvent,
    TickEvent, UnpublishEntityEvent, UpdateComponentEvent,
};
pub use room::{RoomKey, RoomMut, RoomRef};
pub use server::Server;
pub use server_config::ServerConfig;
pub use user::{User, UserKey, UserMut, UserRef};
pub use user_scope::{UserScopeMut, UserScopeRef};
pub use world::{
    entity_mut::EntityMut, entity_owner::EntityOwner, replication_config::ReplicationConfig,
};
