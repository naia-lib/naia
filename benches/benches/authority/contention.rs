use std::time::Duration;

use criterion::{criterion_group, BatchSize, BenchmarkId, Criterion};

use naia_benches::BenchWorldBuilder;

const USER_COUNTS: &[usize] = &[1, 2, 4, 8];

/// Multi-user authority contention: K clients simultaneously request authority
/// on the same entity, then a tick resolves the contention.
///
/// Setup: K users, 1000 entities, fully replicated.
/// Measured: all K clients call request_authority(entity[0]) + tick.
///           On this tick the server receives K requests, picks one winner,
///           sends AuthGranted to the winner and AuthDenied to the rest.
///
/// Parametric over user count K. Measures the per-tick cost of resolving
/// a K-way authority conflict.
pub fn multi_user_contention(c: &mut Criterion) {
    let mut group = c.benchmark_group("authority/contention");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(5));

    for &k in USER_COUNTS {
        group.bench_with_input(
            BenchmarkId::new("users", k),
            &k,
            |b, &k| {
                b.iter_batched(
                    || BenchWorldBuilder::new().users(k).entities(1_000).build(),
                    |mut world| {
                        world.request_authority_all_clients(0);
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
    name = authority_contention;
    config = Criterion::default();
    targets = multi_user_contention
);
