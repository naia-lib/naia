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

mod channel_tick_buffer;
mod client;
mod client_config;
mod connection;
mod constants;
mod entity_manager;
mod entity_record;
mod entity_ref;
mod error;
mod event;
mod handshake_manager;
mod io;
mod tick_buffer;
mod tick_manager;
mod tick_queue;
mod types;

pub use naia_shared as shared;

pub use client::Client;
pub use client_config::ClientConfig;
pub use entity_ref::EntityRef;
pub use error::NaiaClientError;
pub use event::Event;

pub mod internal {
    pub use crate::handshake_manager::{HandshakeManager, HandshakeState};
}
