
#[macro_use]
extern crate log;

#[macro_use]
extern crate cfg_if;

mod entities;
mod events;
mod rtt;
mod ack_manager;
mod config;
mod connection;
mod packet_type;
mod sequence_buffer;
mod standard_header;
mod timestamp;
mod manager_type;
mod packet_reader;
mod packet_writer;
mod host_type;
mod manifest;
mod duration;
mod instant;
pub mod utils;

pub use naia_socket_shared::{find_my_ip_address, Timer};

pub use {
    packet_type::PacketType,
    standard_header::StandardHeader,
    config::Config,
    connection::Connection,
    timestamp::Timestamp,
    manager_type::ManagerType,
    packet_reader::PacketReader,
    packet_writer::{PacketWriter, MTU_SIZE},
    host_type::HostType,
    ack_manager::AckManager,
    sequence_buffer::SequenceNumber,
    manifest::Manifest,
    duration::Duration,
    instant::Instant,
    events::{
        event::{Event, EventClone},
        event_type::EventType,
        event_manager::EventManager,
        event_builder::{EventBuilder},
    },
    entities::{
        entity::{Entity},
        entity_type::EntityType,
        local_entity_key::LocalEntityKey,
        state_mask::StateMask,
        entity_notifiable::EntityNotifiable,
        entity_mutator::EntityMutator,
        entity_builder::EntityBuilder,
        property::{Property},
        property_io::{PropertyIo},
    },
    rtt::{
        rtt_data::RttData,
        rtt_measurer::RttMeasurer,
        rtt_tracker::RttTracker,
    },
};