//! # Naia Server
//! A server that uses either UDP or WebRTC communication to send/receive events to/from connected clients, and syncs registered entities to clients to whom those entities are in-scope.

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

pub use naia_shared::{find_my_ip_address, Config, Entity, EntityType};

mod client_connection;
mod entities;
mod error;
mod naia_server;
mod room;
mod server_event;
mod user;

pub use {
    naia_server::NaiaServer, room::room_key::RoomKey, server_event::ServerEvent,
    user::user_key::UserKey,
};
