use std::time::Duration;

use criterion::{criterion_group, BatchSize, BenchmarkId, Criterion, Throughput};

use naia_benches::BenchWorldBuilder;

const ENTITY_COUNTS: &[usize] = &[100, 500, 1_000, 5_000, 10_000];

/// Scope-enter: time for the first tick after a new user has been added to a
/// room with N entities already present.
///
/// This captures the per-entity burst cost Naia pays when a user first enters
/// scope — it must queue SpawnWithComponents messages for all N entities.
/// Expected shape: **linear in N** (unavoidable — O(N) entity messages must be sent).
pub fn scope_enter(c: &mut Criterion) {
    let mut group = c.benchmark_group("tick/scope_enter");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(6));

    for &n in ENTITY_COUNTS {
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::new("entities", n), &n, |b, &n| {
            b.iter_batched(
                || BenchWorldBuilder::new().users(1).entities(n).build(),
                |mut world| world.tick(),
                BatchSize::LargeInput,
            )
        });
    }
    group.finish();
}

/// Scope-exit: tick cost when a user is removed from a room with N entities.
///
/// Naia must clean up per-user diff state and send despawn messages for all
/// N entities. Expected shape: linear in N.
pub fn scope_exit(c: &mut Criterion) {
    let mut group = c.benchmark_group("tick/scope_exit");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(6));

    for &n in ENTITY_COUNTS {
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::new("entities", n), &n, |b, &n| {
            b.iter_batched(
                || BenchWorldBuilder::new().users(1).entities(n).build(),
                |mut world| {
                    // Remove user from room; tick processes teardown.
                    world.remove_user_from_room(0);
                    world.tick();
                },
                BatchSize::LargeInput,
            )
        });
    }
    group.finish();
}

criterion_group!(
    name = tick_scope;
    config = Criterion::default();
    targets = scope_enter, scope_exit
);
