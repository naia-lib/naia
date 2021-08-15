//! # Naia Server
//! A server that uses either UDP or WebRTC communication to send/receive
//! messages to/from connected clients, and syncs registered replicates to
//! clients to whom those replicates are in-scope.

#![deny(
    missing_docs,
    trivial_casts,
    trivial_numeric_casts,
    unsafe_code,
    unstable_features,
    unused_import_braces
)]

#[macro_use]
extern crate cfg_if;

#[macro_use]
extern crate log;

#[macro_use]
extern crate slotmap;

#[cfg(all(feature = "use-udp", feature = "use-webrtc"))]
compile_error!("Naia Server can only use UDP or WebRTC, you must pick one");

#[cfg(all(not(feature = "use-udp"), not(feature = "use-webrtc")))]
compile_error!("Naia Server requires either the 'use-udp' or 'use-webrtc' feature to be enabled, you must pick one.");

pub use naia_shared::{
    find_my_ip_address, EntityKey, LinkConditionerConfig, LocalReplicateKey, ProtocolType, Random,
    Ref, Replicate, SharedConfig,
};

mod client_connection;
mod command_receiver;
mod error;
mod event;
mod interval;
mod packet_writer;
mod ping_manager;
mod replicate;
mod room;
mod server;
mod server_config;
mod tick_manager;
mod user;

pub use event::Event;
pub use replicate::keys::{replicate_key::ReplicateKey, ComponentKey, GlobalPawnKey, ObjectKey};
pub use room::room_key::RoomKey;
pub use server::Server;
pub use server_config::{ServerAddresses, ServerConfig};
pub use user::user_key::UserKey;
