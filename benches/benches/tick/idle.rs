use std::time::Duration;

use criterion::{criterion_group, BatchSize, BenchmarkId, Criterion, Throughput};

use naia_benches::BenchWorldBuilder;

const ENTITY_COUNTS: &[usize] = &[100, 500, 1_000, 5_000, 10_000];

/// Idle-room tick: N entities in scope, 0 mutations.
///
/// Expected shape: **flat line** — time must not grow with entity count.
/// A rising curve means Win-2 or Win-3 regression.
pub fn idle_room_tick(c: &mut Criterion) {
    let mut group = c.benchmark_group("tick/idle");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(5));

    for &n in ENTITY_COUNTS {
        group.throughput(Throughput::Elements(1)); // 1 tick per iteration
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

criterion_group!(
    name = tick_idle;
    config = Criterion::default();
    targets = idle_room_tick
);
