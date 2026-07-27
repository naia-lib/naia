pub use bevy_ecs;

// Bevy-adapter `Replicate` derive: component=false flavor (users add their own
// `#[derive(Component)]` alongside, so we skip the auto Component impl).
// This goes into the MACRO namespace as `Replicate`.
pub use naia_bevy_derive::Replicate;

// The `Replicate` TRAIT — imported from naia_shared under its `ReplicateTrait`
// alias (which is a pure type re-export) so Rust sees only a TYPE-namespace
// import here and does not conflict with the macro-namespace derive above.
// We then re-export it as `Replicate` in the type namespace; together with the
// derive above, both namespaces hold `naia_bevy_shared::Replicate`, mirroring
// how naia_shared itself dual-exports the name.
pub use naia_shared::ReplicateTrait as Replicate;

pub use naia_shared::{
    sequence_greater_than, sequence_less_than, wrapping_diff, AuthorityError, BandwidthConfig,
    BitReader, BitWrite, BitWriter, Channel, ChannelDirection, ChannelKind, ChannelMode,
    ComponentFieldUpdate, ComponentKind, ComponentKinds, CompressionConfig, CompressionMode,
    ConstBitLength, DiffMask, EntityAndGlobalEntityConverter, EntityAuthAccessor, EntityAuthStatus,
    EntityDoesNotExistError, EntityProperty, FakeEntityConverter, FileBitWriter, GameInstant,
    GlobalEntity, HostComponent, HostEntity, HostEntityAuthStatus, IdentityToken, Instant,
    LinkConditionerConfig, LocalEntityAndGlobalEntityConverter,
    LocalEntityAndGlobalEntityConverterMut, LocalEntityMap, Message, MessageBuilder,
    MessageContainer, MessageKind, MessageKinds, Named, OwnedBitReader, PendingComponentUpdate,
    Property, PropertyMutate, PropertyMutator, Random, ReliableSettings, RemoteEntity,
    ReplicaDynMut, ReplicaDynRef, ReplicaDynRefWrapper, ReplicaRefWrapper, ReplicateBuilder,
    ReplicatedComponent, Request, ResourceAlreadyExists, ResourceKinds, ResourceRegistry, Response,
    ResponseReceiveKey, ResponseSendKey, SerdeBevyShared as Serde, SerdeErr, SerdeFloatConversion,
    SerdeIntegerConversion, SignedFloat, SignedInteger, SignedVariableFloat, SignedVariableInteger,
    SnapshotWorld, Tick, TickBufferSettings, Timer, UnsignedFloat, UnsignedInteger,
    UnsignedVariableFloat, UnsignedVariableInteger, WorldMutType, WorldRefType,
    CACHED_UPDATE_BITS, CACHED_UPDATE_BYTES, MTU_SIZE_BYTES,
    // Named by `#[derive(Replicate)]`'s generated `max_bit_length`, which
    // resolves everything through this crate for bevy consumers.
    MaxBits, MaxBitsFallback, UNBOUNDED_BIT_LENGTH,
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
mod snapshot_reader_registry;
mod system_set;
mod world_data;
mod world_op_command;
mod world_proxy;

pub use bundle::ReplicateBundle;
pub use replicated_resource::ReplicatedResource;
pub use snapshot_reader_registry::SnapshotReaderRegistry;
pub use world_op_command::WorldOpCommand;

pub use change_detection::{on_despawn, on_host_owned_added, HostSyncEvent};
pub use component_access::{AppTag, ComponentAccess, ComponentAccessor};
pub use components::{HostOwned, HostOwnedMap};
/// Re-export of `naia_shared::TestClock` for bevy-app integration
/// tests that need to drive naia ticks deterministically. Available
/// only with the `test_time` feature on this crate (or transitively
/// via `naia-bevy-server` / `naia-bevy-client`).
#[cfg(all(feature = "test_time", not(target_arch = "wasm32")))]
pub use naia_shared::TestClock;
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

/// Re-export of the in-process local transport (`LocalTransportHub`,
/// `FAKE_SERVER_ADDR`, etc.) so consumers wiring up the local transport
/// (test harnesses, local-transport app builds) depend only on this adapter
/// and never reach past it into `naia-shared` directly. Gated to match
/// `naia_shared`'s own `transport_local` cfg.
#[cfg(feature = "transport_local")]
pub use naia_shared::transport;
