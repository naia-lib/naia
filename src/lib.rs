
#[macro_use]
extern crate log;

#[macro_use]
extern crate cfg_if;

mod entities;
mod events;
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
pub mod utils;

pub use gaia_socket_shared::{find_my_ip_address, Timer};

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
    events::{
        net_event::{NetEvent, NetEventType, NetEventClone},
        event_manifest::EventManifest,
        event_type::EventType,
        event_manager::EventManager,
    },
    entities::{
        net_entity::{NetEntity, NetEntityType},
        entity_manifest::EntityManifest,
        entity_type::EntityType,
        entity_store::{EntityKey, EntityStore},
        entity_record::{EntityRecord, LocalEntityStatus, LocalEntityKey},
        state_mask::StateMask,
        mut_handler::MutHandler,
        entity_notifiable::EntityNotifiable,
    },
};