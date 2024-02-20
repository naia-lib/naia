//! # Naia Client
//! A cross-platform client that can send/receive messages to/from a server, and
//! has a pool of in-scope Entities/Components that are synced with the
//! server.

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
        default_channels, sequence_greater_than, GlobalRequestId, GlobalResponseId, Instant,
        Message, Protocol, Random, ResponseReceiveKey, SocketConfig, Tick,
    };
}
pub mod internal {
    pub use crate::connection::handshake_manager::{HandshakeManager, HandshakeState};
}

mod client;
mod client_config;
mod command_history;
mod connection;
mod error;
mod events;
mod request;
mod world;

pub use client::{Client, ConnectionStatus};
pub use client_config::ClientConfig;
pub use command_history::CommandHistory;
pub use error::NaiaClientError;
pub use events::{
    ClientTickEvent, ConnectEvent, DespawnEntityEvent, DisconnectEvent, EntityAuthDeniedEvent,
    EntityAuthGrantedEvent, EntityAuthResetEvent, ErrorEvent, Events, InsertComponentEvent,
    MessageEvent, PublishEntityEvent, RejectEvent, RemoveComponentEvent, RequestEvent,
    ServerTickEvent, SpawnEntityEvent, UnpublishEntityEvent, UpdateComponentEvent,
};
pub use world::{
    entity_mut::EntityMut, entity_ref::EntityRef, replication_config::ReplicationConfig,
};
