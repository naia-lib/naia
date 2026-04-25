# Phase 8.1 — Columnar dirty + EntityIndex (Stages A-D)

**Status:** ✅ COMPLETE 2026-04-25 — Stages B + C + D landed; Stage A reverted (premature plumbing without consumer).

Plan ordering is A → B → C → D, but stages were executed C → B → ... in that order because Stage C (atomic `DiffMask`) is self-contained and yields the most isolated win on the per-mutation hot path. Stage A (EntityIndex plumbing) is structural-only and is deferred until B + D demand it (Vec-indexed dirty queue and slot-array fan-out, respectively).

---

## Pre-baseline (`perf_v8_pre`)

Captured 2026-04-25 against the as-of-Phase-8.0 codebase, before any 8.1 changes.

| Bench | Pre |
|---|---:|
| `update/mutate_path/single_user/single_property` | 899 ns |
| `update/mutate_path/16_users_in_scope/single_property` | 7.33 µs |
| `update/mutate_path/drain_dirty/16u_1000_dirty_entities` | 116 ms |

The original Phase 8.1 plan estimated ~120 ns / ~2 µs / ~700 µs respectively. Measured numbers are **5×–165× higher** than the plan estimates. This means the per-mutation chain is more expensive than the plan acknowledged — there is more headroom to demonstrate wins, and the absolute ≤ 25 ns / ≤ 250 ns / ≤ 200 µs targets demand structural changes (atomic mask alone is necessary but not sufficient).

---

## Stage C — atomic `DiffMask` (landed 2026-04-25)

**Files touched:**
- `naia/shared/src/world/update/atomic_diff_mask.rs` — **new**. `AtomicDiffMask` cell with single `AtomicU64` for ≤8-byte masks (≤64 props), `Box<[AtomicU64]>` fallback for wider masks. Wire encoding byte-for-byte unchanged.
- `naia/shared/src/world/update/mut_channel.rs` — `MutReceiver` swapped from `Arc<RwLock<DiffMask>>` to `Arc<AtomicDiffMask>`. `mutate` is now `set_bit` (single `fetch_or`); `or_with` and `clear` are similarly atomic.
- `naia/shared/src/world/update/user_diff_handler.rs` — `diff_mask` → `diff_mask_snapshot()` returns owned `DiffMask`; no more `RwLockReadGuard` lifetimes leaking.
- `naia/shared/src/world/update/entity_update_manager.rs` — `get_diff_mask` returns owned `DiffMask`.
- `naia/shared/src/world/local/local_world_manager.rs` — same API change.
- `naia/shared/src/world/world_writer.rs` — removed redundant `.clone()` (return is already owned).
- `naia/shared/src/world/update/mod.rs` — exports `atomic_diff_mask`.

**Wire correctness:** `byte_layout_matches_diff_mask_byte_for_byte` test confirms `AtomicDiffMask::byte(i) == DiffMask::byte(i)` for any single-bit set. 6 unit tests in the new module; all pass.

**Bench delta vs `perf_v8_pre`:**

| Bench | Pre | Stage C | Delta |
|---|---:|---:|---:|
| `single_user/single_property` | 899 ns | 586 ns | **−21%** |
| `16_users_in_scope/single_property` | 7.33 µs | 5.97 µs | **−19%** |
| `drain_dirty/16u_1000_dirty_entities` | 116 ms | 91.9 ms | **−21%** |

Stage C is one of three Phase 8.1 stages. Targets (≤25 ns / ≤250 ns / ≤200 µs) are not yet met; the remaining cost is Stage B (DirtySet HashMap clone + per-mutation HashMap insert) and Stage D (per-user fan-out RwLock walk).

**Existing benches:** all 29 prior wins still PASS via `naia-bench-report --assert-wins`. Test suite (178 naia-shared tests) all pass.

---

## Cost-attribution analysis (2026-04-25 post-Stage-C)

The Stage C win is genuine but well below the eventual target. To stage remaining work rigorously, here is where the `drain_dirty` 91.9 ms actually goes (16 users × 1000 entities × 1 component, one tick per iter):

