//! # Naia Shared
//! Common functionality shared between naia-server & naia-client crates.

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
extern crate cfg_if;

mod ack_manager;
mod actors;
mod connection;
mod connection_config;
mod events;
mod host_tick_manager;
mod host_type;
mod manager_type;
mod manifest;
mod packet_type;
mod sequence_buffer;
mod shared_config;
mod standard_header;
mod wrapping_number;

/// Commonly used utility methods to be used by naia-server & naia-client
pub mod utils;

pub use naia_socket_shared::{find_my_ip_address, Random, LinkConditionerConfig, Timer, Instant, Timestamp, PacketReader};

pub use ack_manager::AckManager;
pub use actors::{
    actor::{Actor, ActorEq},
    actor_builder::ActorBuilder,
    actor_mutator::ActorMutator,
    actor_notifiable::ActorNotifiable,
    actor_type::ActorType,
    interp_lerp::interp_lerp,
    local_actor_key::LocalActorKey,
    property::Property,
    state_mask::StateMask,
};
pub use connection::Connection;
pub use connection_config::ConnectionConfig;
pub use events::{
    event::{Event, EventClone},
    event_builder::EventBuilder,
    event_manager::EventManager,
    event_packet_writer::{EventPacketWriter, MTU_SIZE},
    event_type::EventType,
};
pub use host_tick_manager::HostTickManager;
pub use host_type::HostType;
pub use manager_type::ManagerType;
pub use manifest::Manifest;
pub use packet_type::PacketType;
pub use sequence_buffer::{SequenceBuffer, SequenceIterator, SequenceNumber};
pub use shared_config::SharedConfig;
pub use standard_header::StandardHeader;
pub use wrapping_number::{sequence_greater_than, sequence_less_than, wrapping_diff};
