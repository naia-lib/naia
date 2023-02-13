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

pub use naia_derive::{
    Channel, Message, MessageBevy, MessageHecs, Replicate, ReplicateBevy, ReplicateHecs,
};
pub use naia_serde::{Serde, SerdeBevy, SerdeHecs, SerdeInternal};
pub use naia_socket_shared::{Instant, LinkConditionerConfig, Random, SocketConfig};

mod backends;
mod component;
mod connection;
mod entity;
mod messages;

mod bigmap;
mod constants;
mod key_generator;
mod protocol;
mod types;
mod world_type;
mod wrapping_number;

pub use backends::{Timer, Timestamp};
pub use component::{
    component_kinds::{ComponentKind, ComponentKinds},
    component_update::ComponentUpdate,
    diff_mask::DiffMask,
    property::Property,
    property_mutate::{PropertyMutate, PropertyMutator},
    replica_ref::{
        ReplicaDynMut, ReplicaDynMutTrait, ReplicaDynMutWrapper, ReplicaDynRef, ReplicaDynRefTrait,
        ReplicaDynRefWrapper, ReplicaMutTrait, ReplicaMutWrapper, ReplicaRefTrait,
        ReplicaRefWrapper,
    },
    replicate::{
        Replicate, Replicate as ReplicateHecs, Replicate as ReplicateBevy, ReplicateBuilder,
    },
};
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
pub use entity::{
    entity_action::EntityAction,
    entity_action_receiver::EntityActionReceiver,
    entity_action_type::EntityActionType,
    entity_handle::EntityHandle,
    entity_property::{
        EntityConverter, EntityDoesNotExistError, EntityHandleConverter, EntityProperty,
        FakeEntityConverter, NetEntityConverter, NetEntityHandleConverter,
    },
    net_entity::NetEntity,
};
pub use messages::{
    channel::{Channel, ChannelDirection, ChannelMode, ReliableSettings, TickBufferSettings},
    channel_kinds::{ChannelKind, ChannelKinds},
    default_channels,
    message::{Message, Message as MessageBevy, Message as MessageHecs, MessageBuilder},
    message_channel::{ChannelReader, ChannelReceiver, ChannelSender, ChannelWriter},
    message_kinds::{MessageKind, MessageKinds},
    message_manager::MessageManager,
    named::Named,
    ordered_reliable_receiver::OrderedReliableReceiver,
    protocol_io::ProtocolIo,
    reliable_sender::ReliableSender,
    unordered_reliable_receiver::UnorderedReliableReceiver,
};
pub use naia_serde::{
    BitReader, BitWrite, BitWriter, OwnedBitReader, SerdeErr, UnsignedInteger,
    UnsignedVariableInteger,
};

pub use bigmap::{BigMap, BigMapKey};
pub use constants::MESSAGE_HISTORY_SIZE;
pub use key_generator::KeyGenerator;
pub use protocol::{Protocol, ProtocolPlugin};
pub use types::{HostType, MessageIndex, PacketIndex, ShortMessageIndex, Tick};
pub use world_type::{WorldMutType, WorldRefType};
pub use wrapping_number::{sequence_greater_than, sequence_less_than, wrapping_diff};
