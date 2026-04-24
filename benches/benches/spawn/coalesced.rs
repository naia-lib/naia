use std::time::Duration;

use criterion::{criterion_group, BatchSize, BenchmarkId, Criterion};

use naia_benches::BenchWorldBuilder;

const ENTITY_COUNTS: &[usize] = &[1, 10, 100, 1_000];

/// Spawn burst steady-state cost: idle tick after N entities have been
/// replicated to a client, parametric over entity count.
///
/// This shows that steady-state tick cost after a burst is O(1) in entity
/// count — once entities are replicated, their presence on the client has
/// no ongoing per-tick cost.
///
/// Note: a direct legacy-vs-SpawnWithComponents (Win-4) comparison would
/// require two Naia builds (one without the Win-4 optimisation). Since
/// SpawnWithComponents is always the wire format in this build, that
/// comparison is deferred. The per-entity throughput here gives a lower
/// bound on the coalesced spawn efficiency.
pub fn coalesced_spawn(c: &mut Criterion) {
    let mut group = c.benchmark_group("spawn/coalesced");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(5));

    for &n in ENTITY_COUNTS {
        group.bench_with_input(
            BenchmarkId::new("entities", n),
            &n,
            |b, &n| {
                b.iter_batched(
                    || BenchWorldBuilder::new().users(1).entities(n).build(),
                    |mut world| world.tick(),
                    BatchSize::LargeInput,
                )
            },
        );
    }
    group.finish();
}

criterion_group!(
    name = spawn_coalesced;
    config = Criterion::default();
    targets = coalesced_spawn
);
