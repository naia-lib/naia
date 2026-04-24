use std::time::Duration;

use criterion::{criterion_group, BatchSize, BenchmarkId, Criterion, Throughput};

use naia_benches::BenchWorldBuilder;

const MUTATION_COUNTS: &[usize] = &[1, 10, 100, 1_000, 5_000];

/// K mutations per tick, parametric over K.
///
/// Uses `Throughput::Elements(k)` so Criterion reports mutations/sec.
/// Complements tick/active by isolating the update-pipeline cost from scope overhead.
pub fn bulk_mutations(c: &mut Criterion) {
    let mut group = c.benchmark_group("update/bulk");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(5));

    for &k in MUTATION_COUNTS {
        group.throughput(Throughput::Elements(k as u64));
        group.bench_with_input(
            BenchmarkId::new("mutations", k),
            &k,
            |b, &k| {
                b.iter_batched(
                    || {
                        BenchWorldBuilder::new()
                            .users(1)
                            .entities(k) // enough entities for k mutations
                            .build()
                    },
                    |mut world| {
                        world.mutate_entities(k);
                        world.tick();
                    },
                    BatchSize::LargeInput,
                )
            },
        );
    }
    group.finish();
}

criterion_group!(
    name = update_bulk;
    config = Criterion::default();
    targets = bulk_mutations
);
