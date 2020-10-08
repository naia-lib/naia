//! # Naia Server
//! A server that uses either UDP or WebRTC communication to send/receive events
//! to/from connected clients, and syncs registered entities to clients to whom
//! those entities are in-scope.

#![deny(
    missing_docs,
    trivial_casts,
    trivial_numeric_casts,
    unsafe_code,
    unstable_features,
    unused_import_braces
)]

#[macro_use]
extern crate log;

#[macro_use]
extern crate slotmap;

#[cfg(all(feature = "use-udp", feature = "use-webrtc"))]
compile_error!("Naia Server can only use UDP or WebRTC, you must pick one");

#[cfg(all(not(feature = "use-udp"), not(feature = "use-webrtc")))]
compile_error!("Naia Server requires either the 'use-udp' or 'use-webrtc' feature to be enabled, you must pick one.");

pub use naia_shared::{
    find_my_ip_address, random, Entity, EntityType, LinkConditionerConfig, SharedConfig,
};

mod client_connection;
mod command_receiver;
mod entities;
mod error;
mod naia_server;
mod ping_manager;
mod room;
mod server_config;
mod server_event;
mod server_packet_writer;
mod server_tick_manager;
mod user;

pub use naia_server::NaiaServer;
pub use room::room_key::RoomKey;
pub use server_config::ServerConfig;
pub use server_event::ServerEvent;
pub use user::user_key::UserKey;
