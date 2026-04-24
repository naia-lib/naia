use std::time::Duration;

use criterion::{criterion_group, BatchSize, BenchmarkId, Criterion, Throughput};

use naia_benches::BenchWorldBuilder;

/// (user_count, mutation_count) pairs for bandwidth stress tests.
const SCENARIOS: &[(usize, usize)] = &[
    (1, 100),
    (4, 100),
    (1, 1_000),
    (4, 1_000),
];

/// Sustained mutation throughput: N users, K mutations/tick.
///
/// Throughput::Bytes is calibrated from Naia's per-tick outgoing byte
/// counter: we run 60 warmup ticks, then read
/// `server.outgoing_bytes_last_tick()` — a precise per-tick total that
/// `send_all_packets` resets at the start of each tick. Criterion then
/// reports iterations/sec and bytes/sec correctly.
pub fn sustained_bandwidth(c: &mut Criterion) {
    let mut group = c.benchmark_group("wire/bandwidth");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(5));

    for &(users, mutations) in SCENARIOS {
        // Probe to calibrate actual bytes/tick using Naia's per-tick counter.
        let bytes_per_tick = {
            let mut probe = BenchWorldBuilder::new()
                .users(users)
                .entities(mutations)
                .build();
            for _ in 0..60 {
                probe.mutate_entities(mutations);
                probe.tick();
            }
            probe.server_outgoing_bytes_per_tick()
        };

        let label = format!("{}u_{}m", users, mutations);
        group.throughput(Throughput::Bytes(bytes_per_tick));
        group.bench_with_input(
            BenchmarkId::new("scenario", &label),
            &(users, mutations),
            |b, &(users, mutations)| {
                b.iter_batched(
                    || {
                        BenchWorldBuilder::new()
                            .users(users)
                            .entities(mutations)
                            .build()
                    },
                    |mut world| {
                        world.mutate_entities(mutations);
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
    name = wire_bandwidth;
    config = Criterion::default();
    targets = sustained_bandwidth
);
