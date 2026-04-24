use std::time::Duration;

use criterion::{criterion_group, BatchSize, BenchmarkId, Criterion, Throughput};

use naia_benches::BenchWorldBuilder;

const ENTITY_COUNTS: &[usize] = &[1, 10, 100, 1_000];

/// Wire-framing efficiency: bytes/sec at various mutation counts.
///
/// Setup: N entities, all mutated each tick.
/// Calibration: a probe world runs 60 warmup ticks and reads
///   `server.outgoing_bandwidth_total()` (kbps) to set `Throughput::Bytes`.
///   Criterion then reports bytes/sec so results are directly comparable
///   to network budget estimates.
pub fn wire_framing(c: &mut Criterion) {
    let mut group = c.benchmark_group("wire/framing");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(5));

    for &n in ENTITY_COUNTS {
        // Calibrate bytes/tick using the server's built-in bandwidth monitor.
        let bytes_per_tick = {
            let mut probe = BenchWorldBuilder::new()
                .with_bandwidth()
                .users(1)
                .entities(n)
                .build();
            for _ in 0..60 {
                probe.mutate_entities(n);
                probe.tick();
            }
            probe.server_outgoing_bytes_per_tick()
        };

        group.throughput(Throughput::Bytes(bytes_per_tick));
        group.bench_with_input(
            BenchmarkId::new("entities", n),
            &n,
            |b, &n| {
                b.iter_batched(
                    || BenchWorldBuilder::new().users(1).entities(n).build(),
                    |mut world| {
                        world.mutate_entities(n);
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
    name = wire_framing_group;
    config = Criterion::default();
    targets = wire_framing
);
