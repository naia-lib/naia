pub use bevy_ecs;

pub use naia_shared::{
    sequence_greater_than, sequence_less_than, wrapping_diff, AuthorityError, BitReader, BitWrite,
    BitWriter,
    BandwidthConfig, Channel, ChannelDirection, ChannelKind, ChannelMode, ComponentFieldUpdate,
    ComponentKind, ComponentKinds, PendingComponentUpdate, CompressionConfig, CompressionMode,
    ConstBitLength, DiffMask, EntityAndGlobalEntityConverter, EntityAuthAccessor,
    EntityAuthStatus, EntityDoesNotExistError, EntityProperty, FakeEntityConverter, FileBitWriter,
    GameInstant, GlobalEntity, HostEntity, HostEntityAuthStatus, Instant, LinkConditionerConfig,
    LocalEntityAndGlobalEntityConverter, LocalEntityAndGlobalEntityConverterMut, LocalEntityMap,
    MessageBevy as Message, MessageBuilder, MessageContainer, MessageKind, MessageKinds, Named,
    OwnedBitReader, Property, PropertyMutate, PropertyMutator, Random, ReliableSettings,
    RemoteEntity, ReplicaDynMut, ReplicaDynRef, ReplicateBevy as Replicate, ReplicateBuilder,
    Request, ResourceAlreadyExists, ResourceKinds, ResourceRegistry, Response, ResponseReceiveKey,
    ResponseSendKey, SerdeBevyShared as Serde, SerdeErr, SerdeFloatConversion,
    SerdeIntegerConversion, SignedFloat, SignedInteger, SignedVariableFloat, SignedVariableInteger,
    Tick, TickBufferSettings, Timer, UnsignedFloat, UnsignedInteger, UnsignedVariableFloat,
    UnsignedVariableInteger, WorldMutType, WorldRefType, MTU_SIZE_BYTES,
};

mod bundle;
mod change_detection;
mod component_access;
mod component_ref;
mod components;
mod plugin;
mod protocol;
mod protocol_plugin;
mod replicated_resource;
mod system_set;
mod world_data;
mod world_op_command;
mod world_proxy;

pub use bundle::ReplicateBundle;
pub use replicated_resource::ReplicatedResource;
pub use world_op_command::WorldOpCommand;

/// Re-export of `naia_shared::TestClock` for bevy-app integration
/// tests that need to drive naia ticks deterministically. Available
/// only with the `test_time` feature on this crate (or transitively
/// via `naia-bevy-server` / `naia-bevy-client`).
#[cfg(all(feature = "test_time", not(target_arch = "wasm32")))]
pub use naia_shared::TestClock;
pub use change_detection::HostSyncEvent;
pub use component_access::{AppTag, ComponentAccess, ComponentAccessor};
pub use components::{HostOwned, HostOwnedMap};
pub use plugin::SharedPlugin;
pub use protocol::Protocol;
pub use protocol_plugin::ProtocolPlugin;
pub use system_set::{
    HandleTickEvents, HandleWorldEvents, HostSyncChangeTracking, HostSyncOwnedAddedTracking,
    ProcessPackets, ReceivePackets, SendPackets, TranslateTickEvents, TranslateWorldEvents,
    WorldToHostSync, WorldUpdate,
};
pub use world_data::WorldData;
pub use world_proxy::{WorldMut, WorldProxy, WorldProxyMut, WorldRef};
