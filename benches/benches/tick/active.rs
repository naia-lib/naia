use std::time::Duration;

use criterion::{criterion_group, BatchSize, BenchmarkId, Criterion};

use naia_benches::BenchWorldBuilder;

/// Fixed entity count for active-tick benchmarks.
const FIXED_ENTITY_COUNT: usize = 10_000;
/// Number of mutations per tick, parametric.
const MUTATION_COUNTS: &[usize] = &[0, 1, 10, 100, 1_000];

/// Active-room tick: fixed N entities, K mutations per tick.
///
/// Expected shape: **linear in K**, independent of N.
/// Proves Win-3: update work scales with dirty count, not entity count.
pub fn active_room_tick(c: &mut Criterion) {
    let mut group = c.benchmark_group("tick/active");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(5));

    for &k in MUTATION_COUNTS {
        group.bench_with_input(
            BenchmarkId::new("mutations", k),
            &k,
            |b, &k| {
                b.iter_batched(
                    || {
                        BenchWorldBuilder::new()
                            .users(1)
                            .entities(FIXED_ENTITY_COUNT)
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
    name = tick_active;
    config = Criterion::default();
    targets = active_room_tick
);
