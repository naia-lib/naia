//! # Naia Shared
//! Common functionality shared between naia-server & naia-client crates.

#![deny(trivial_numeric_casts, unstable_features, unused_import_braces)]

#[macro_use]
extern crate cfg_if;

cfg_if! {
    if #[cfg(all(target_arch = "wasm32", feature = "wbindgen", feature = "mquad"))]
    {
        // Use both protocols...
        compile_error!("wasm target for 'naia_shared' crate requires either the 'wbindgen' OR 'mquad' feature to be enabled, you must pick one.");
    }
    else if #[cfg(all(target_arch = "wasm32", not(feature = "wbindgen"), not(feature = "mquad")))]
    {
        // Use no protocols...
        compile_error!("wasm target for 'naia_shared' crate requires either the 'wbindgen' or 'mquad' feature to be enabled, you must pick one.");
    }
}

pub use naia_socket_shared::{Instant, LinkConditionerConfig, Random, SocketConfig};

pub use naia_derive::*;
pub use naia_serde as serde;
pub use serde::derive_serde;

mod backends;
mod connection;
mod messages;
mod protocol;

mod bigmap;
mod constants;
mod key_generator;
mod shared_config;
mod types;
mod world_type;
mod wrapping_number;

pub use backends::{Timer, Timestamp};
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
    message_channel::{ChannelReader, ChannelReceiver, ChannelSender, ChannelWriter},
    message_list_header,
    message_manager::MessageManager,
    ordered_reliable_receiver::OrderedReliableReceiver,
    reliable_sender::ReliableSender,
    unordered_reliable_receiver::UnorderedReliableReceiver,
};
pub use protocol::{
    component_update::ComponentUpdate,
    diff_mask::DiffMask,
    entity_action::EntityAction,
    entity_action_receiver::EntityActionReceiver,
    entity_action_type::EntityActionType,
    entity_handle::EntityHandle,
    entity_property::{
        EntityConverter, EntityHandleConverter, EntityProperty, FakeEntityConverter,
        NetEntityConverter, NetEntityHandleConverter,
    },
    net_entity::NetEntity,
    property::Property,
    property_mutate::{PropertyMutate, PropertyMutator},
    protocol_io::ProtocolIo,
    protocolize::{ProtocolInserter, ProtocolKindType, Protocolize},
    replica_ref::{
        ReplicaDynMut, ReplicaDynMutTrait, ReplicaDynMutWrapper, ReplicaDynRef, ReplicaDynRefTrait,
        ReplicaDynRefWrapper, ReplicaMutTrait, ReplicaMutWrapper, ReplicaRefTrait,
        ReplicaRefWrapper,
    },
    replicate::{Replicate, ReplicateSafe},
};

pub use bigmap::{BigMap, BigMapKey};
pub use constants::{MESSAGE_HISTORY_SIZE, MTU_SIZE_BITS, MTU_SIZE_BYTES};
pub use key_generator::KeyGenerator;
pub use shared_config::SharedConfig;
pub use types::{HostType, MessageId, PacketIndex, ShortMessageId, Tick};
pub use world_type::{WorldMutType, WorldRefType};
pub use wrapping_number::{sequence_greater_than, sequence_less_than, wrapping_diff};
