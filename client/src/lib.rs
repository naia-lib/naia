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

pub use naia_shared::{default_channels, Random};

mod client;
mod client_config;
mod command_history;
mod connection;
mod error;
mod events;
mod protocol;
mod tick;

pub use client::Client;
pub use client_config::ClientConfig;
pub use command_history::CommandHistory;
pub use error::NaiaClientError;
pub use events::{
    ConnectEvent, DespawnEntityEvent, DisconnectEvent, ErrorEvent, Events, InsertComponentEvent,
    MessageEvent, RejectEvent, RemoveComponentEvent, SpawnEntityEvent, TickEvent,
    UpdateComponentEvent,
};
pub use protocol::entity_ref::EntityRef;

pub mod internal {
    pub use crate::connection::handshake_manager::{HandshakeManager, HandshakeState};
}
