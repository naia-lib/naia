use std::time::Duration;

use criterion::{criterion_group, BenchmarkId, Criterion, Throughput};

use naia_benches::BenchWorldBuilder;

const ENTITY_COUNTS: &[usize] = &[100, 500, 1_000, 5_000, 10_000];

// Matrix sweep: U users × N entities. Pins the player-capacity surface for
// tiles-as-immutable-entities. Cells picked to bracket Halo-scale server
// sessions (U=1/4/16) against level-scale tile counts (N=100/1000/10000).
const MATRIX_USERS: &[usize] = &[1, 4, 16];
const MATRIX_ENTITIES: &[usize] = &[100, 1_000, 10_000];

/// Idle-room tick: N entities in scope, 0 mutations.
///
/// Expected shape: **flat line** — time must not grow with entity count.
/// A rising curve means Win-2 or Win-3 regression.
///
/// World is built ONCE per bench cell and reused across iterations (the Bevy
/// idiom for world.update() benches). Heavy `iter_batched(LargeInput)` held
/// ~10 worlds live per batch, pushing 16u×10k cells into swap; the thrash
/// showed up as a phantom O(U·N) tick cost. Steady-state ticks on a prebuilt
/// world measure the actual hot path.
pub fn idle_room_tick(c: &mut Criterion) {
    let mut group = c.benchmark_group("tick/idle");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(5));

    for &n in ENTITY_COUNTS {
        group.throughput(Throughput::Elements(1));
        group.bench_with_input(BenchmarkId::new("entities", n), &n, |b, &n| {
            let mut world = BenchWorldBuilder::new().users(1).entities(n).build();
            b.iter(|| world.tick());
        });
    }
    group.finish();
}

/// Idle-room tick, U×N matrix: K users × N entities in scope, 0 mutations.
///
/// Reveals whether idle cost scales with users, entities, or their product —
/// which determines whether Naia optimization should target per-user or
/// per-entity work first for tiles-as-immutable-entities at Halo-scale.
pub fn idle_room_tick_matrix(c: &mut Criterion) {
    let mut group = c.benchmark_group("tick/idle_matrix");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(5));
    group.sample_size(20); // 9 cells × 5s ≈ 45s wall time; trim samples to keep it bounded

    for &u in MATRIX_USERS {
        for &n in MATRIX_ENTITIES {
            group.throughput(Throughput::Elements(1));
            let id = BenchmarkId::new("u_x_n", format!("{}u_{}e", u, n));
            group.bench_with_input(id, &(u, n), |b, &(u, n)| {
                let mut world = BenchWorldBuilder::new().users(u).entities(n).build();
                b.iter(|| world.tick());
            });
        }
    }
    group.finish();
}

/// Idle-room tick, U×N matrix, IMMUTABLE entities.
///
/// The capacity surface Cyberlith actually runs on — tiles are immutable
/// Naia entities (GDD §10). Compare against `tick/idle_matrix` to see how
/// much of Win-5's per-tick discount survives at scale.
pub fn idle_room_tick_matrix_immutable(c: &mut Criterion) {
    let mut group = c.benchmark_group("tick/idle_matrix_immutable");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(5));
    group.sample_size(20);

    for &u in MATRIX_USERS {
        for &n in MATRIX_ENTITIES {
            group.throughput(Throughput::Elements(1));
            let id = BenchmarkId::new("u_x_n", format!("{}u_{}e", u, n));
            group.bench_with_input(id, &(u, n), |b, &(u, n)| {
                let mut world = BenchWorldBuilder::new()
                    .users(u)
                    .entities(n)
                    .immutable()
                    .build();
                b.iter(|| world.tick());
            });
        }
    }
    group.finish();
}

criterion_group!(
    name = tick_idle;
    config = Criterion::default();
    targets = idle_room_tick, idle_room_tick_matrix, idle_room_tick_matrix_immutable
);
