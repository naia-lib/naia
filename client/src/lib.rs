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

mod client;
mod client_config;
mod connection;
mod connection_state;
mod entity_action;
mod entity_manager;
mod entity_message_packet_writer;
mod entity_message_sender;
mod entity_record;
mod entity_ref;
mod error;
mod event;
mod handshake_manager;
mod io;
mod packet_writer;
mod ping_manager;
mod tick;
mod tick_manager;
mod tick_queue;

pub use naia_shared as shared;

pub use client::Client;
pub use client_config::ClientConfig;
pub use entity_ref::EntityRef;
pub use error::NaiaClientError;
pub use event::Event;
pub use naia_client_socket::Packet;
