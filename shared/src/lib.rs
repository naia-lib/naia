//! # Naia Shared
//! Common functionality shared between naia-server & naia-client crates.

#![deny(trivial_numeric_casts, unstable_features, unused_import_braces)]

#[macro_use]
extern crate cfg_if;

pub use naia_socket_shared::{
    Instant, LinkConditionerConfig, Random, SocketConfig, Timer, Timestamp,
};

pub use naia_derive::*;
pub use naia_serde as serde;
pub use serde::derive_serde;

mod connection;
mod messages;
mod protocol;

mod bigmap;
mod constants;
mod key_generator;
mod shared_config;
mod types;
mod vecmap;
mod world_type;
mod wrapping_number;

pub use connection::{
    ack_manager::AckManager,
    bandwidth_monitor::BandwidthMonitor,
    base_connection::BaseConnection,
    compression_config::{CompressionConfig, CompressionMode},
    connection_config::ConnectionConfig,
    decoder::Decoder,
    encoder::Encoder,
    packet_notifiable::PacketNotifiable,
    packet_type::PacketType,
    ping_config::PingConfig,
    ping_manager::{PingIndex, PingManager},
    standard_header::StandardHeader,
};
pub use messages::{
    channel_config::{
        Channel, ChannelConfig, ChannelDirection, ChannelIndex, ChannelMode, DefaultChannels,
        ReliableSettings, TickBufferSettings,
    },
    channel_tick_buffer::ChannelTickBuffer,
    message_channel::{ChannelReceiver, ChannelSender},
    message_list_header,
    message_manager::MessageManager,
    reliable_sender::ReliableSender,
    tick_buffer::TickBuffer,
};
pub use protocol::{
    diff_mask::DiffMask,
    entity_action_type::EntityActionType,
    entity_handle::EntityHandle,
    entity_property::{
        EntityConverter, EntityHandleConverter, EntityProperty, FakeEntityConverter,
        NetEntityConverter, NetEntityHandleConverter,
    },
    net_entity::NetEntity,
    property::Property,
    property_mutate::{PropertyMutate, PropertyMutator},
    protocolize::{ProtocolInserter, ProtocolKindType, Protocolize},
    replica_ref::{
        ReplicaDynMut, ReplicaDynMutTrait, ReplicaDynMutWrapper, ReplicaDynRef, ReplicaDynRefTrait,
        ReplicaDynRefWrapper, ReplicaMutTrait, ReplicaMutWrapper, ReplicaRefTrait,
        ReplicaRefWrapper,
    },
    replicate::{Replicate, ReplicateSafe},
};

pub use bigmap::{BigMap, BigMapKey};
pub use constants::{MTU_SIZE_BITS, MTU_SIZE_BYTES};
pub use key_generator::KeyGenerator;
pub use shared_config::SharedConfig;
pub use types::{MessageId, PacketIndex, Tick};
pub use vecmap::VecMap;
pub use world_type::{WorldMutType, WorldRefType};
pub use wrapping_number::{sequence_greater_than, sequence_less_than, wrapping_diff};
