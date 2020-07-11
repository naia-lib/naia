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
mod connection;
mod connection_config;
mod entities;
mod events;
mod instant;
mod manager_type;
mod manifest;
mod packet_reader;
mod packet_type;
mod packet_writer;
mod remote_tick_manager;
mod rtt;
mod sequence_buffer;
mod shared_config;
mod standard_header;
mod timestamp;

/// Commonly used utility methods to be used by naia-server & naia-client
pub mod utils;

pub use naia_socket_shared::{find_my_ip_address, Timer};

pub use ack_manager::AckManager;
pub use connection::Connection;
pub use connection_config::ConnectionConfig;
pub use entities::{
    entity::Entity, entity_builder::EntityBuilder, entity_mutator::EntityMutator,
    entity_notifiable::EntityNotifiable, entity_type::EntityType, local_entity_key::LocalEntityKey,
    property::Property, property_io::PropertyIo, state_mask::StateMask,
};
pub use events::{
    event::{Event, EventClone},
    event_builder::EventBuilder,
    event_manager::EventManager,
    event_type::EventType,
};
pub use instant::Instant;
pub use manager_type::ManagerType;
pub use manifest::Manifest;
pub use packet_reader::PacketReader;
pub use packet_type::PacketType;
pub use packet_writer::{PacketWriter, MTU_SIZE};
pub use rtt::rtt_tracker::RttTracker;
pub use sequence_buffer::SequenceNumber;
pub use shared_config::SharedConfig;
pub use timestamp::Timestamp;
