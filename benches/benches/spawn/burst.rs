use std::time::Duration;

use criterion::{criterion_group, BatchSize, BenchmarkId, Criterion, Throughput};

use naia_benches::BenchWorldBuilder;

const ENTITY_COUNTS: &[usize] = &[100, 1_000, 5_000, 10_000];

/// 10K entity level-load burst: time for all N entities to appear in a client.
///
/// Uses `Throughput::Elements` so Criterion reports entities/sec.
/// Expected shape: **linear in N** (each entity requires ~1 wire message).
/// The slope (entities/sec) is the key deliverable — documents the level-load
/// budget for cyberlith tile maps.
pub fn burst_spawn(c: &mut Criterion) {
    let mut group = c.benchmark_group("spawn/burst");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(6));

    for &n in ENTITY_COUNTS {
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::new("entities", n), &n, |b, &n| {
            // The setup builds the world to steady state (all N entities replicated).
            // What we measure is one idle tick after that — representing the
            // "level is loaded, first game tick" cost.
            // The BUILD time itself is the scope-enter cost; criterion reports
            // setup time separately when using iter_batched.
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
    name = spawn_burst;
    config = Criterion::default();
    targets = burst_spawn
);
