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

pub use naia_shared as shared;

mod client;
mod client_config;
mod command_history;
mod connection;
mod error;
mod event;
mod protocol;
mod tick;

pub use client::Client;
pub use client_config::ClientConfig;
pub use command_history::CommandHistory;
pub use error::NaiaClientError;
pub use event::Event;
pub use protocol::entity_manager::EntityManager;
pub use protocol::entity_ref::EntityRef;
pub use tick::tick_buffer_sender::TickBufferSender;
pub use tick::tick_manager::{TickManager, TickManagerConfig};
pub use tick::tick_queue::{ItemContainer, TickQueue};

pub mod internal {
    pub use crate::connection::handshake_manager::{HandshakeManager, HandshakeState};
}
