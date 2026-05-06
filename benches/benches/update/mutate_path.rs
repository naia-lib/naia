use std::sync::Arc;
use std::time::{Duration, Instant};

use criterion::{criterion_group, BatchSize, Criterion};

use naia_benches::{bench, BenchWorldBuilder};
use naia_shared::{DirtyNotifier, DirtySet, EntityIndex, MutReceiver};

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

    // ── single_user / notify_clean_to_dirty (B-strict) ──
    //
    // Direct microbench of the per-mutation hot path with the harness
    // floor removed. Builds a `MutReceiver` + `DirtySet` + `DirtyNotifier`
    // by hand, caches them outside the timed loop, and times one
    // clean→dirty mutation per iteration. The clean state between iters
    // is restored by `mask.clear()` (atomic swap-zero) + a `drain` of
    // the dirty set; both are *also* timed and serve as the post-mutate
    // reset constant — they cancel out of the pre/post-B delta.
    //
    // The existing `single_user/single_property` cell pays a ~350 ns
    // harness floor for `world.entity_mut(...).component::<...>()` per
    // iteration; this cell's measured cost is dominated by `mutate` +
    // `clear_mask` + `drain`, which is what production code actually
    // exercises on every replicated mutation. Lock-free `notify_dirty`
    // (B-strict step 2) shows here as a direct ns-level win.
    group.bench_function(bench!("single_user/notify_clean_to_dirty"), |b| {
        let dirty_set: Arc<DirtySet> = Arc::new(DirtySet::new(1));
        let receiver = MutReceiver::new(1);
        receiver.attach_notifier(DirtyNotifier::new(
            EntityIndex(0),
            0,
            Arc::downgrade(&dirty_set),
        ));
        b.iter_custom(|iters| {
            let start = Instant::now();
            for _ in 0..iters {
                receiver.mutate(0); // clean → dirty: atomic set + notify
                receiver.clear_mask(); // dirty → clean: atomic clear + cancel
                drain_dirty_set(&dirty_set); // dedupe stale indices
            }
            start.elapsed()
        })
    });

    // ── single_user / notify_dirty_repeat (B-strict, atomic floor) ──
    //
    // Pre-warms the receiver into the dirty state, then times back-to-back
    // mutations. Subsequent `mutate(0)` calls hit `set_bit` on an
    // already-dirty mask: `was_clear == false`, so `notify_dirty` is
    // skipped — we measure only the `AtomicU64::fetch_or` cost. This is
    // the lower bound for `MutReceiver::mutate` regardless of dirty-set
    // implementation. Used to confirm that the lock-free win in
    // `notify_clean_to_dirty` is not a measurement artifact.
    group.bench_function(bench!("single_user/notify_dirty_repeat"), |b| {
        let dirty_set: Arc<DirtySet> = Arc::new(DirtySet::new(1));
        let receiver = MutReceiver::new(1);
        receiver.attach_notifier(DirtyNotifier::new(
            EntityIndex(0),
            0,
            Arc::downgrade(&dirty_set),
        ));
        receiver.mutate(0); // pre-warm into dirty
        b.iter(|| receiver.mutate(0))
    });

    // ── 16_users_in_scope / notify_clean_to_dirty (B-strict) ──
    //
    // 16 receivers share one DirtySet (the per-user wiring under fan-out).
    // Each iter performs 16 clean→dirty mutations + 16 clear_masks + 1
    // drain. The fan-out cost is what `MutChannel::send` pays in
    // production; lock-free `notify_dirty` removes the per-receiver
    // mutex acquire that today shows up 16× per fan-out.
    group.bench_function(bench!("16_users_in_scope/notify_clean_to_dirty"), |b| {
        let dirty_set: Arc<DirtySet> = Arc::new(DirtySet::new(1));
        let receivers: Vec<MutReceiver> = (0..16u8)
            .map(|i| {
                let r = MutReceiver::new(1);
                r.attach_notifier(DirtyNotifier::new(
                    EntityIndex(i as u32),
                    0,
                    Arc::downgrade(&dirty_set),
                ));
                r
            })
            .collect();
        b.iter_custom(|iters| {
            let start = Instant::now();
            for _ in 0..iters {
                for r in &receivers {
                    r.mutate(0);
                }
                for r in &receivers {
                    r.clear_mask();
                }
                drain_dirty_set(&dirty_set);
            }
            start.elapsed()
        })
    });

    group.finish();
}

#[inline]
fn drain_dirty_set(dirty_set: &Arc<DirtySet>) {
    let _ = dirty_set.drain();
}

criterion_group!(
    name = mutate_path_group;
    config = Criterion::default();
    targets = mutate_path
);
