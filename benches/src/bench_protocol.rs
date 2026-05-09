use naia_shared::{
    Channel, ChannelDirection, ChannelMode, Message, Property, Protocol, ReliableSettings,
    Replicate, Serde, SignedVariableFloat,
};

use crate::serde_quat::BenchQuat;

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

// ─── Quantized realistic-archetype components ────────────────────────────────
//
// Mirror cyberlith's production wire encoding for `wire/bandwidth_realistic_quantized`.
// Cyberlith wraps multi-axis state into a single `Property<State>` (see
// `cyberlith/services/game/naia_proto/src/components/networked/{position,velocity,rotation}.rs`),
// so mutation tracking is at the whole-component level — one DiffMask bit per
// component, not per axis. The encodings:
//   - Position: i16 tile × 3  + SignedVariableFloat<14, 0> delta × 3 (~93–138 bits/state)
//   - Velocity: SignedVariableFloat<11, 2> × 3  (~3–39 bits/state)
//   - Rotation: BenchQuat smallest-three (~21 bits/state, mirrors `SerdeQuat`)

#[derive(Serde, PartialEq, Clone)]
pub struct PositionQState {
    pub tile_x: i16,
    pub tile_y: i16,
    pub tile_z: i16,
    pub dx: SignedVariableFloat<14, 0>,
    pub dy: SignedVariableFloat<14, 0>,
    pub dz: SignedVariableFloat<14, 0>,
}

#[derive(Replicate)]
pub struct PositionQ {
    pub state: Property<PositionQState>,
}

impl PositionQ {
    pub fn new(x: f32, y: f32, z: f32) -> Self {
        Self::new_complete(PositionQState {
            tile_x: x.round() as i16,
            tile_y: y.round() as i16,
            tile_z: z.round() as i16,
            dx: SignedVariableFloat::new(0.0),
            dy: SignedVariableFloat::new(0.0),
            dz: SignedVariableFloat::new(0.0),
        })
    }
}

#[derive(Serde, PartialEq, Clone)]
pub struct VelocityQState {
    pub vx: SignedVariableFloat<11, 2>,
    pub vy: SignedVariableFloat<11, 2>,
    pub vz: SignedVariableFloat<11, 2>,
}

#[derive(Replicate)]
pub struct VelocityQ {
    pub state: Property<VelocityQState>,
}

impl VelocityQ {
    pub fn new(x: f32, y: f32, z: f32) -> Self {
        Self::new_complete(VelocityQState {
            vx: SignedVariableFloat::new(x),
            vy: SignedVariableFloat::new(y),
            vz: SignedVariableFloat::new(z),
        })
    }
}

#[derive(Replicate)]
pub struct RotationQ {
    pub state: Property<BenchQuat>,
}

impl RotationQ {
    pub fn new(x: f32, y: f32, z: f32, w: f32) -> Self {
        Self::new_complete(BenchQuat::new(x, y, z, w))
    }
}

// ─── Benchmark resource ───────────────────────────────────────────────────────

/// Delta-tracked resource used by `resources/throughput` benchmarks.
/// Registered via `add_resource` — replicated as a hidden entity to every
/// connected client.
#[derive(Replicate)]
pub struct BenchResource {
    pub value: Property<u32>,
}

impl BenchResource {
    pub fn new(v: u32) -> Self {
        Self::new_complete(v)
    }
}

// ─── Halo scenario components ─────────────────────────────────────────────────
//
// Used by `scenarios/halo_btb_16v16`: a cyberlith-shaped scenario with 10K
// immutable tiles and 32 mutable units per room.

/// Immutable tile — stands in for cyberlith's NetworkedTile.
/// No properties: presence is the data (zero diff-tracking cost per tick).
#[derive(Replicate)]
#[replicate(immutable)]
pub struct HaloTile;

/// Mutable unit — stands in for a moving character (position + facing).
#[derive(Replicate)]
pub struct HaloUnit {
    pub pos: Property<[i16; 2]>,
    pub facing: Property<u8>,
}

impl HaloUnit {
    pub fn new(x: i16, y: i16, facing: u8) -> Self {
        Self::new_complete([x, y], facing)
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
        .add_component::<PositionQ>()
        .add_component::<VelocityQ>()
        .add_component::<RotationQ>()
        .add_component::<HaloTile>()
        .add_component::<HaloUnit>()
        .add_resource::<BenchResource>()
        .add_message::<BenchAuth>()
        .add_channel::<BenchChannel>(
            ChannelDirection::Bidirectional,
            ChannelMode::UnorderedReliable(ReliableSettings::default()),
        )
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn halo_unit_new_round_trips() {
        let u = HaloUnit::new(5, -3, 128);
        assert_eq!(*u.pos, [5i16, -3i16]);
        assert_eq!(*u.facing, 128u8);
    }

    #[test]
    fn halo_unit_facing_wraps() {
        let mut u = HaloUnit::new(0, 0, 255);
        *u.facing = u.facing.wrapping_add(1);
        assert_eq!(*u.facing, 0u8);
    }
}
