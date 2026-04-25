use std::time::Duration;

use criterion::{criterion_group, BatchSize, BenchmarkId, Criterion};

use naia_benches::BenchWorldBuilder;

const CELLS: &[(usize, usize, &str)] = &[
    (1, 100, "1u_100e"),
    (1, 1_000, "1u_1000e"),
    (1, 10_000, "1u_10000e"),
    (4, 100, "4u_100e"),
    (4, 1_000, "4u_1000e"),
    (4, 10_000, "4u_10000e"),
    (16, 100, "16u_100e"),
    (16, 1_000, "16u_1000e"),
    (16, 10_000, "16u_10000e"),
];

/// Steady-state tick cost when N entities are in scope for U users in a single
/// room and game code invokes `server.scope_checks()` once per tick (the
/// canonical pattern from `demos/basic`, `demos/macroquad`). The rebuild is
/// `O(rooms × users × entities)` HashMap lookups today
/// (`world_server.rs:628-647`, with a literal `// TODO: precache this` comment).
///
/// At Cyberlith canonical (1 room × 16 users × 65,536 tiles) the rebuild runs
/// >1M HashMap lookups per tick. Phase 8.2 replaces this with a push-based
/// cache invalidated only on room/user/entity churn — `scope_checks()` then
/// returns a borrowed slice with zero per-tick allocation.
///
/// Bench shape: every iteration is one `tick()` followed by `scope_checks()`,
/// so the dominant signal is the rebuild plus the surrounding tick scaffolding.
pub fn scope_with_rooms(c: &mut Criterion) {
    let mut group = c.benchmark_group("tick/scope_with_rooms/u_x_n");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(6));

    for &(users, entities, label) in CELLS {
        group.bench_with_input(
            BenchmarkId::from_parameter(label),
            &(users, entities),
            |b, &(u, n)| {
                b.iter_batched(
                    || BenchWorldBuilder::new().users(u).entities(n).build(),
                    |mut world| {
                        world.tick();
                        let _ = world.scope_checks_tuple_count();
                    },
                    BatchSize::LargeInput,
                )
            },
        );
    }
    group.finish();
}

criterion_group!(
    name = tick_scope_with_rooms;
    config = Criterion::default();
    targets = scope_with_rooms
);
