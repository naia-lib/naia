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

#[macro_use]
extern crate cfg_if;

pub mod transport;
pub mod shared {
    pub use naia_shared::{
        default_channels, BigMap, BigMapKey, BitReader, BitWrite, BitWriter, ConstBitLength,
        FileBitWriter, GlobalResponseId, Instant, Protocol, Random, ResponseReceiveKey, Serde,
        SerdeErr, SignedInteger, SignedVariableInteger, SocketConfig, UnsignedInteger,
        UnsignedVariableInteger,
    };
}

pub use naia_shared::SerdeBevyServer as SerdeBevy;
pub use naia_shared::{EntityPriorityMut, EntityPriorityRef};

mod connection;
mod error;
mod events;
mod handshake;
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
#[cfg(feature = "bench_instrumentation")]
pub use connection::connection::bench_send_counters;
pub use error::NaiaServerError;
pub use events::{
    AuthEvent, ConnectEvent, DelegateEntityEvent, DespawnEntityEvent, DisconnectEvent,
    EntityAuthGrantEvent, EntityAuthResetEvent, ErrorEvent, Event, Events, InsertComponentEvent,
    MainEvents, MessageEvent, PublishEntityEvent, RemoveComponentEvent, RequestEvent,
    SpawnEntityEvent, TickEvent, TickEvents, UnpublishEntityEvent, UpdateComponentEvent,
    WorldEvents, WorldPacketEvent,
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
