//! # Naia Server
//! A server that uses either UDP or WebRTC communication to send/receive
//! messages to/from connected clients, and syncs registered
//! Entities/Components to clients to whom they are in-scope.

#![deny(
    trivial_casts,
    trivial_numeric_casts,
    unstable_features,
    unused_import_braces
)]

extern crate cfg_if;

#[macro_use]
extern crate log;

#[macro_use]
extern crate slotmap;

#[cfg(all(feature = "use-udp", feature = "use-webrtc"))]
compile_error!("Naia Server can only use UDP or WebRTC, you must pick one");

#[cfg(all(not(feature = "use-udp"), not(feature = "use-webrtc")))]
compile_error!("Naia Server requires either the 'use-udp' or 'use-webrtc' feature to be enabled, you must pick one.");

pub use naia_server_socket::ServerAddrs;

pub use naia_shared::{
    LinkConditionerConfig, Protocolize, Random, ReplicaMutWrapper, Replicate, SharedConfig,
    SocketConfig, WorldMutType, WorldRefType,
};

mod connection;
mod entity_action;
mod entity_manager;
mod entity_ref;
mod entity_scope_map;
mod error;
mod event;
mod global_diff_handler;
mod global_entity_record;
mod handshake_manager;
mod io;
mod keys;
mod local_component_record;
mod local_entity_record;
mod locality_status;
mod mut_channel;
mod packet_writer;
mod ping_manager;
mod room;
mod server;
mod server_config;
mod tick_manager;
mod user;
mod user_diff_handler;
mod user_scope;
mod world_record;

pub use entity_ref::{EntityMut, EntityRef};
pub use error::NaiaServerError;
pub use event::Event;
pub use keys::ComponentKey;
pub use room::{room_key::RoomKey, RoomMut, RoomRef};
pub use server::Server;
pub use server_config::ServerConfig;
pub use user::{user_key::UserKey, User, UserMut, UserRef};
pub use user_scope::UserScopeMut;
