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

pub use naia_derive::{Channel, Message, MessageBevy, Replicate, ReplicateBevy};
pub use naia_serde::{
    BitCounter, BitReader, BitWrite, BitWriter, ConstBitLength, FileBitWriter, OutgoingPacket, OwnedBitReader,
    Serde, SerdeBevyClient, SerdeBevyServer, SerdeBevyShared, SerdeErr, SerdeFloatConversion,
    SerdeIntegerConversion, SerdeInternal, SignedFloat, SignedInteger, SignedVariableFloat,
    SignedVariableInteger, UnsignedFloat, UnsignedInteger, UnsignedVariableFloat,
    UnsignedVariableInteger, MTU_SIZE_BITS, MTU_SIZE_BYTES,
};
pub use naia_socket_shared::{
    generate_identity_token, link_condition_logic, IdentityToken, Instant, LinkConditionerConfig,
    Random, SocketConfig, TimeQueue,
};

// Re-export bevy_ecs when bevy_support is active so the Replicate derive can
// reference it as `naia_shared::bevy_ecs::...` — makes non-Bevy downstream
// crates compile correctly under workspace-wide feature unification.
#[cfg(feature = "bevy_support")]
pub use bevy_ecs;

#[cfg(all(
    feature = "test_time",
    not(all(target_arch = "wasm32", any(feature = "wbindgen", feature = "mquad")))
))]
pub use naia_socket_shared::TestClock;

mod backends;
mod bigmap;
mod connection;

#[cfg(feature = "observability")]
pub const MESSAGES_SENT_TOTAL: &str = "naia_messages_sent_total";
#[cfg(feature = "observability")]
pub const SERVER_SPAWNS_TOTAL: &str = "naia_server_spawns_total";
#[cfg(feature = "observability")]
pub const SERVER_DESPAWNS_TOTAL: &str = "naia_server_despawns_total";
#[cfg(feature = "observability")]
pub const SERVER_COMPONENT_INSERTS_TOTAL: &str = "naia_server_component_inserts_total";
#[cfg(feature = "observability")]
pub const SERVER_COMPONENT_REMOVES_TOTAL: &str = "naia_server_component_removes_total";
mod constants;
mod game_time;
pub mod handshake;
mod key_generator;
mod messages;
mod named;
mod protocol;
mod protocol_id;
mod sequence_list;
mod types;
mod world;
mod wrapping_number;

cfg_if! {
    if #[cfg(any(feature = "transport_udp", feature = "transport_local"))]{
        pub mod transport;
        pub use transport as http_utils;
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
    bandwidth::BandwidthConfig,
    bandwidth_monitor::BandwidthMonitor,
    base_connection::BaseConnection,
    compression_config::{CompressionConfig, CompressionMode},
    connection_config::ConnectionConfig,
    connection_stats::ConnectionStats,
    decoder::Decoder,
    encoder::Encoder,
    entity_priority::{EntityPriorityMut, EntityPriorityRef},
    loss_monitor::LossMonitor,
    priority_state::{GlobalPriorityState, OutgoingPriorityHook, UserPriorityState},
    packet_notifiable::PacketNotifiable,
    packet_type::PacketType,
    ping_store::{PingIndex, PingStore},
    standard_header::StandardHeader,
};
pub use messages::{
    channels::{
        channel::{
            Channel, ChannelCriticality, ChannelDirection, ChannelMode, ChannelSettings,
            ReliableSettings, TickBufferSettings,
        },
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
    message::{Message, Message as MessageBevy, MessageBuilder},
    message_container::MessageContainer,
    message_kinds::{MessageKind, MessageKinds},
    message_manager::MessageManager,
    request::{
        GlobalRequestId, GlobalResponseId, Request, Response, ResponseReceiveKey, ResponseSendKey,
    },
};
pub use named::Named;
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
            Replicate, Replicate as ReplicateBevy, ReplicateBuilder,
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
    resource::{ResourceKinds, ResourceRegistry, resource_registry::ResourceAlreadyExists},
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
pub use protocol_id::ProtocolId;
pub use types::{DisconnectReason, HostType, MessageIndex, PacketIndex, ShortMessageIndex, Tick};
pub use world::entity_command::EntityCommand;
pub use world::publicity::Publicity;
pub use world::entity_index::{EntityIndex, KeyGenerator32};
pub use world::entity_event::EntityEvent;
pub use world::host::host_entity_generator::HostEntityGenerator;
pub use world::host::host_world_manager::SubCommandId;
pub use world::local::local_entity::{HostEntity, OwnedLocalEntity, RemoteEntity};
pub use world::local::local_entity_map::LocalEntityMap;
pub use world::local::local_world_manager::LocalWorldManager;
pub use world::world_reader::WorldReader;
pub use world::sync::auth_channel::EntityAuthChannelState;
pub use world::sync::authority_error::AuthorityError;
pub use world::sync::host_entity_channel::HostEntityChannel;
#[cfg(feature = "e2e_debug")]
pub use world::sync::remote_entity_channel::EntityChannelState;
pub use world::sync::remote_entity_channel::RemoteEntityChannel;
pub use world::update::component_update::{ComponentFieldUpdate, ComponentUpdate};
pub use world::update::diff_mask::DiffMask;
pub use world::update::global_diff_handler::GlobalDiffHandler;
pub use world::update::mut_channel::{MutChannelType, MutReceiver};
#[cfg(feature = "bench_instrumentation")]
pub use world::update::mut_channel::{DirtyNotifier, DirtyQueue, DirtySet};
#[cfg(feature = "bench_instrumentation")]
pub use world::local::local_world_manager::bench_take_events_counters;
#[cfg(feature = "bench_instrumentation")]
pub use world::local::local_world_manager::cmd_emission_counters;
#[cfg(feature = "bench_instrumentation")]
pub use world::update::user_diff_handler::dirty_scan_counters;
pub use wrapping_number::{
    sequence_equal_or_greater_than, sequence_equal_or_less_than, sequence_greater_than,
    sequence_less_than, wrapping_diff,
};
