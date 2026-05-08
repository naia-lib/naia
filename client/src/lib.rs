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

#[macro_use]
extern crate cfg_if;

pub mod transport;
pub mod shared {
    pub use naia_shared::{
        default_channels, sequence_greater_than, GameInstant, GlobalRequestId, GlobalResponseId,
        Instant, LinkConditionerConfig, Message, Protocol, Random, ResponseReceiveKey,
        SocketConfig, Tick,
    };
}

pub use naia_shared::{EntityPriorityMut, EntityPriorityRef};

mod client;
mod client_config;
mod command_history;
mod connection;
pub mod counters;
mod error;

// Extern function for shared code to increment CLIENT_SAW_SET_AUTH_WIRE
#[cfg(feature = "e2e_debug")]
#[no_mangle]
pub extern "Rust" fn client_saw_set_auth_wire_increment() {
    use crate::counters::CLIENT_SAW_SET_AUTH_WIRE;
    use std::sync::atomic::Ordering;
    CLIENT_SAW_SET_AUTH_WIRE.fetch_add(1, Ordering::Relaxed);
}

// Extern function for shared code to increment CLIENT_SAW_SPAWN
#[cfg(feature = "e2e_debug")]
#[no_mangle]
pub extern "Rust" fn client_saw_spawn_increment() {
    use crate::counters::CLIENT_SAW_SPAWN;
    use std::sync::atomic::Ordering;
    CLIENT_SAW_SPAWN.fetch_add(1, Ordering::Relaxed);
}

// Extern function for shared code to increment CLIENT_ROUTED_REMOTE_SPAWN
#[cfg(feature = "e2e_debug")]
#[no_mangle]
pub extern "Rust" fn client_routed_remote_spawn_increment() {
    use crate::counters::CLIENT_ROUTED_REMOTE_SPAWN;
    use std::sync::atomic::Ordering;
    CLIENT_ROUTED_REMOTE_SPAWN.fetch_add(1, Ordering::Relaxed);
}

// Extern function for shared code to increment CLIENT_PROCESSED_SPAWN
#[cfg(feature = "e2e_debug")]
#[no_mangle]
pub extern "Rust" fn client_processed_spawn_increment() {
    use crate::counters::CLIENT_PROCESSED_SPAWN;
    use std::sync::atomic::Ordering;
    CLIENT_PROCESSED_SPAWN.fetch_add(1, Ordering::Relaxed);
}
mod handshake;
mod request;
mod tick_events;
mod world;
mod world_events;

cfg_if! {
    if #[cfg(feature = "interior_visibility")] {
        pub use naia_shared::LocalEntity;
    }
}

pub use client::{Client, ConnectionStatus};
pub use client_config::ClientConfig;
pub use command_history::CommandHistory;
pub use connection::jitter_buffer::JitterBufferType;
pub use error::NaiaClientError;
pub use tick_events::{ClientTickEvent, ServerTickEvent, TickEvent, TickEvents};
pub use world::{
    entity_mut::EntityMut, entity_owner::EntityOwner, entity_ref::EntityRef,
    replication_config::Publicity,
};
pub use world_events::{
    ConnectEvent, DespawnEntityEvent, DisconnectEvent, EntityAuthDeniedEvent,
    EntityAuthGrantedEvent, EntityAuthResetEvent, ErrorEvent, InsertComponentEvent, MessageEvent,
    PublishEntityEvent, RejectEvent, RemoveComponentEvent, RequestEvent, SpawnEntityEvent,
    UnpublishEntityEvent, UpdateComponentEvent, WorldEvent, Events,
};
