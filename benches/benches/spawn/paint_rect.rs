use std::time::Duration;

use criterion::{criterion_group, BatchSize, BenchmarkId, Criterion, Throughput};

use naia_benches::BenchWorldBuilder;

/// Phase 6 — PaintRect burst cost: measures the wall time for spawning N
/// entities (with K components each) on the server and driving ticks until
/// every entity has replicated to a connected client.
///
/// Distinct from the existing benches:
/// - `spawn/burst` and `spawn/coalesced` both measure *one idle tick after*
///   a steady-state world is built. They tell us the post-replication cost.
/// - This bench measures the burst-to-replicated round trip — the cost
///   `PaintRect` of N tiles actually exposes to the level editor.
///
/// Throughput is reported as entities/sec so the slope (entities-per-second)
/// is the headline number. The expectation, per the Phase 6 audit
/// (`phase6_paint_rect_audit.rs`), is one `SpawnWithComponents` command per
/// entity — i.e., the wire-message count grows linearly in N, not N×K.
const ENTITY_COUNTS: &[usize] = &[100, 1_000, 5_000];
const COMPONENTS_PER_ENTITY: usize = 2;

pub fn paint_rect_burst(c: &mut Criterion) {
    let mut group = c.benchmark_group("spawn/paint_rect");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(8));

    for &n in ENTITY_COUNTS {
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::new("entities", n), &n, |b, &n| {
            b.iter_batched(
                // Setup: connected world with the user already in the room
                // and zero entities. Setup is NOT measured.
                || BenchWorldBuilder::new().users(1).entities(0).build(),
                // Measurement: spawn the rect, then tick until every entity
                // is visible on the client.
                |mut world| {
                    world.paint_rect_spawn_burst(n, COMPONENTS_PER_ENTITY);
                    for _ in 0..2_000 {
                        world.tick();
                        if world.client_entity_count() >= n {
                            break;
                        }
                    }
                },
                BatchSize::LargeInput,
            )
        });
    }
    group.finish();
}

criterion_group!(
    name = spawn_paint_rect;
    config = Criterion::default();
    targets = paint_rect_burst
);
