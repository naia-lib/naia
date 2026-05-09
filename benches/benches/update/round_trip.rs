use std::time::Duration;

use criterion::{criterion_group, BatchSize, Criterion};

use naia_benches::{bench, BenchWorldBuilder};

/// Single-client mutation round-trip latency.
///
/// Setup: 1 user, 1 entity, mutation applied before measurement starts.
/// Measured: tick sequence until client 0 has received and applied the
///           updated component value (confirmed end-to-end).
///
/// With local (in-memory) transport the update propagates in exactly one
/// tick — the measurement is the wall time of that tick including the
/// client's receive path. This is the true end-to-end round-trip cost as
/// seen by the game loop, not just server-side dispatch.
///
/// Complements `update/mutation` (server-side dispatch only). Latency
/// numbers from this bench are the ones to quote for "how long until my
/// client sees my change?"
pub fn single_mutation_round_trip(c: &mut Criterion) {
    let mut group = c.benchmark_group("update/round_trip");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(5));

    group.bench_function(bench!("single_mutation_round_trip"), |b| {
        b.iter_batched(
            || {
                let mut world = BenchWorldBuilder::new().users(1).entities(1).build();
                // Mutation applied in setup (not measured): the measured closure
                // drives ticks until the client confirms receipt.
                world.mutate_entities(1);
                world
            },
            |mut world| {
                let ok = world.tick_until_client_entity_updated(1, 100);
                assert!(ok, "round-trip timed out: client did not confirm update");
            },
            BatchSize::SmallInput,
        )
    });
    group.finish();
}

criterion_group!(
    name = update_round_trip;
    config = Criterion::default();
    targets = single_mutation_round_trip
);
