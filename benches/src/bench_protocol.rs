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

// ─── Realistic-archetype components ──────────────────────────────────────────
//
// Used by `wire/bandwidth_realistic`. Three small-but-realistic shapes that
// stand in for typical netgame state: 3-axis position, 3-axis velocity,
// 2-axis camera rotation. All `f32` to mirror standard quantization-naive
// production state. Bench-only — not used by simulation tests.

#[derive(Replicate)]
pub struct Position {
    pub x: Property<f32>,
    pub y: Property<f32>,
    pub z: Property<f32>,
}
impl Position {
    pub fn new(x: f32, y: f32, z: f32) -> Self {
        Self::new_complete(x, y, z)
    }
}

#[derive(Replicate)]
pub struct Velocity {
    pub x: Property<f32>,
    pub y: Property<f32>,
    pub z: Property<f32>,
}
impl Velocity {
    pub fn new(x: f32, y: f32, z: f32) -> Self {
        Self::new_complete(x, y, z)
    }
}

#[derive(Replicate)]
pub struct Rotation {
    pub yaw: Property<f32>,
    pub pitch: Property<f32>,
}
impl Rotation {
    pub fn new(yaw: f32, pitch: f32) -> Self {
        Self::new_complete(yaw, pitch)
    }
}

// ─── Channel ─────────────────────────────────────────────────────────────────

#[derive(Channel)]
pub struct BenchChannel;

// ─── Protocol ─────────────────────────────────────────────────────────────────

pub fn bench_protocol() -> Protocol {
    Protocol::builder()
        .enable_client_authoritative_entities()
        .add_component::<BenchComponent>()
        .add_component::<BenchImmutableComponent>()
        .add_component::<Position>()
        .add_component::<Velocity>()
        .add_component::<Rotation>()
        .add_message::<BenchAuth>()
        .add_channel::<BenchChannel>(
            ChannelDirection::Bidirectional,
            ChannelMode::UnorderedReliable(ReliableSettings::default()),
        )
        .build()
}