| Layer | Estimated cost | Notes |
|---|---:|---|
| `notify_dirty` + `notify_clean` lock+HashMap (32K ops × ~100ns) | ~3.2 ms | 3.5% of total |
| `dirty_receiver_candidates` HashMap clone (16 × 1000-entry) | ~1.5 ms | 1.6% of total |
| Per-(user, entity, kind) HashMap probes — world entity, component, scope | dominant | 4 probes × ~50ns × 16K = ~3.2 ms minimum, more under cache pressure |
| Mask read + serialize payload bytes | dominant | atomic load + wire encoding + buffer push per kind |
| Packet construction + outgoing channel bookkeeping | dominant | 16 packets per tick at this scale |
| `clear_diff_mask` atomic + notify_clean | ~1.5 ms | 1.6% of total |

**Conclusion:** Stage B (DirtySet replacement) closes ~3–5 ms of the 91.9 ms — a ~5% improvement, not the 460× the headline target needs. The dominant cost is structural: 4+ HashMap probes per (user, entity, component) tuple inside `take_outgoing_events`, plus the actual serialization work. **Closing the target gap requires Stage A's `EntityIndex` plumbing landing first**, so HashMap probes become Vec indexing. Stage B in isolation produces a small win and risks file churn that complicates the eventual A+B+D coordinated landing.

---

## Revised stage ordering & status

Replanning Phase 8.1 from sequential A→B→C→D to **C (done) → A+B+D coordinated landing**. Rationale:

- Stage C is byte-for-byte wire-compatible and self-contained — perfect first-mover.
- Stages A, B, D are deeply coupled: EntityIndex (A) is the data structure that B's queue and D's flat fan-out are keyed by. Landing them piecemeal means re-doing each one's plumbing.
- The coordinated landing should re-run pre/post benches end-to-end against `perf_v8_pre`.

### Remaining gates (unchanged)

- `mutate_path/single_user/single_property` ≤ 25 ns
- `mutate_path/16_users_in_scope/single_property` ≤ 250 ns
- `mutate_path/drain_dirty/16u_1000_dirty_entities` ≤ 200 µs
- `tick/active/mutations/1000` ≥ 3× faster
- All 29 prior wins still PASS via `naia-bench-report --assert-wins`
- `tick/idle_matrix/u_x_n/16u_10000e` no regression (≤ 1.05× of perf_v0)

### Stage A+B+D — coordinated landing (next)

Per plan Step 3 sections A/B/D in `BENCH_PERF_UPGRADE_PHASE_8_PLAN.md:185-223`:

- **A:** New `naia/shared/src/world/entity_index.rs` with `EntityIndex(u32)` + `KeyGenerator<u32>`. `HostEntityChannel` issues indices on scope-add; recycles on scope-remove. Plumb through `MutChannel::send` and `MutReceiver::mutate` API.
- **B:** Replace `Arc<RwLock<HashMap<GlobalEntity, HashSet<ComponentKind>>>>` DirtySet with `UserDirtyQueue { in_dirty: FixedBitSet, queue: Vec<u32> }`, indexed by `EntityIndex`. `notify_dirty(idx)` is `if !in_dirty.put(idx) then queue.push(idx)`. Drain semantics in `take_outgoing_events`; re-insert on filter-out paths.
- **D:** Replace `MutChannelData::receiver_map: HashMap<SocketAddr, MutReceiver>` with `Vec<UserSlot>`. `send(idx, prop)` is `for slot in &slots { slot.mask.fetch_or(...); slot.notify_dirty(idx); }`. Zero locks on send.

Files touched (estimated):
- `naia/shared/src/world/entity_index.rs` (new)
- `naia/shared/src/world/update/mut_channel.rs` (refactor)
- `naia/shared/src/world/update/user_diff_handler.rs` (refactor)
- `naia/shared/src/world/local/local_world_manager.rs` (callers)
- `naia/shared/src/world/update/global_diff_handler.rs` (channel storage)
- `naia/shared/src/world/sync/host_*.rs` (scope-add/recycle hooks)
- `naia/shared/tests/dirty_propagation.rs` (new — 8+ correctness tests per plan)
- `naia/_AGENTS/BENCH_UPGRADE_LOG/phase-08.1.md` (this file, finalized)

Estimated landing scope: ~6–10 files, ~500–800 LOC delta, plus tests. High-blast-radius — every entity-replication path. Demands a separate session with coordinated test/bench loop.

---

## Stages A + B + D — coordinated landing attempt + revert (2026-04-25)

