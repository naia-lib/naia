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
pub use naia_serde::{
    BitReader, BitWrite, BitWriter, ConstBitLength, OutgoingPacket, OwnedBitReader, Serde,
    SerdeBevy, SerdeErr, SerdeHecs, SerdeInternal, UnsignedInteger, UnsignedVariableInteger,
    MTU_SIZE_BITS, MTU_SIZE_BYTES,
};
pub use naia_socket_shared::{Instant, LinkConditionerConfig, Random, SocketConfig};

mod backends;
mod connection;
mod messages;
mod world;

mod bigmap;
mod constants;
mod game_time;
mod key_generator;
mod protocol;
mod types;
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
    ping_store::{PingIndex, PingStore},
    standard_header::StandardHeader,
};
pub use messages::{
    channels::{
        channel::{Channel, ChannelDirection, ChannelMode, ReliableSettings, TickBufferSettings},
        channel_kinds::{ChannelKind, ChannelKinds},
        default_channels,
        receivers::{
            channel_receiver::ChannelReceiver, ordered_reliable_receiver::OrderedReliableReceiver,
            unordered_reliable_receiver::UnorderedReliableReceiver,
        },
        senders::{channel_sender::ChannelSender, reliable_sender::ReliableSender},
    },
    message::{Message, Message as MessageBevy, Message as MessageHecs, MessageBuilder},
    message_container::MessageContainer,
    message_kinds::{MessageKind, MessageKinds},
    message_manager::MessageManager,
    named::Named,
};
pub use world::{
    component::{
        component_kinds::{ComponentKind, ComponentKinds},
        component_update::ComponentUpdate,
        diff_mask::DiffMask,
        entity_property::EntityProperty,
        property::Property,
        property_mutate::{PropertyMutate, PropertyMutator},
        replica_ref::{
            ReplicaDynMut, ReplicaDynMutTrait, ReplicaDynMutWrapper, ReplicaDynRef,
            ReplicaDynRefTrait, ReplicaDynRefWrapper, ReplicaMutTrait, ReplicaMutWrapper,
            ReplicaRefTrait, ReplicaRefWrapper,
        },
        replicate::{
            Replicate, Replicate as ReplicateHecs, Replicate as ReplicateBevy, ReplicateBuilder,
        },
    },
    entity::{
        entity_action::EntityAction,
        entity_action_receiver::EntityActionReceiver,
        entity_action_type::EntityActionType,
        entity_converters::{
            EntityConverter, EntityHandleConverter, FakeEntityConverter, NetEntityConverter,
            NetEntityHandleConverter,
        },
        entity_handle::EntityHandle,
        entity_ref::EntityRef,
        error::EntityDoesNotExistError,
        net_entity::NetEntity,
    },
    host::{
        global_diff_handler::GlobalDiffHandler, host_global_world_manager::HostGlobalWorldManager,
        host_local_world_manager::HostLocalWorldManager, world_record::WorldRecord,
    },
    remote::{
        entity_action_event::EntityActionEvent, entity_event::EntityEvent,
        entity_record::EntityRecord, remote_world_manager::RemoteWorldManager,
    },
    world_type::{WorldMutType, WorldRefType},
};

pub use bigmap::{BigMap, BigMapKey};
pub use game_time::{GameDuration, GameInstant, GAME_TIME_LIMIT};
pub use key_generator::KeyGenerator;
pub use protocol::{Protocol, ProtocolPlugin};
pub use types::{HostType, MessageIndex, PacketIndex, ShortMessageIndex, Tick};
pub use wrapping_number::{sequence_greater_than, sequence_less_than, wrapping_diff};
