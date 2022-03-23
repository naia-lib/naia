//! # Naia Shared
//! Common functionality shared between naia-server & naia-client crates.

#![deny(trivial_numeric_casts, unstable_features, unused_import_braces)]

#[macro_use]
extern crate cfg_if;

pub use naia_derive::*;
pub use naia_serde as serde;
pub use serde::derive_serde;

mod ack_manager;
mod bandwidth_monitor;
mod base_connection;
mod bigmap;
mod channel_config;
mod compression_config;
mod connection_config;
mod constants;
mod decoder;
mod diff_mask;
mod encoder;
mod entity_action_type;
mod entity_handle;
mod entity_property;
mod key_store;
mod keys;
mod manifest;
mod message_list_header;
mod message_manager;
mod packet_notifiable;
mod packet_type;
mod ping_config;
mod property;
mod property_mutate;
mod protocolize;
mod replica_builder;
mod replica_ref;
mod replicate;
mod sequence_buffer;
mod shared_config;
mod standard_header;
mod types;
mod world_type;
mod wrapping_number;
mod channel_message_manager;

pub use naia_socket_shared::{
    Instant, LinkConditionerConfig, Random, SocketConfig, Timer, Timestamp,
};

pub use ack_manager::AckManager;
pub use bandwidth_monitor::BandwidthMonitor;
pub use base_connection::BaseConnection;
pub use bigmap::{BigMap, BigMapKey};
pub use channel_config::{Channel, ChannelConfig, ChannelIndex, ChannelMode, ChannelDirection, TickBufferSettings, ReliableSettings};
pub use compression_config::{CompressionConfig, CompressionMode};
pub use connection_config::ConnectionConfig;
pub use constants::{MTU_SIZE_BITS, MTU_SIZE_BYTES};
pub use decoder::Decoder;
pub use diff_mask::DiffMask;
pub use encoder::Encoder;
pub use entity_action_type::EntityActionType;
pub use entity_handle::EntityHandle;
pub use entity_property::{
    EntityConverter, EntityHandleConverter, EntityProperty, FakeEntityConverter,
    NetEntityConverter, NetEntityHandleConverter,
};
pub use key_store::KeyGenerator;
pub use keys::NetEntity;
pub use manifest::Manifest;
pub use message_list_header::{read_list_header, write_list_header};
pub use message_manager::MessageManager;
pub use packet_notifiable::PacketNotifiable;
pub use packet_type::PacketType;
pub use ping_config::PingConfig;
pub use property::Property;
pub use property_mutate::{PropertyMutate, PropertyMutator};
pub use protocolize::{ProtocolInserter, ProtocolKindType, Protocolize};
pub use replica_builder::ReplicaBuilder;
pub use replica_ref::{
    ReplicaDynMut, ReplicaDynMutTrait, ReplicaDynMutWrapper, ReplicaDynRef, ReplicaDynRefTrait,
    ReplicaDynRefWrapper, ReplicaMutTrait, ReplicaMutWrapper, ReplicaRefTrait, ReplicaRefWrapper,
};
pub use replicate::{Replicate, ReplicateSafe};
pub use shared_config::SharedConfig;
pub use standard_header::StandardHeader;
pub use types::{PacketIndex, Tick};
pub use world_type::{WorldMutType, WorldRefType};
pub use wrapping_number::{sequence_greater_than, sequence_less_than, wrapping_diff};
