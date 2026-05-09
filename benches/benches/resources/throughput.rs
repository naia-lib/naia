use std::time::Duration;

use criterion::{criterion_group, BatchSize, Criterion};

use naia_benches::{bench, BenchWorldBuilder};

/// Resource initial replication latency.
///
/// Setup: 1 user, 0 entities.
/// Measured: server inserts `BenchResource` then drives ticks until client 0
///           has received it.
///
/// With local transport this is one tick. The measurement captures the cost
/// of the resource insert path + the first delta send + client receive.
pub fn resource_insert_latency(c: &mut Criterion) {
    let mut group = c.benchmark_group("resources/throughput");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(5));

    group.bench_function(bench!("insert_latency"), |b| {
        b.iter_batched(
            || BenchWorldBuilder::new().users(1).entities(0).build(),
            |mut world| {
                world.insert_resource();
                let ok = world.tick_until_client_has_resource(100);
                assert!(ok, "resource replication timed out");
            },
            BatchSize::SmallInput,
        )
    });

    // Resource mutation throughput: cost of one mutated-resource tick.
    // Setup: 1 user, 0 entities, resource already inserted and replicated to
    // client (so the measured tick is purely the delta propagation).
    // Measured: mutate resource value + tick.
    group.bench_function(bench!("mutation_throughput"), |b| {
        b.iter_batched(
            || {
                let mut world = BenchWorldBuilder::new().users(1).entities(0).build();
                world.insert_resource();
                let ok = world.tick_until_client_has_resource(100);
                assert!(ok, "resource setup timed out");
                world
            },
            |mut world| {
                world.mutate_resource();
                world.tick();
            },
            BatchSize::SmallInput,
        )
    });

    group.finish();
}

criterion_group!(
    name = resources_throughput;
    config = Criterion::default();
    targets = resource_insert_latency
);
