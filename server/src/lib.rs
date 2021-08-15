//! # Naia Server
//! A server that uses either UDP or WebRTC communication to send/receive
//! messages to/from connected clients, and syncs registered
//! Objects/Entities/Components to clients to whom they are in-scope.

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
    find_my_ip_address, EntityKey, LinkConditionerConfig, LocalReplicaKey, ProtocolType, Random,
    Ref, Replicate, SharedConfig,
};

mod client_connection;
mod command_receiver;
mod entity_record;
mod error;
mod event;
mod interval;
mod keys;
mod locality_status;
mod mut_handler;
mod packet_writer;
mod ping_manager;
mod property_mutator;
mod replica_action;
mod replica_manager;
mod replica_record;
mod room;
mod server;
mod server_config;
mod tick_manager;
mod user;

pub use event::Event;
pub use keys::{replica_key::ReplicaKey, ComponentKey, GlobalPawnKey, ObjectKey};
pub use room::room_key::RoomKey;
pub use server::Server;
pub use server_config::{ServerAddresses, ServerConfig};
pub use user::user_key::UserKey;
