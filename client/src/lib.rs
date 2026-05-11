//! Client-side half of the naia real-time entity replication and messaging
//! library.
//!
//! The central type is [`Client<E>`], which connects to a naia server,
//! maintains a local mirror of replicated entities, and sends client
//! messages and (optionally) client-authoritative entity mutations.
//!
//! Runs natively (UDP) and in the browser (WebRTC via
//! `wasm32-unknown-unknown`). The same application logic compiles for both
//! targets; only the [`Socket`](transport::Socket) implementation differs.
//!
//! # Connection setup
//!
//! ```no_run
//! # use naia_client::{Client, ClientConfig};
//! # fn example<E, P, S>(mut client: Client<E>, protocol: P, socket: S)
//! #     where E: Copy + Eq + std::hash::Hash + Send + Sync,
//! #           P: Into<naia_shared::Protocol>,
//! #           S: Into<Box<dyn naia_client::transport::Socket>>
//! # {
//! // Optional: include an auth message in the handshake.
//! // client.auth(MyAuthMessage { token: "…".into() });
//!
//! client.connect(socket); // begin handshake; loop until ConnectionEvent fires
//! # }
//! ```
//!
//! # Main loop
//!
//! ```no_run
//! # use naia_client::{Client, Events, TickEvents};
//! # use naia_shared::{Instant, WorldMutType, WorldRefType};
//! # fn run<E, W>(client: &mut Client<E>, world: W, now: Instant)
//! #     where E: Copy + Eq + std::hash::Hash + Send + Sync,
//! #           W: WorldMutType<E> + WorldRefType<E> + Copy
//! # {
//! loop {
//!     client.receive_all_packets();                     // 1. read from socket
//!     client.process_all_packets(world, &now);          // 2. decode + apply
//!     let events: Events<E> = client.take_world_events(); // 3. drain events
//!     let ticks: TickEvents = client.take_tick_events(&now); // 4. tick clock
//!     // 5. apply prediction / interpolation here
//!     client.send_all_packets(world);                   // 6. flush outbound
//! #   break;
//! }
//! # }
//! ```
//!
//! # Key types
//!
//! | Type | Purpose |
//! |------|---------|
//! | [`Client<E>`] | Main entry point |
//! | [`EntityMut`] | Builder for client-authoritative entities |
//! | [`EntityRef`] | Read-only entity handle |
//! | [`Publicity`] | The three visibility states (Private / Public / Delegated) |
//! | [`CommandHistory`] | Rollback buffer for client-prediction |
//! | [`ConnectionStatus`](client::ConnectionStatus) | Lifecycle state |

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

pub use naia_shared::{DisconnectReason, EntityPriorityMut, EntityPriorityRef};

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
