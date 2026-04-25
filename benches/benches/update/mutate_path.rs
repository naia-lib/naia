use std::time::Duration;

use criterion::{criterion_group, BatchSize, Criterion};

use naia_benches::{bench, BenchWorldBuilder};

/// Phase 8.1 hot-path microbenches.
///
/// These benches isolate the *per-mutation* cost — the chain that runs when
/// production code does `*comp.value = x;`. The chain today (audit
/// 2026-04-25) is:
///
/// ```text
/// Property::DerefMut → MutSender::send → MutChannel::send (RwLock read)
///   → for each in-scope user receiver:
///       MutReceiver::mutate
///         RwLock<DiffMask>::write          ← lock #1
///         Vec scan
///         DirtyNotifier::notify_dirty
///           RwLock<DirtySet>::write        ← lock #2
///           HashMap::entry().or_default().insert()
/// ```
///
/// And the drain path (one tick, one user, after N entities mutated):
///
/// ```text
/// UserDiffHandler::dirty_receiver_candidates
///   RwLock<DirtySet>::read
///   HashMap::clone()                       ← O(N) alloc + copy per tick per user
/// ```
///
/// The three cells below pin those costs and let Phase 8.1's columnar
/// rewrite (see `_AGENTS/BENCH_PERF_UPGRADE_PHASE_8_PLAN.md`) prove its
/// numbers against a recorded `perf_v8_pre` baseline.
///
/// **Measurement note** — these run in `iter_batched` over a freshly built
/// world per iteration to keep the property's first-dirty state clean. The
/// cost includes one `entity_mut(...).component::<BenchComponent>()`
/// lookup per iter (HashMap probe + downcast); that lookup is constant
/// before/after Phase 8.1, so it cancels out of the delta. Targeted
/// numbers in the plan (≤ 25 ns / 250 ns) are post-fix targets and must
/// account for the lookup floor.
pub fn mutate_path(c: &mut Criterion) {
    let mut group = c.benchmark_group("update/mutate_path");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(5));

    // ── single_user / single_property ──
    //
    // 1 user in scope, 1 entity, 1 component. One mutation per iter, no tick.
    // Measures the per-mutation hot path with minimum fan-out — the best-case
    // cost-per-`*comp.value = x` that today's RwLock-chain pays even at U=1.
    group.bench_function(bench!("single_user/single_property"), |b| {
        b.iter_batched_ref(
            || BenchWorldBuilder::new().users(1).entities(1).build(),
            |world| {
                world.mutate_entities(1);
            },
            BatchSize::SmallInput,
        )
    });

    // ── 16_users_in_scope / single_property ──
    //
    // 16 users in scope of one entity. One mutation per iter, no tick.
    // Measures the per-user fan-out cost — today's `MutChannel::send` walks
    // every receiver on every mutation, taking 16 RwLock writes for one
    // logical state change. Phase 8.1 Stage D collapses this to a flat
    // fan-out without locks.
    group.bench_function(bench!("16_users_in_scope/single_property"), |b| {
        b.iter_batched_ref(
            || BenchWorldBuilder::new().users(16).entities(1).build(),
            |world| {
                world.mutate_entities(1);
            },
            BatchSize::SmallInput,
        )
    });

    // ── drain_dirty / 16u_1000_dirty_entities ──
    //
    // 16 users × 1000 entities, all in scope, all mutated. One tick per iter.
    // Measures the drain side of the dirty pipeline — `take_outgoing_events`
    // for each user clones the entire DirtySet HashMap. At 16u × 1000e that
    // is 16 × 1000-entry clones per tick today; Phase 8.1 Stage B replaces
    // the HashMap-clone with a `Vec<u32>::drain` over packed indices.
    //
    // Note: this includes more than just the drain — it includes the 16,000
    // mutations and the full server send_all_packets. We can't isolate the
    // drain externally because Naia doesn't expose `take_outgoing_events`
    // at the public surface. The drain is the dominant cost at this size,
    // so the bench is informative for the Stage B win even with the noise.
    group.bench_function(bench!("drain_dirty/16u_1000_dirty_entities"), |b| {
        b.iter_batched(
            || {
                let mut world = BenchWorldBuilder::new().users(16).entities(1000).build();
                world.mutate_entities(1000);
                world
            },
            |mut world| {
                world.tick();
            },
            BatchSize::LargeInput,
        )
    });

    group.finish();
}

criterion_group!(
    name = mutate_path_group;
    config = Criterion::default();
    targets = mutate_path
);
