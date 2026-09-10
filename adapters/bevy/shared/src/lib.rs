pub use bevy_ecs;

// Bevy-adapter `Replicate` derive: component=false flavor (users add their own
// `#[derive(Component)]` alongside, so we skip the auto Component impl).
// This goes into the MACRO namespace as `Replicate`.
pub use naia_bevy_derive::Replicate;

// Bevy-adapter `Channel`/`Message` derives: same `*_impl` traversal as the
// shared flavor, emitting `naia_bevy_shared::` paths instead of
// `naia_shared::`. These go into the MACRO namespace. Without them, the
// `Channel`/`Message` entries in the `naia_shared` list below would smuggle
// the shared-flavor derives into this macro namespace (a list re-export
// carries all namespaces), and every bevy-tier consumer would silently expand
// to `naia_shared::` paths.
pub use naia_bevy_derive::{Channel, Message};

// The `Replicate` TRAIT — imported from naia_shared under its `ReplicateTrait`
// alias (which is a pure type re-export) so Rust sees only a TYPE-namespace
// import here and does not conflict with the macro-namespace derive above.
// We then re-export it as `Replicate` in the type namespace; together with the
// derive above, both namespaces hold `naia_bevy_shared::Replicate`, mirroring
// how naia_shared itself dual-exports the name.
pub use naia_shared::ReplicateTrait as Replicate;

// The `Channel`/`Message` TRAITs — imported under their pure-type aliases so
// Rust sees only TYPE-namespace imports here, exactly as for `Replicate`
// above. Re-exported below as `Channel`/`Message`, so both namespaces hold
// the bevy meaning and the smuggled shared-flavor derives are gone.
pub use naia_shared::{ChannelTrait as Channel, MessageTrait as Message};

pub use naia_shared::{
    sequence_greater_than,
    sequence_less_than,
    wire_schema_count,
    wire_schema_field,
    wire_schema_label,
    wrapping_diff,
    AuthorityError,
    BandwidthConfig,
    BitReader,
    BitWrite,
    BitWriter,
    ChannelDirection,
    ChannelKind,
    ChannelMode,
    ComponentFieldUpdate,
    ComponentKind,
    ComponentKinds,
    CompressionConfig,
    CompressionMode,
    ConstBitLength,
    DiffMask,
    EntityAndGlobalEntityConverter,
    EntityAuthAccessor,
    EntityAuthStatus,
    EntityDoesNotExistError,
    EntityProperty,
    FakeEntityConverter,
    FileBitWriter,
    GameInstant,
    GlobalEntity,
    HostComponent,
    HostEntity,
    HostEntityAuthStatus,
    IdentityToken,
    Instant,
    LinkConditionerConfig,
    LocalEntityAndGlobalEntityConverter,
    LocalEntityAndGlobalEntityConverterMut,
    LocalEntityMap,
    // Named by `#[derive(Replicate)]`'s generated `max_bit_length`, which
    // resolves everything through this crate for bevy consumers.
    MaxBits,
    MaxBitsFallback,
    MessageBuilder,
    MessageContainer,
    MessageKind,
    MessageKinds,
    Named,
    OwnedBitReader,
    PendingComponentUpdate,
    Property,
    PropertyMutate,
    PropertyMutator,
    Random,
    ReliableSettings,
    RemoteEntity,
    ReplicaDynMut,
    ReplicaDynRef,
    ReplicaDynRefWrapper,
    ReplicaRefWrapper,
    ReplicateBuilder,
    ReplicatedComponent,
    Request,
    ResourceAlreadyExists,
    ResourceKinds,
    ResourceRegistry,
    Response,
    ResponseReceiveKey,
    ResponseSendKey,
    SerdeBevyShared as Serde,
    SerdeErr,
    SerdeFloatConversion,
    SerdeIntegerConversion,
    SignedFloat,
    SignedInteger,
    SignedVariableFloat,
    SignedVariableInteger,
    SnapshotWorld,
    Tick,
    TickBufferSettings,
    Timer,
    UnsignedFloat,
    UnsignedInteger,
    UnsignedVariableFloat,
    UnsignedVariableInteger,
    WireSchema,
    WireSchemaContext,
    WorldMutType,
    WorldRefType,
    CACHED_UPDATE_BITS,
    CACHED_UPDATE_BYTES,
    MTU_SIZE_BYTES,
    SCHEMA_NATIVE_ENDIAN,
    SCHEMA_ORDERED,
    SCHEMA_TAG_ARRAY,
    SCHEMA_TAG_BACKREF,
    SCHEMA_TAG_BOOL,
    SCHEMA_TAG_BYTES,
    SCHEMA_TAG_CHAR,
    SCHEMA_TAG_CUSTOM_LEAF,
    SCHEMA_TAG_ENTITY_PROPERTY,
    SCHEMA_TAG_ENUM,
    SCHEMA_TAG_FLOAT,
    SCHEMA_TAG_HASH_MAP,
    SCHEMA_TAG_HASH_SET,
    SCHEMA_TAG_INTEGER,
    SCHEMA_TAG_NATIVE,
    SCHEMA_TAG_OPTION,
    SCHEMA_TAG_PHANTOM,
    SCHEMA_TAG_STRING,
    SCHEMA_TAG_STRUCT,
    SCHEMA_TAG_TUPLE,
    SCHEMA_TAG_UNIT,
    SCHEMA_TAG_VECTOR,
    SCHEMA_UNORDERED,
    UNBOUNDED_BIT_LENGTH,
    WIRE_SCHEMA_DOMAIN,
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