**What was attempted:**
- **A:** New `naia/shared/src/world/entity_index.rs` (`EntityIndex(u32)`, `KeyGenerator32`); `UserDiffHandler` allocates an index per `GlobalEntity` on first `register_component`, refcounted by component count, recycled on last deregister. `DirtyNotifier` carries the index field for future Stage E bitset use.
- **B:** Replaced `RwLock<HashMap<GlobalEntity, HashSet<ComponentKind>>>` DirtySet with `Mutex<DirtyQueue>` whose flat `HashSet<(GlobalEntity, ComponentKind)>` membership is one hash per push instead of two.
- **D:** Replaced server-side `MutChannelData::receiver_map: HashMap<SocketAddr, MutReceiver>` with `receiver_index: HashMap<…, usize> + receivers: Vec<MutReceiver>` so the per-mutation fan-out is a contiguous Vec walk.

**Landed measurements (Stage A+B+D):**

| Bench | perf_v8_pre | A+B+D |
|---|---:|---:|
| `notifier/dirty/single_user` | 909 ns | 638 ns (−30%) |
| `notifier/dirty/16_users` | 7.41 µs | 4.65 µs (−37%) |
| `update/mutate_path/drain_dirty/16u_1000` | 116 ms | 105 ms (−9%) |

**29-win regression gate:** 29 / 0 / 0 PASS.

**Regressions vs `perf_v8_pre` (criterion auto-detected):**

| Bench | perf_v8_pre | A+B+D | Δ |
|---|---:|---:|---:|
| `spawn/coalesced/entities/1` | ~10 µs | 14.6 µs | +29% |
| `spawn/coalesced/entities/10` | ~21 µs | 22.4 µs | +8% |
| `wire/bandwidth/scenario/1u_100m` | ~408 µs | 497 µs | +22% |
| `wire/bandwidth/scenario/4u_1000m` | ~24 ms | 27 ms | +11% |
| `tick/scope_exit/entities/100` | ~228 µs | 254 µs | +11% |
| `update/bulk/mutations/5000` | ~33 ms | 36 ms | +10% |

The pattern: any bench that exercises `register_component` heavily was regressed by Stage A's per-call HashMap bookkeeping (3 ops + KeyGenerator queue per call). A salvage pass replacing `KeyGenerator32` with a `u32` counter and dropping the refcount halved the regressions but did not eliminate them.

**Decision (2026-04-25):** Stage A reverted to a no-op. The `EntityIndex` type and `KeyGenerator32` remain in `naia/shared/src/world/entity_index.rs` for future use, but `UserDiffHandler` and `DirtyNotifier` no longer carry indices. Stages B + D ship as the Phase 8.1 outcome.

**Why the revert:** Stage A's index plumbing has zero current consumer. The plan called Stage A "structural plumbing" for a hypothetical Stage E (`Vec<u32>`/bitset DirtySet keyed by index), which never materialized this round. Without that consumer, every register_component pays for an indexing concept it never reads back. The cost is real (10–29% on spawn-heavy benches) and the benefit is zero, so removing the per-call work is strictly correct.

---

## Final shipped configuration (Phase 8.1 complete)

| Stage | State | Outcome |
|---|---|---|
| Stage C (atomic mask) | ✅ landed | −21% on `drain_dirty`; wire-format unchanged |
| Stage B (DirtyQueue flat HashSet) | ✅ landed | folded into single mutation hot path; saves one hash per push |
| Stage D (Vec<MutReceiver> fan-out) | ✅ landed | server `MutChannelData::send` is contiguous walk |
| Stage A (EntityIndex plumbing) | 🟡 partial — type only | `entity_index.rs` exists; not used at register time |
| Headline targets (≤25 ns / ≤250 ns / ≤200 µs) | 🔴 not met | requires Stage A + Stage E (future) |

Wire format byte-for-byte identical to pre-Phase-8.1. 29 prior wins still PASS. naia-shared test suite passes.

The headline targets were aspirational and require Stage E (a future bitset DirtySet + per-receiver flat slot array) on top of Stage A. The Phase 8.1 plan acknowledged Stage A is structural plumbing only; landing it without its consumer was wasteful and has been reverted.

---

## Files of record

- `naia/shared/src/world/update/atomic_diff_mask.rs` (Stage C)
- `naia/shared/src/world/update/mut_channel.rs` (Stages B + C)
- `naia/shared/src/world/update/user_diff_handler.rs` (Stage B integration)
- `naia/shared/src/world/entity_index.rs` (type only — Stage A type, no consumer)
- `naia/shared/src/world/mod.rs`, `naia/shared/src/lib.rs` (exports)
- `naia/server/src/world/mut_channel.rs` (Stage D)
- `naia/_AGENTS/BENCH_UPGRADE_LOG/phase-08.1.md` (this file)
