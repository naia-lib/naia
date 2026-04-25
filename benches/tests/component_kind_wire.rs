//! Phase 8.3 — wire-format tests for the protocol-sized `ComponentKind`
//! NetId encoding.
//!
//! `ComponentKind::ser` switched from a fixed `u16` (16 bits regardless
//! of how many kinds were actually registered) to a fixed-width raw bit
//! field whose width is derived from the protocol's registered kind
//! count: `bits = ceil(log2(N))`. Both ends share the same registration
//! order, so both compute the same width.
//!
//! | Registered kinds N | Bits per tag |
//! |--------------------|-------------:|
//! | 0..1               |            0 |
//! | 2                  |            1 |
//! | 3..4               |            2 |
//! | 5..8               |            3 |
//! | 9..16              |            4 |
//! | 17..32             |            5 |
//! | 33..64             |            6 |
//! | 65..128            |            7 |
//!
//! For the bench protocol's 8 components every tag is exactly 3 bits — a
//! 13/16 = 81% reduction from the prior 16-bit encoding, AND strictly
//! cheaper than a varint<3> (which would have paid 4 bits even for the
//! same 8-kind case because of its proceed-bit overhead).

use naia_shared::{BitCounter, BitReader, BitWriter, ComponentKind, ComponentKinds};

use naia_benches::bench_protocol::{
    BenchComponent, BenchImmutableComponent, Position, PositionQ, Rotation, RotationQ, Velocity,
    VelocityQ,
};

/// Build a fresh `ComponentKinds` registry holding the 8 bench-protocol
/// components — NetIds are assigned 0..7 in registration order.
fn build_8_kind_registry() -> ComponentKinds {
    let mut kinds = ComponentKinds::new();
    kinds.add_component::<BenchComponent>();
    kinds.add_component::<BenchImmutableComponent>();
    kinds.add_component::<Position>();
    kinds.add_component::<Velocity>();
    kinds.add_component::<Rotation>();
    kinds.add_component::<PositionQ>();
    kinds.add_component::<VelocityQ>();
    kinds.add_component::<RotationQ>();
    kinds
}

fn ser_bit_count(kinds: &ComponentKinds, kind: ComponentKind) -> u32 {
    let mut counter = BitCounter::new(0, 0, u32::MAX);
    kind.ser(kinds, &mut counter);
    counter.bits_needed()
}

#[test]
fn protocol_with_8_kinds_emits_3_bit_kind_tags() {
    let kinds = build_8_kind_registry();
    for kind in [
        ComponentKind::of::<BenchComponent>(),
        ComponentKind::of::<BenchImmutableComponent>(),
        ComponentKind::of::<Position>(),
        ComponentKind::of::<Velocity>(),
        ComponentKind::of::<Rotation>(),
        ComponentKind::of::<PositionQ>(),
        ComponentKind::of::<VelocityQ>(),
        ComponentKind::of::<RotationQ>(),
    ] {
        assert_eq!(
            ser_bit_count(&kinds, kind),
            3,
            "kind {kind:?} (8-kind protocol → 3-bit tag) should serialize as 3 bits, not 16"
        );
    }
}

#[test]
fn protocol_with_8_kinds_round_trips_through_writer_and_reader() {
    let kinds = build_8_kind_registry();
    for kind in [
        ComponentKind::of::<BenchComponent>(),
        ComponentKind::of::<BenchImmutableComponent>(),
        ComponentKind::of::<Position>(),
        ComponentKind::of::<Velocity>(),
        ComponentKind::of::<Rotation>(),
        ComponentKind::of::<PositionQ>(),
        ComponentKind::of::<VelocityQ>(),
        ComponentKind::of::<RotationQ>(),
    ] {
        let mut writer = BitWriter::with_max_capacity();
        kind.ser(&kinds, &mut writer);
        let bytes = writer.to_bytes();
        let mut reader = BitReader::new(&bytes);
        let decoded = ComponentKind::de(&kinds, &mut reader)
            .expect("ComponentKind round-trip should not error");
        assert_eq!(decoded, kind, "ComponentKind round-trip mismatch");
    }
}

#[test]
fn bit_width_scales_with_registered_kind_count() {
    // Build registries of varying sizes and check that the per-tag bit
    // count tracks ceil(log2(N)).
    fn first_kind_bits(n: usize) -> u32 {
        let mut kinds = ComponentKinds::new();
        // We only need *some* type to register; reuse the bench protocol's
        // distinct types. n is bounded by our 8 available types.
        if n >= 1 { kinds.add_component::<BenchComponent>(); }
        if n >= 2 { kinds.add_component::<BenchImmutableComponent>(); }
        if n >= 3 { kinds.add_component::<Position>(); }
        if n >= 4 { kinds.add_component::<Velocity>(); }
        if n >= 5 { kinds.add_component::<Rotation>(); }
        if n >= 6 { kinds.add_component::<PositionQ>(); }
        if n >= 7 { kinds.add_component::<VelocityQ>(); }
        if n >= 8 { kinds.add_component::<RotationQ>(); }
        ser_bit_count(&kinds, ComponentKind::of::<BenchComponent>())
    }
    // 1 kind → 0 bits (degenerate; nothing to disambiguate).
    assert_eq!(first_kind_bits(1), 0);
    // 2 kinds → 1 bit.
    assert_eq!(first_kind_bits(2), 1);
    // 3..4 kinds → 2 bits.
    assert_eq!(first_kind_bits(3), 2);
    assert_eq!(first_kind_bits(4), 2);
    // 5..8 kinds → 3 bits.
    assert_eq!(first_kind_bits(5), 3);
    assert_eq!(first_kind_bits(8), 3);
}

#[test]
fn round_trip_works_at_every_bit_width_tier() {
    // Walk through every size 1..=8 and round-trip every registered kind.
    for n in 1..=8usize {
        let mut kinds = ComponentKinds::new();
        let mut registered: Vec<ComponentKind> = Vec::new();
        macro_rules! add { ($t:ty) => {{
            kinds.add_component::<$t>();
            registered.push(ComponentKind::of::<$t>());
        }}; }
        if n >= 1 { add!(BenchComponent); }
        if n >= 2 { add!(BenchImmutableComponent); }
        if n >= 3 { add!(Position); }
        if n >= 4 { add!(Velocity); }
        if n >= 5 { add!(Rotation); }
        if n >= 6 { add!(PositionQ); }
        if n >= 7 { add!(VelocityQ); }
        if n >= 8 { add!(RotationQ); }
        for kind in &registered {
            let mut writer = BitWriter::with_max_capacity();
            kind.ser(&kinds, &mut writer);
            let bytes = writer.to_bytes();
            let mut reader = BitReader::new(&bytes);
            let decoded = ComponentKind::de(&kinds, &mut reader)
                .expect("round-trip should not error");
            assert_eq!(decoded, *kind, "round-trip mismatch with n={n}");
        }
    }
}
