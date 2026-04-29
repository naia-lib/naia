//! Wire bandwidth — static-split savings proof.
//!
//! Proves that routing tiles through the static entity pool keeps dynamic unit
//! IDs in the 9-bit varint range, saving bits per update reference per tick.
//!
//! Two scenarios are compared:
//!
//! | Scenario | Tile IDs | Unit IDs | Bits/unit-ref |
//! |----------|----------|----------|---------------|
//! | control  | dyn 0..9999 | dyn 10000..10031 | 18 (17-bit varint + is_host + is_static) |
//! | treatment| static 0..9999 | dyn 0..31 | 10 (8-bit varint + is_host + is_static) |
//!
//! CPU gate (criterion): treatment time/tick ≤ control time/tick.
//! Bytes gate (test): see `tests/local_entity_wire.rs` static_split_saves_8_bits_per_dynamic_ref*

use criterion::{criterion_group, Criterion, Throughput};

use naia_benches::BenchWorldBuilder;

const TILE_COUNT: usize = 10_000;
const UNIT_COUNT: usize = 32;
const STEADY_STATE_TICKS: usize = 120; // 2× the warmup criterion does internally

/// Build a control world (all dynamic) to steady state and return it.
/// Tiles occupy IDs 0..TILE_COUNT in the dynamic pool, pushing unit IDs to TILE_COUNT+.
fn build_control() -> (naia_benches::BenchWorld, std::ops::Range<usize>) {
    let mut world = BenchWorldBuilder::new()
        .users(1)
        .entities(TILE_COUNT + UNIT_COUNT)
        .uncapped_bandwidth()
        .build();

    let unit_range = TILE_COUNT..(TILE_COUNT + UNIT_COUNT);

    for _ in 0..STEADY_STATE_TICKS {
        world.mutate_entity_range(unit_range.clone());
        world.tick();
    }

    (world, unit_range)
}

/// Build a treatment world (static tiles, dynamic units) to steady state and return it.
/// Tile IDs are in the static pool; unit IDs start from 0 in the dynamic pool.
fn build_treatment() -> (naia_benches::BenchWorld, std::ops::Range<usize>) {
    let mut world = BenchWorldBuilder::new()
        .users(1)
        .static_entities(TILE_COUNT)
        .entities(UNIT_COUNT)
        .uncapped_bandwidth()
        .build();

    let unit_range = TILE_COUNT..(TILE_COUNT + UNIT_COUNT);

    for _ in 0..STEADY_STATE_TICKS {
        world.mutate_entity_range(unit_range.clone());
        world.tick();
    }

    (world, unit_range)
}

// ── Control scenario ──────────────────────────────────────────────────────────
fn bench_control(c: &mut Criterion) {
    let mut group = c.benchmark_group("wire/bandwidth_static_split");

    // Throughput::Elements(UNIT_COUNT): criterion reports "time per unit mutation".
    // Identical denominator in control and treatment → directly comparable.
    group.throughput(Throughput::Elements(UNIT_COUNT as u64));

    group.bench_function("control_all_dynamic_10k_tiles", |b| {
        // Build + steady-state warmup is in iter_batched setup so it's never timed.
        b.iter_batched(
            build_control,
            |(mut world, unit_range)| {
                world.mutate_entity_range(unit_range);
                world.tick();
            },
            criterion::BatchSize::SmallInput,
        )
    });

    group.finish();
}

// ── Treatment scenario ────────────────────────────────────────────────────────
fn bench_treatment(c: &mut Criterion) {
    let mut group = c.benchmark_group("wire/bandwidth_static_split");

    group.throughput(Throughput::Elements(UNIT_COUNT as u64));

    group.bench_function("treatment_static_tiles_10k", |b| {
        b.iter_batched(
            build_treatment,
            |(mut world, unit_range)| {
                world.mutate_entity_range(unit_range);
                world.tick();
            },
            criterion::BatchSize::SmallInput,
        )
    });

    group.finish();
}

criterion_group!(
    wire_bandwidth_static_split,
    bench_control,
    bench_treatment,
);
