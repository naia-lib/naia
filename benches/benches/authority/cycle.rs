use std::time::Duration;

use criterion::{criterion_group, BatchSize, Criterion};

use naia_benches::{bench, BenchWorldBuilder};

/// Authority grant/revoke full-cycle cost.
///
/// Setup: 1 user, 10 entities, all replicated to steady state.
/// Measured: server grants authority on entity[0] to user[0], drives ticks
///           until client confirms `Granted`, then server takes authority back
///           and drives ticks until the `Granted` status clears.
///
/// This is the complete authority-round-trip cost — both directions of the
/// authority lifecycle in a single iteration. Complements `authority/grant`
/// (server-side grant + one tick, no client confirmation) and
/// `authority/contention` (multi-client simultaneous requests).
///
/// With local transport the client confirms in one tick in each direction,
/// so the measured wall time is two advance_tick calls plus two event-drain
/// passes. Numbers from this bench are what game-server code pays to hand off
/// authority and then reclaim it.
pub fn authority_grant_revoke_cycle(c: &mut Criterion) {
    let mut group = c.benchmark_group("authority/cycle");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(5));

    group.bench_function(bench!("grant_revoke_cycle"), |b| {
        b.iter_batched(
            || BenchWorldBuilder::new().users(1).entities(10).delegated().build(),
            |mut world| {
                world.give_authority_on_entity(0);
                let ok1 = world.tick_until_client_auth_granted(100);
                assert!(ok1, "authority grant timed out");
                world.take_authority_on_entity(0);
                let ok2 = world.tick_until_client_auth_not_granted(100);
                assert!(ok2, "authority revoke timed out");
            },
            BatchSize::LargeInput,
        )
    });
    group.finish();
}

criterion_group!(
    name = authority_cycle;
    config = Criterion::default();
    targets = authority_grant_revoke_cycle
);
