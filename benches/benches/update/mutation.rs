use std::time::Duration;

use criterion::{criterion_group, BatchSize, Criterion};

use naia_benches::{bench, BenchWorldBuilder};

/// Single mutation end-to-end dispatch latency.
///
/// One entity, one mutation per tick. Baseline latency number users
/// quote in bandwidth planning.
pub fn single_mutation(c: &mut Criterion) {
    let mut group = c.benchmark_group("update/mutation");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(5));

    group.bench_function(bench!("single_mutation"), |b| {
        b.iter_batched(
            || BenchWorldBuilder::new().users(1).entities(1).build(),
            |mut world| {
                world.mutate_entities(1);
                world.tick();
            },
            BatchSize::SmallInput,
        )
    });
    group.finish();
}

criterion_group!(
    name = update_mutation;
    config = Criterion::default();
    targets = single_mutation
);
