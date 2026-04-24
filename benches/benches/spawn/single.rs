use std::time::Duration;

use criterion::{criterion_group, BatchSize, Criterion};

use naia_benches::{bench, BenchWorldBuilder};

/// Single entity spawn: server-side spawn + first-tick send cost.
///
/// Setup: empty world (0 entities, 1 user, fully connected).
/// Measured: spawn_one_entity() (server creates entity + adds to room)
///           + tick() (processes outbound spawn message to client).
///
/// This establishes the per-entity spawn baseline cost. The client will
/// receive the entity on a subsequent tick (network round-trip); this
/// measures the server-side cost of the first tick after spawn.
pub fn spawn_single(c: &mut Criterion) {
    let mut group = c.benchmark_group("spawn/single");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(5));

    group.bench_function(bench!("spawn_single"), |b| {
        b.iter_batched(
            || BenchWorldBuilder::new().users(1).entities(0).build(),
            |mut world| {
                world.spawn_one_entity();
                world.tick();
            },
            BatchSize::SmallInput,
        )
    });
    group.finish();
}

criterion_group!(
    name = spawn_single_group;
    config = Criterion::default();
    targets = spawn_single
);
