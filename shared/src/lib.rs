//! # Naia Shared
//! Common functionality shared between naia-server & naia-client crates.

#![deny(
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
mod config;
mod connection;
mod duration;
mod entities;
mod events;
mod host_type;
mod instant;
mod manager_type;
mod manifest;
mod packet_reader;
mod packet_type;
mod packet_writer;
mod rtt;
mod sequence_buffer;
mod standard_header;
mod timestamp;

/// Commonly used utility methods to be used by naia-server & naia-client
pub mod utils;

pub use naia_socket_shared::{find_my_ip_address, Timer};

pub use {
    ack_manager::AckManager,
    config::Config,
    connection::Connection,
    entities::{
        entity::Entity, entity_builder::EntityBuilder, entity_mutator::EntityMutator,
        entity_notifiable::EntityNotifiable, entity_type::EntityType,
        local_entity_key::LocalEntityKey, property::Property, property_io::PropertyIo,
        state_mask::StateMask,
    },
    events::{
        event::{Event, EventClone},
        event_builder::EventBuilder,
        event_manager::EventManager,
        event_type::EventType,
    },
    host_type::HostType,
    instant::Instant,
    manager_type::ManagerType,
    manifest::Manifest,
    packet_reader::PacketReader,
    packet_type::PacketType,
    packet_writer::{PacketWriter, MTU_SIZE},
    rtt::rtt_tracker::RttTracker,
    sequence_buffer::SequenceNumber,
    timestamp::Timestamp,
};
