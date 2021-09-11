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
mod connection;
mod connection_config;
mod diff_mask;
mod host_tick_manager;
mod key_store;
mod keys;
mod manager_type;
mod manifest;
mod message_manager;
mod message_packet_writer;
mod packet_notifiable;
mod packet_type;
mod property;
mod property_mutate;
mod protocol_type;
mod replica_action_type;
mod replica_builder;
mod replicate;
mod sequence_buffer;
mod shared_config;
mod standard_header;
mod wrapping_number;
mod component_record;

/// Commonly used utility methods to be used by naia-server & naia-client
pub mod utils;

pub use naia_socket_shared::{
    find_my_ip_address, Instant, LinkConditionerConfig, PacketReader, Random, Ref, Timer, Timestamp,
};

pub use ack_manager::AckManager;
pub use component_record::ComponentRecord;
pub use connection::Connection;
pub use connection_config::ConnectionConfig;
pub use diff_mask::DiffMask;
pub use host_tick_manager::HostTickManager;
pub use key_store::KeyGenerator;
pub use keys::{
    EntityKey, LocalComponentKey, LocalEntityKey, NaiaKey,
};
pub use manager_type::ManagerType;
pub use manifest::Manifest;
pub use message_manager::MessageManager;
pub use message_packet_writer::{MessagePacketWriter, MTU_SIZE};
pub use packet_notifiable::PacketNotifiable;
pub use packet_type::PacketType;
pub use property::Property;
pub use property_mutate::PropertyMutate;
pub use protocol_type::ProtocolType;
pub use replica_action_type::ReplicaActionType;
pub use replica_builder::ReplicaBuilder;
pub use replicate::{ImplRef, ReplicaEq, Replicate};
pub use sequence_buffer::{SequenceBuffer, SequenceIterator, SequenceNumber};
pub use shared_config::SharedConfig;
pub use standard_header::StandardHeader;
pub use wrapping_number::{sequence_greater_than, sequence_less_than, wrapping_diff};
