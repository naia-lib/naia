//! # Naia Client
//! A cross-platform client that can send/receive events to/from a server, and
//! has a pool of in-scope actors that are synced with the server.

#![deny(
    missing_docs,
    missing_debug_implementations,
    trivial_casts,
    trivial_numeric_casts,
    unsafe_code,
    unstable_features,
    unused_import_braces
)]

mod client_actor_manager;
mod client_actor_message;
mod client_config;
mod client_connection_state;
mod client_event;
mod client_packet_writer;
mod client_tick_manager;
mod command_receiver;
mod command_sender;
mod dual_command_receiver;
mod dual_command_sender;
mod error;
mod client;
mod ping_manager;
mod server_connection;
mod tick_queue;

pub use naia_shared::{find_my_ip_address, Instant, LinkConditionerConfig, Random, Ref, wrapping_diff, NaiaKey};

pub use client_config::ClientConfig;
pub use client_event::ClientEvent;
pub use client::Client;
pub use naia_client_socket::Packet;
