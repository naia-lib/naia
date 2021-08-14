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
extern crate cfg_if;

mod ack_manager;
mod state;
mod connection;
mod connection_config;
mod ecs;
mod events;
mod host_tick_manager;
mod host_type;
mod key_store;
mod manager_type;
mod manifest;
mod packet_type;
mod sequence_buffer;
mod shared_config;
mod standard_header;
mod wrapping_number;

/// Commonly used utility methods to be used by naia-server & naia-client
pub mod utils;

pub use naia_socket_shared::{
    find_my_ip_address, Instant, LinkConditionerConfig, PacketReader, Random, Ref, Timer, Timestamp,
};

pub use ack_manager::AckManager;
pub use state::{
    state::{State, StateEq, EventClone},
    state_builder::StateBuilder,
    state_message_type::StateMessageType,
    state_mutator::StateMutator,
    state_notifiable::StateNotifiable,
    state_type::StateType,
    property::Property,
    diff_mask::DiffMask,
};
pub use connection::Connection;
pub use connection_config::ConnectionConfig;
pub use events::{
    event_manager::EventManager,
    event_packet_writer::{EventPacketWriter, MTU_SIZE},
};
pub use ecs::{
    keys::{EntityKey, LocalObjectKey, LocalEntityKey, LocalComponentKey, PawnKey, NaiaKey}
};
pub use host_tick_manager::HostTickManager;
pub use host_type::HostType;
pub use key_store::KeyGenerator;
pub use manager_type::ManagerType;
pub use manifest::Manifest;
pub use packet_type::PacketType;
pub use sequence_buffer::{SequenceBuffer, SequenceIterator, SequenceNumber};
pub use shared_config::SharedConfig;
pub use standard_header::StandardHeader;
pub use wrapping_number::{sequence_greater_than, sequence_less_than, wrapping_diff};
