use std::time::Duration;

use criterion::{criterion_group, BatchSize, Criterion};

use naia_benches::{bench, BenchWorldBuilder};

const ENTITY_COUNT: usize = 10_000;

/// Immutable vs. mutable component idle-tick overhead.
///
/// Win-5 claim: immutable components allocate no diff-tracking state
/// (no MutChannel, UserDiffHandler, or MutReceiver), so their idle-tick
/// cost should be equal to or less than mutable components with 0 mutations.
///
/// A measurable delta between mutable_idle and immutable_idle would indicate
/// a Win-5 regression (diff-tracking allocation for immutable components).
pub fn mutable_idle(c: &mut Criterion) {
    let mut group = c.benchmark_group("update/immutable");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(5));

    group.bench_function(bench!("mutable_idle"), |b| {
        b.iter_batched(
            || {
                BenchWorldBuilder::new()
                    .users(1)
                    .entities(ENTITY_COUNT)
                    .build()
            },
            |mut world| world.tick(),
            BatchSize::LargeInput,
        )
    });
    group.finish();
}

pub fn immutable_idle(c: &mut Criterion) {
    let mut group = c.benchmark_group("update/immutable");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(5));

    group.bench_function(bench!("immutable_idle"), |b| {
        b.iter_batched(
            || {
                BenchWorldBuilder::new()
                    .users(1)
                    .entities(ENTITY_COUNT)
                    .immutable()
                    .build()
            },
            |mut world| world.tick(),
            BatchSize::LargeInput,
        )
    });
    group.finish();
}

criterion_group!(
    name = update_immutable;
    config = Criterion::default();
    targets = mutable_idle, immutable_idle
);
