//! # Naia Client
//! A cross-platform client that can send/receive messages to/from a server, and
//! has a pool of in-scope Entities/Components that are synced with the
//! server.

#![deny(
    missing_docs,
    missing_debug_implementations,
    trivial_casts,
    trivial_numeric_casts,
    unsafe_code,
    unstable_features,
    unused_import_braces
)]

mod client;
mod client_config;
mod command_receiver;
mod command_sender;
mod connection_state;
mod entity_action;
mod entity_manager;
mod entity_record;
mod entity_ref;
mod error;
mod event;
mod packet_writer;
mod ping_manager;
mod server_connection;
mod tick_manager;
mod tick_queue;
mod component_record;

pub use naia_shared::{
    wrapping_diff, Instant, LinkConditionerConfig, LocalEntityKey, NaiaKey, ProtocolType, Random,
    Ref, ReplicaEq, Replicate,
};

pub use client::Client;
pub use client_config::ClientConfig;
pub use event::Event;
pub use naia_client_socket::Packet;
