//! # Naia Shared
//! Common functionality shared between naia-server & naia-client crates.

#![deny(trivial_numeric_casts, unstable_features, unused_import_braces)]

#[macro_use]
extern crate cfg_if;
extern crate core;

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
    BitReader, BitWrite, BitWriter, ConstBitLength, FileBitWriter, OutgoingPacket, OwnedBitReader,
    Serde, SerdeBevyClient, SerdeBevyServer, SerdeBevyShared, SerdeErr, SerdeFloatConversion,
    SerdeHecs, SerdeIntegerConversion, SerdeInternal, SignedFloat, SignedInteger,
    SignedVariableFloat, SignedVariableInteger, UnsignedFloat, UnsignedInteger,
    UnsignedVariableFloat, UnsignedVariableInteger, MTU_SIZE_BITS, MTU_SIZE_BYTES,
};
pub use naia_socket_shared::{
    generate_identity_token, link_condition_logic, IdentityToken, Instant, LinkConditionerConfig,
    Random, SocketConfig, TimeQueue,
};

#[cfg(feature = "test_time")]
pub use naia_socket_shared::TestClock;

mod backends;
mod bigmap;
mod connection;
mod constants;
mod game_time;
pub mod handshake;
mod key_generator;
mod messages;
mod protocol;
mod sequence_list;
mod types;
mod world;
mod wrapping_number;

cfg_if! {
    if #[cfg(any(feature = "transport_udp", feature = "transport_local"))]{
        pub mod transport;
    }
}

cfg_if! {
    if #[cfg(feature = "interior_visibility")] {
        pub use world::local::LocalEntity;
    }
}

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
        senders::{
            channel_sender::{ChannelSender, MessageChannelSender},
            reliable_sender::ReliableSender,
            request_sender::LocalResponseId,
        },
    },
    message::{Message, Message as MessageBevy, Message as MessageHecs, MessageBuilder},
    message_container::MessageContainer,
    message_kinds::{MessageKind, MessageKinds},
    message_manager::MessageManager,
    named::Named,
    request::{
        GlobalRequestId, GlobalResponseId, Request, Response, ResponseReceiveKey, ResponseSendKey,
    },
};
pub use world::{
    component::{
        component_kinds::{ComponentKind, ComponentKinds},
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
            ReplicatedComponent,
        },
    },
    delegation::{
        auth_channel::EntityAuthAccessor,
        entity_auth_status::{EntityAuthStatus, HostEntityAuthStatus},
        host_auth_handler::HostAuthHandler,
    },
    entity::{
        entity_converters::{
            EntityAndGlobalEntityConverter, EntityConverterMut, FakeEntityConverter,
            GlobalWorldManagerType, LocalEntityAndGlobalEntityConverter,
            LocalEntityAndGlobalEntityConverterMut,
        },
        entity_message::EntityMessage,
        entity_message_receiver::EntityMessageReceiver,
        entity_message_type::EntityMessageType,
        error::EntityDoesNotExistError,
        global_entity::GlobalEntity,
        global_entity_map::{GlobalEntityMap, GlobalEntitySpawner},
        in_scope_entities::InScopeEntities,
    },
    host::host_world_manager::HostWorldManager,
    remote::remote_world_manager::RemoteWorldManager,
    shared_global_world_manager::SharedGlobalWorldManager,
    world_type::{WorldMutType, WorldRefType},
};

pub use bigmap::{BigMap, BigMapKey};
pub use game_time::{GameDuration, GameInstant, GAME_TIME_LIMIT};
pub use key_generator::KeyGenerator;
pub use messages::channels::senders::request_sender::{
    LocalRequestOrResponseId, RequestOrResponse,
};
pub use protocol::{Protocol, ProtocolPlugin};
pub use types::{HostType, MessageIndex, PacketIndex, ShortMessageIndex, Tick};
pub use world::entity_command::EntityCommand;
pub use world::entity_event::EntityEvent;
pub use world::host::host_entity_generator::HostEntityGenerator;
pub use world::host::host_world_manager::SubCommandId;
pub use world::local::local_entity::{HostEntity, OwnedLocalEntity, RemoteEntity};
pub use world::local::local_entity_map::LocalEntityMap;
pub use world::local::local_world_manager::LocalWorldManager;
pub use world::sync::auth_channel::EntityAuthChannelState;
pub use world::sync::authority_error::AuthorityError;
#[cfg(feature = "e2e_debug")]
pub use world::sync::remote_entity_channel::EntityChannelState;
pub use world::sync::host_entity_channel::HostEntityChannel;
pub use world::sync::remote_entity_channel::RemoteEntityChannel;
pub use world::update::component_update::{ComponentFieldUpdate, ComponentUpdate};
pub use world::update::diff_mask::DiffMask;
pub use world::update::global_diff_handler::GlobalDiffHandler;
pub use world::update::mut_channel::{MutChannelType, MutReceiver};
pub use wrapping_number::{
    sequence_equal_or_greater_than, sequence_equal_or_less_than, sequence_greater_than,
    sequence_less_than, wrapping_diff,
};
