//! Wire bandwidth — static-split savings proof.
//!
//! Proves that routing tiles through the static entity pool keeps dynamic unit
//! IDs in the 9-bit varint range, saving bits per update reference per tick.
//!
//! Two scenarios are compared via criterion `BenchmarkGroup::throughput`:
//!
//! | Scenario | Tile IDs | Unit IDs | Bits/unit-ref |
//! |----------|----------|----------|---------------|
//! | control  | dyn 0..9999 | dyn 10000..10031 | 18 (17-bit varint) |
//! | treatment| static 0..9999 | dyn 0..31 | 10 (8-bit varint)  |
//!
//! Gate: treatment bytes/tick ≤ control bytes/tick.
//! Expected saving: ≥ 8 bits × 32 units = 256 bits = 32 bytes/tick per client.

use criterion::{criterion_group, Criterion, Throughput};

use naia_benches::BenchWorldBuilder;

const TILE_COUNT: usize = 10_000;
const UNIT_COUNT: usize = 32;

// ── Control scenario ──────────────────────────────────────────────────────────
// No static pool: tile entities occupy dynamic IDs 0..9999, so unit entities
// get dynamic IDs 10_000..10_031 (17-bit varint range → 18 bits/ref with is_host + is_static).
fn bench_control(c: &mut Criterion) {
    let mut group = c.benchmark_group("wire/bandwidth_static_split");

    // Control: all dynamic. Spawn TILE_COUNT dummy dynamic tiles to push unit IDs high.
    // We measure only the unit mutation bytes (not the tile bytes) since tiles are
    // immutable and not diff-tracked — this isolates the per-unit-ID cost.
    let mut world = BenchWorldBuilder::new()
        .users(1)
        .entities(TILE_COUNT + UNIT_COUNT) // all dynamic — tiles push unit IDs to 10K+
        .uncapped_bandwidth()
        .build();

    // The last UNIT_COUNT entities are "units" — mutate only those each tick.
    let unit_range = TILE_COUNT..(TILE_COUNT + UNIT_COUNT);

    // Warmup
    for _ in 0..60 {
        world.mutate_entity_range(unit_range.clone());
        world.tick();
    }

    let bytes_after_warmup = world.server_outgoing_bytes_per_tick();
    group.throughput(Throughput::Bytes(bytes_after_warmup as u64));

    group.bench_function("control_all_dynamic_10k_tiles", |b| {
        b.iter(|| {
            world.mutate_entity_range(unit_range.clone());
            world.tick();
        })
    });

    group.finish();
}

// ── Treatment scenario ────────────────────────────────────────────────────────
// Static pool: tile entities use static IDs (separate pool), unit entities use
// dynamic IDs starting from 0. Unit IDs are 0..31 (9-bit varint → 10 bits/ref).
fn bench_treatment(c: &mut Criterion) {
    let mut group = c.benchmark_group("wire/bandwidth_static_split");

    let mut world = BenchWorldBuilder::new()
        .users(1)
        .static_entities(TILE_COUNT)  // tiles → static pool; unit IDs reset to 0
        .entities(UNIT_COUNT)         // units → dynamic pool starting at 0
        .uncapped_bandwidth()
        .build();

    // Only dynamic (unit) entities exist in the entity range to mutate.
    // static_entity_count entities are at indices 0..TILE_COUNT (static, never mutated).
    // dynamic entities are at indices TILE_COUNT..TILE_COUNT+UNIT_COUNT.
    let unit_range = TILE_COUNT..(TILE_COUNT + UNIT_COUNT);

    // Warmup
    for _ in 0..60 {
        world.mutate_entity_range(unit_range.clone());
        world.tick();
    }

    let bytes_after_warmup = world.server_outgoing_bytes_per_tick();
    group.throughput(Throughput::Bytes(bytes_after_warmup as u64));

    group.bench_function("treatment_static_tiles_10k", |b| {
        b.iter(|| {
            world.mutate_entity_range(unit_range.clone());
            world.tick();
        })
    });

    group.finish();
}

criterion_group!(
    wire_bandwidth_static_split,
    bench_control,
    bench_treatment,
);
