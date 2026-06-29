//! Cyberlith `halo_btb_16v16_10k` capacity scenario.
//!
//! Models a realistic cyberlith game room at 25 Hz:
//!   - 16 connected players
//!   - 10 000 immutable HaloTile entities (the level)
//!   - 32 mutable HaloUnit entities (one per player, 16v16)
//!
//! Four measurement phases answer different capacity questions:
//!
//! | Bench ID                          | Question                                       |
//! |-----------------------------------|------------------------------------------------|
//! | `level_load`                      | How long does the level take to reach clients? |
//! | `steady_state_idle`               | Server tick cost when nothing moves            |
//! | `steady_state_active`             | Server tick cost when all units move           |
//! | `client_receive_active`           | Per-client cost receiving an active tick       |
//!
//! Results feed `naia-bench-report --capacity-report` to produce a
//! "concurrent games at 25 Hz" estimate.
//!
//! # Benchmark design
//!
//! Criterion calls the `bench_function` closure multiple times (once for
//! warm-up, once per sample). To avoid rebuilding the world on every call,
//! the three steady-state benches declare the world as `Option<BenchWorld>`
//! in the enclosing function and lazily initialise it on the first closure
//! invocation — the world then persists across all warm-up and sample runs.
//!
//! `level_load` intentionally rebuilds each time: that IS what it measures.
//! It uses `iter_custom` with `sample_size(10)` so criterion runs exactly
//! 10 level-load iterations (+ 1 warm-up) for a manageable total time.

use std::time::Duration;

use criterion::{criterion_group, Criterion};
use naia_benches::{BenchWorld, BenchWorldBuilder};

const PLAYERS: usize = 16;
const TILE_COUNT: usize = 10_000;
const UNIT_COUNT: usize = 32;
const TICK_HZ: u16 = 25;

fn new_world() -> BenchWorld {
    let mut w = BenchWorldBuilder::new()
        .users(PLAYERS)
        .tick_rate_hz(TICK_HZ)
        .uncapped_bandwidth()
        .build();
    w.spawn_halo_scene(TILE_COUNT, UNIT_COUNT);
    w
}

/// Phase A — level load.
///
/// Measures wall time from "16 clients connected, no entities" to
/// "10K tiles + 32 units fully replicated to all 16 clients".
/// Each iteration rebuilds from scratch — that IS the measurement.
fn bench_level_load(c: &mut Criterion) {
    let mut group = c.benchmark_group("scenarios/halo_btb_16v16");
    group.sample_size(10);

    group.bench_function("level_load", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                let mut world = BenchWorldBuilder::new()
                    .users(PLAYERS)
                    .tick_rate_hz(TICK_HZ)
                    .uncapped_bandwidth()
                    .build();
                let t = std::time::Instant::now();
                world.spawn_halo_scene(TILE_COUNT, UNIT_COUNT);
                total += t.elapsed();
            }
            total
        });
    });

    group.finish();
}

/// Phase B — steady-state idle.
///
/// One full server tick with 16 players, 10K tiles, 32 units, zero mutations.
/// World is built ONCE (lazy, on the first criterion closure invocation) and
/// reused across all warm-up and sample runs.
fn bench_steady_state_idle(c: &mut Criterion) {
    let mut group = c.benchmark_group("scenarios/halo_btb_16v16");

    let mut world: Option<BenchWorld> = None;
    group.bench_function("steady_state_idle", |b| {
        let w = world.get_or_insert_with(new_world);
        b.iter(|| w.tick());
    });

    group.finish();
}

/// Phase C — steady-state active.
///
/// One full server tick with all 32 units mutating every tick.
/// World is built ONCE and reused. Mutations accumulate and flush each tick.
fn bench_steady_state_active(c: &mut Criterion) {
    let mut group = c.benchmark_group("scenarios/halo_btb_16v16");

    let mut world: Option<BenchWorld> = None;
    group.bench_function("steady_state_active", |b| {
        let w = world.get_or_insert_with(new_world);
        b.iter(|| {
            w.mutate_halo_units(UNIT_COUNT);
            w.tick();
        });
    });

    group.finish();
}

/// Phase D — per-client receive (active load).
///
/// Server runs a full active tick (32 mutations), then we time exactly one
/// client's receive path in isolation. World is built ONCE and reused.
/// Only the client-receive duration is accumulated per iteration.
fn bench_client_receive_active(c: &mut Criterion) {
    let mut group = c.benchmark_group("scenarios/halo_btb_16v16");

    let mut world: Option<BenchWorld> = None;
    group.bench_function("client_receive_active", |b| {
        let w = world.get_or_insert_with(new_world);
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                w.mutate_halo_units(UNIT_COUNT);
                total += w.tick_server_then_measure_one_client(0);
            }
            total
        });
    });

    group.finish();
}

criterion_group!(
    halo_btb,
    bench_level_load,
    bench_steady_state_idle,
    bench_steady_state_active,
    bench_client_receive_active,
);
