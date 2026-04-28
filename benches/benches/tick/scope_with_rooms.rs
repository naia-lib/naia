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

/// Clone cost of `scope_checks_all()` — the full-list path used by game code
/// that implements dynamic scope (e.g. distance or visibility checks that may
/// exclude entities each tick). Bench shape: one `tick()` + one
/// `scope_checks_all()` clone per iteration; dominant signal is the Vec clone
/// of all (room, user, entity) tuples.
///
/// The push-based `ScopeChecksCache` (Phase 8.2) eliminated the prior O(rooms
/// × users × entities) HashMap rebuild — `scope_checks_all()` now pays only
/// the O(N tuples) clone cost. For game servers that use add-all-on-first-sight
/// scope, `scope_checks_pending()` is free after initial load; see
/// `scope_checks_pending_tuple_count()` in `lib.rs`.
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
                        let _ = world.scope_checks_all_tuple_count();
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
