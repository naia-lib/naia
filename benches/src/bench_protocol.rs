use naia_shared::{
    Channel, ChannelDirection, ChannelMode, Message, Property, Protocol, ReliableSettings, Replicate,
};

// ─── Entity type ─────────────────────────────────────────────────────────────

/// Minimal entity type for benchmarks.
pub type BenchEntity = naia_demo_world::Entity;

// ─── Auth message ─────────────────────────────────────────────────────────────

/// Empty auth message — benchmarks auto-accept all connections.
#[derive(Message)]
pub struct BenchAuth;

// ─── Components ───────────────────────────────────────────────────────────────

/// Mutable benchmark component. Used by all active-workload benchmarks.
#[derive(Replicate)]
pub struct BenchComponent {
    pub value: Property<u32>,
}

impl BenchComponent {
    pub fn new(v: u32) -> Self {
        Self::new_complete(v)
    }
}

/// Immutable benchmark component. Used by Win-5 (zero-allocation) benchmarks.
#[derive(Replicate)]
#[replicate(immutable)]
pub struct BenchImmutableComponent;

// ─── Channel ─────────────────────────────────────────────────────────────────

#[derive(Channel)]
pub struct BenchChannel;

// ─── Protocol ─────────────────────────────────────────────────────────────────

pub fn bench_protocol() -> Protocol {
    Protocol::builder()
        .enable_client_authoritative_entities()
        .add_component::<BenchComponent>()
        .add_component::<BenchImmutableComponent>()
        .add_message::<BenchAuth>()
        .add_channel::<BenchChannel>(
            ChannelDirection::Bidirectional,
            ChannelMode::UnorderedReliable(ReliableSettings::default()),
        )
        .build()
}
