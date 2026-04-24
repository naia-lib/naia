use std::time::Duration;

use criterion::{criterion_group, BatchSize, Criterion};

use naia_benches::{bench, BenchWorldBuilder};

/// Authority grant: tick cost when the server grants authority on an entity.
///
/// Setup: 1 user, 10 entities, all at steady state.
/// Measured: server calls give_authority(entity[0], user[0]) + tick.
///           This is the server-side cost of the grant operation and the
///           tick that propagates the authority transfer to the client.
///
/// The client receives `ClientEntityAuthGrantedEvent` on a subsequent tick;
/// this benchmark captures the server-side half of the round-trip.
pub fn authority_grant(c: &mut Criterion) {
    let mut group = c.benchmark_group("authority/grant");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(5));

    group.bench_function(bench!("authority_grant"), |b| {
        b.iter_batched(
            || BenchWorldBuilder::new().users(1).entities(10).build(),
            |mut world| {
                world.give_authority_on_entity(0);
                world.tick();
            },
            BatchSize::SmallInput,
        )
    });
    group.finish();
}

criterion_group!(
    name = authority_grant_group;
    config = Criterion::default();
    targets = authority_grant
);
