//! Server-side half of the naia real-time entity replication and messaging
//! library.
//!
//! The central type is [`Server<E>`], which listens for connections, maintains
//! rooms and user scopes, replicates entities to in-scope clients, and routes
//! typed messages and requests.
//!
//! # Main loop
//!
//! ```no_run
//! # use naia_server::{Server, Events, TickEvents};
//! # use naia_shared::{Instant, WorldMutType, WorldRefType};
//! # fn run<E, W>(server: &mut Server<E>, world: W, now: Instant)
//! #     where E: Copy + Eq + std::hash::Hash + Send + Sync,
//! #           W: WorldMutType<E> + WorldRefType<E> + Copy
//! # {
//! loop {
//!     server.receive_all_packets();                    // 1. read from socket
//!     server.process_all_packets(world, &now);         // 2. decode + apply
//!     let events: Events<E> = server.take_world_events(); // 3. drain events
//!     let ticks: TickEvents = server.take_tick_events(&now); // 4. tick clock
//!     // 5. mutate replicated components here
//!     server.send_all_packets(world);                  // 6. flush outbound
//! #   break;
//! }
//! # }
//! ```
//!
//! Steps must run in this order every frame. Accepting a connection requires
//! calling [`accept_connection`](Server::accept_connection) inside a
//! [`ConnectEvent`] handler.
//!
//! # Key types
//!
//! | Type | Purpose |
//! |------|---------|
//! | [`Server<E>`] | Main entry point |
//! | [`EntityMut`] | Builder returned by [`spawn_entity`](Server::spawn_entity) |
//! | [`EntityRef`] | Read-only entity handle |
//! | [`RoomMut`] | Manages room membership |
//! | [`UserScopeMut`] | Fine-grained entity-per-user visibility |
//! | [`ReplicationConfig`] | Controls publicity and scope-exit behaviour |
//! | [`Publicity`] | The three visibility states (Private / Public / Delegated) |

#![deny(
    trivial_casts,
    trivial_numeric_casts,
    unstable_features,
    unused_import_braces
)]
#![warn(missing_docs)]

#[macro_use]
extern crate cfg_if;

/// Transport-layer socket abstractions (UDP, WebRTC, local in-process).
pub mod transport;
/// Re-exports of commonly-used `naia_shared` types, scoped for server consumers.
pub mod shared {
    pub use naia_shared::{
        default_channels, BigMap, BigMapKey, BitReader, BitWrite, BitWriter, ConstBitLength,
        FileBitWriter, GlobalResponseId, Instant, Protocol, Random, ResponseReceiveKey, Serde,
        SerdeErr, SignedInteger, SignedVariableInteger, SocketConfig, UnsignedInteger,
        UnsignedVariableInteger,
    };
}

/// Bevy-specific serialization derive support (re-export of [`naia_shared::SerdeBevyServer`]).
pub use naia_shared::SerdeBevyServer as SerdeBevy;
pub use naia_shared::{ConnectionStats, DisconnectReason, EntityPriorityMut, EntityPriorityRef};

mod connection;
mod error;
mod events;
mod handshake;
/// Lag-compensation snapshot buffer that stores per-tick world state for rollback hit detection.
pub mod historian;
mod request;
mod room;
mod server;
mod time_manager;
mod user;
mod user_scope;
mod world;

cfg_if! {
    if #[cfg(feature = "interior_visibility")] {
        pub use naia_shared::LocalEntity;
    }
}

pub use connection::tick_buffer_messages::TickBufferMessages;
pub use historian::Historian;
#[cfg(feature = "bench_instrumentation")]
pub use connection::connection::bench_send_counters;
pub use error::NaiaServerError;
pub use events::{
    AuthEvent, ConnectEvent, DelegateEntityEvent, DespawnEntityEvent, DisconnectEvent,
    EntityAuthDeniedEvent, EntityAuthGrantEvent, EntityAuthResetEvent, ErrorEvent, Event, Events,
    InsertComponentEvent,
    MainEvents, MessageEvent, PublishEntityEvent, QueuedDisconnectEvent, RemoveComponentEvent,
    RequestEvent, SpawnEntityEvent, TickEvent, TickEvents, UnpublishEntityEvent,
    UpdateComponentEvent, WorldEvents, WorldPacketEvent,
};
pub use room::{RoomKey, RoomMut, RoomRef};
pub use server::{MainServer, Server, ServerConfig, WorldServer};

#[cfg(feature = "e2e_debug")]
pub use server::world_server::{
    SERVER_AUTH_GRANTED_EMITTED, SERVER_OUTGOING_CMDS_DRAINED_TOTAL, SERVER_ROOM_MOVE_CALLED,
    SERVER_RX_FRAMES, SERVER_SCOPE_DIFF_ENQUEUED, SERVER_SEND_ALL_PACKETS_CALLS,
    SERVER_SET_AUTH_ENQUEUED, SERVER_SPAWN_APPLIED, SERVER_TX_FRAMES, SERVER_WORLD_MSGS_DRAINED,
    SERVER_WORLD_PKTS_SENT, SERVER_WROTE_SET_AUTH,
};
pub use user::{MainUser, MainUserRef, UserKey, UserMut, UserRef, WorldUser};
pub use user_scope::{UserScopeMut, UserScopeRef};
pub use world::{
    entity_mut::EntityMut, entity_owner::EntityOwner, entity_ref::EntityRef,
    replication_config::{Publicity, ReplicationConfig, ScopeExit},
};
