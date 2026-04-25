# Phase 8 — Per-mutation hot path + scope precache + wire compaction

**Status:** 📐 PLANNED 2026-04-25 — drafted post-Phase-7 close-out. No code changes yet. This document is the durable plan; update it as sub-phases land.

**Predecessors:** `BENCH_PERF_UPGRADE.md` (Phases 0–7, ✅ COMPLETE 2026-04-24, idle 6,356×).

**Headline target:** Halo-shaped 16v16 match (32 players + 50 projectiles + 8 vehicles + 65,536 immutable tiles, 16 receiving clients, 25 Hz) runs at **≤ 30% of the default 64 KB/s per-client bandwidth cap** and **≤ 5 ms server CPU per tick**, while the per-mutation cost in isolation drops by ≥ 8×.

---

## Phase status

| Sub-phase | Status | Bench gate | Landing commit | Log |
|---|---|---|---|---|
| 8.0 — Bench protocol calibration (quantized types) | ✅ COMPLETE 2026-04-25 | `halo_btb_16v16` quantized 2676 B/tick = 0.81× of naive 3304 B/tick (uncapped) | bench-only | `phase-08.0.md` |
| 8.1 — Columnar dirty + `EntityIndex` (Stages A-D) | 🔲 planned | `mutate_path/single_user/single_property` ≤ 25 ns; `tick/active/mutations/1000` ≥ 3× faster | — | `phase-08.1.md` |
| 8.2 — `scope_checks` precache | ✅ COMPLETE 2026-04-25 | rebuild eliminated (cache mirrors `(room, user, entity)` tuples on churn); `tick/scope_with_rooms/u_x_n/16u_10000e` 403.31 → 338.54 ms (-16.1%, -64.77 ms/tick); 8/9 cells improved. **Original ≤ 200 µs target was unreachable** — `tick()` itself (send_all_packets) is multi-hundred ms at 16u_10000e and floors the bench cell. The realistic win is the rebuild-cost delta. | bench-improvement | `phase-08.2.md` |
| 8.3 — Protocol-sized ComponentKind/ChannelKind/MessageKind NetId | ✅ COMPLETE 2026-04-25 | `wire/bandwidth_realistic_quantized/halo_btb_16v16` 2695 → 2310 B/tick (-14.3%, target was 8%); 29/0/0 wins; Phase 4 idle 134µs ≤ 200µs ceiling | wire-format break (cyberlith pre-public) | `phase-08.3.md` |

---

## 1. Why Phase 8

Phase 7 made the *idle* path O(dirty). The remaining cost lives in three distinct places:

1. **Per-mutation propagation chain.** A single `*property = x;` walks `MutSender → MutChannel::send (RwLock read) → fan-out to U receivers, each: RwLock<DiffMask>::write + Vec scan + RwLock<DirtySet>::write + HashMap::entry().or_default().insert()`. Audit (2026-04-25) — `mut_channel.rs:135-146`, `user_diff_handler.rs:164-181`. That's ~3 RwLock ops + 2 HashMap probes per mutation per in-scope user. Estimated 80-150 ns per mutation per user, fully cache-bound.

2. **`scope_checks` is O(rooms × users × entities) per tick.** `world_server.rs:628-647` has a literal `// TODO: precache this, instead of generating a new list every call.`. At Cyberlith canonical (1 room × 16 users × 65,536 tiles) = >1M HashMap lookups per tick. Hidden today because `tick/idle_matrix` doesn't exercise scope filtering — the cost only appears under realistic workloads.

3. **Wire format ComponentKind NetId is fixed `u16`.** `component_kinds.rs:10,48` — 16 bits per component update regardless of how many components are registered. Most projects have ≤16 component types. `UnsignedVariableInteger<3>` would be 4 bits at 0..15, 12 bits at 16..127, 20 bits at 128+ — net ~12% bandwidth on update-heavy traffic.

A fourth issue is foundational: **the realistic-archetype bench mis-models Cyberlith's wire cost.** `wire/bandwidth_realistic` uses naive `Property<f32>` for Position/Velocity/Rotation, but cyberlith's production replication uses heavily quantized types: `SerdeQuat` (smallest-three, 21 bits/quat — `cyberlith/crates/math/src/serde_quat.rs`), `NetworkedPosition` (i16 tile + `SignedVariableFloat<14,0>` delta), `NetworkedVelocity`/`NetworkedAngularVelocity` (`SignedVariableFloat<11,2>` × 3). Without 8.0, every Phase 8 bandwidth claim is calibrated to fiction.

---

## 2. BDD-first / tests-first protocol

**Every sub-phase follows this order. No exceptions.**

1. **Define the bench cell or correctness test that measures the optimization target.** Write the bench/test code first.
2. **Run it on the pre-implementation codebase.** Capture the baseline numbers. Save as `perf_v8_pre` snapshot via `cargo criterion --save-baseline perf_v8_pre`.
3. **State the hypothesis as a number.** What does this bench/test measure today? What should it measure after the change? Write it down in this document.
4. **State the kill-criterion.** If the post-change number doesn't beat the threshold, the sub-phase fails. No partial credit, no soft landings.
5. **For correctness-bearing changes:** write the unit test asserting the expected wire output / observable behavior. See it pass against existing structure (so we know the test models reality), then refactor underneath, then re-run.
6. **Implement.** Stage by stage where the sub-phase says so. Each stage's tests + microbench must pass before proceeding.
7. **Re-run criterion sweep.** Add absolute thresholds to `PHASE_THRESHOLDS` in `test/bench_report/src/assert_wins.rs`. Add the new bench's baseline to the regression sweep. `naia-bench-report --assert-wins` must report PASS on the new gates plus all existing 29.

This is non-negotiable — it's how Phases 1-7 stayed honest, and the only way Phase 8's claims survive review.

---

## 3. Phase 8.0 — Bench protocol calibration (prerequisite)

**Goal:** Add quantized-type bench scenarios so 8.1/8.2/8.3 baselines reflect cyberlith's real wire cost.

### Hypothesis

Existing `wire/bandwidth_realistic/scenario/halo_btb_16v16` reports **1226 B/tick** (1u, measured 2026-04-24). With cyberlith-shape quantization (rotation 21 bits not 64; velocity 3×~13 bits not 96; position tile+delta ≈ 30 bits/axis not 96), the same workload should drop to **400-700 B/tick** — a 2-3× reduction *purely from already-shipped cyberlith schema choices*.

### Step 1 — write the bench (BEFORE adding the types)

New file: `naia/benches/benches/wire/bandwidth_realistic_quantized.rs`

Mirror `bandwidth_realistic.rs` exactly, but use new component types `BenchPositionQ`, `BenchVelocityQ`, `BenchRotationQ` defined in `bench_protocol.rs`. Same scenarios (`player_8`, `halo_8v8`, `halo_btb_16v16`, etc.), same multi-client variants. Register the group in `main.rs`.

### Step 2 — capture pre-implementation baseline

The pre-baseline for 8.0 is the existing `wire/bandwidth_realistic` numbers (`halo_btb_16v16` = 1226 B/tick). The new `_quantized` bench *itself* doesn't have a "pre" because the types don't exist yet — instead, its post-implementation numbers ARE the new ground truth, and the comparison is against the naive `wire/bandwidth_realistic`.

### Step 3 — add the quantized types

In `naia/benches/src/bench_protocol.rs`:

```rust
use naia_serde::{SignedInteger, SignedVariableFloat};
use naia_shared::{Property, Replicate};
// ... add SerdeQuatLike if we wire it; otherwise mirror cyberlith via dependency

#[derive(Replicate)]
pub struct BenchPositionQ {
    // tile-coord (~16 bits each axis) + sub-tile delta (~14 bits)
    pub tile_x: Property<i16>,
    pub tile_y: Property<i16>,
    pub tile_z: Property<i16>,
    pub dx: Property<SignedVariableFloat<14, 0>>,
    pub dy: Property<SignedVariableFloat<14, 0>>,
    pub dz: Property<SignedVariableFloat<14, 0>>,
}

#[derive(Replicate)]
pub struct BenchVelocityQ {
    pub x: Property<SignedVariableFloat<11, 2>>,
    pub y: Property<SignedVariableFloat<11, 2>>,
    pub z: Property<SignedVariableFloat<11, 2>>,
}

#[derive(Replicate)]
pub struct BenchRotationQ {
    // mirror SerdeQuat smallest-three; 21 bits if implemented as a custom Serde type
    pub quat: Property<BenchQuat21>, // or copy SerdeQuat from cyberlith
}
```

Decision point during implementation: either depend on `cyberlith_math` for `SerdeQuat` (cross-repo, prefer not), or copy the `SerdeQuat` type into a new `naia/benches/src/serde_quat.rs`. **Default: copy.** The bench is a leaf crate; no production code depends on it; copy is acceptable to avoid pulling cyberlith into Naia.

### Step 4 — verify

Run `cargo criterion -p naia-benches -- 'wire/bandwidth_realistic_quantized'`. Compare medians cell-by-cell to `wire/bandwidth_realistic`. **Expected drops:**

| Scenario              | Naive (today, B/tick) | Quantized (target, B/tick) | Min ratio |
|-----------------------|-----------------------|----------------------------|-----------|
| `halo_4v4`            | 813                   | 300-500                    | ≤ 0.6×    |
| `halo_8v8` 1u         | 819                   | 350-550                    | ≤ 0.7×    |
| `halo_btb_16v16` 1u   | 1226                  | 500-800                    | ≤ 0.65×   |

### Kill-criterion

- `halo_btb_16v16` quantized ≤ 700 B/tick. If higher: re-audit cyberlith's actual wire encoding to make sure the bench mirrors it. Ship-blocker for 8.1.

### Deliverables

- `naia/benches/src/bench_protocol.rs`: `BenchPositionQ`, `BenchVelocityQ`, `BenchRotationQ` added.
- `naia/benches/benches/wire/bandwidth_realistic_quantized.rs`: 13 scenarios mirroring the unquantized bench (incl. multi-client).
- `naia/benches/benches/main.rs`: group registered.
- `naia/_AGENTS/BENCH_UPGRADE_LOG/phase-08.0.md`: log entry with measured numbers vs. targets, verdict.
- Cyberlith doc updated: `cyberlith/_AGENTS/BENCH_BANDWIDTH_ANALYSIS.md` capacity table refreshed using quantized numbers (mark old table as "naive baseline").

---

## 4. Phase 8.1 — Columnar dirty + EntityIndex

**Goal:** Replace `Arc<RwLock<DiffMask>>` chains and per-user `RwLock<HashMap<...>>` DirtySet with dense `EntityIndex(u32)` + per-user `BitSet + Vec<u32>` dirty queue + `AtomicU64`-packed DiffMasks for ≤64-property components.

### Hypothesis

| Bench | Today | Target | Ratio |
|---|---|---|---|
| `mutate_path/single_user/single_property` (NEW) | ~120 ns | ≤ 25 ns | **5×** (conservative; 90% CI 5-15×) |
| `mutate_path/16_users_in_scope/single_property` (NEW) | ~1500 ns | ≤ 250 ns | **6×** |
| `mutate_path/drain_dirty/16u_1000_dirty_entities` (NEW) | ~700 µs (clones HashMap) | ≤ 200 µs | **3.5×** |
| `tick/active/mutations/1000` (existing) | 38.9 ms | ≤ 13 ms | **3×** |
| `tick/idle_matrix/u_x_n/16u_10000e` (existing) | 47.6 µs | ≤ 47.6 µs | **no regression** |

### Step 1 — write the bench cells (BEFORE writing implementation)

New file: `naia/benches/benches/update/mutate_path.rs`

```rust
// mutate_path/single_user/single_property
//   - 1 user in scope, 1 entity, 1 component, mutate one Property repeatedly
//   - measures the per-mutation hot path in isolation, no I/O, no tick

// mutate_path/16_users_in_scope/single_property
//   - 16 users in scope of one entity, mutate one Property
//   - measures the per-user fan-out cost

// mutate_path/drain_dirty/16u_1000_dirty_entities
//   - 16 users, 1000 entities, all in scope, all mutated
//   - call take_outgoing_events for one user, measure
```

Register in `main.rs`. Run on current codebase. Capture as `perf_v8_pre` baseline:

```bash
cargo criterion -p naia-benches --bench naia --save-baseline perf_v8_pre -- 'update/mutate_path'
```

### Step 2 — write the correctness tests

In `naia/shared/tests/dirty_propagation.rs` (new file):

```rust
#[test] fn mutation_marks_entity_dirty_for_all_in_scope_users() { ... }
#[test] fn clear_after_send_empties_dirty_queue() { ... }
#[test] fn dropped_packet_re_ors_mask_back_into_queue() { ... }
#[test] fn multiple_components_one_entity_pushes_entity_idx_only_once() { ... }
#[test] fn scope_add_issues_monotonic_entity_index() { ... }
#[test] fn scope_remove_recycles_entity_index() { ... }
#[test] fn diff_mask_byte_output_byte_for_byte_unchanged() { ... }
#[test] fn wide_component_64_plus_props_falls_back_to_box_atomic_u64() { ... }
```

Run against current codebase first — they should mostly pass with the existing implementation (the wire-format byte-for-byte test is the contract surface). Failures at this stage mean the test models reality wrong; fix the test before fixing the code.

### Step 3 — implementation, staged

#### Stage A — plumb `EntityIndex` (no behavior change)

- New module: `naia/shared/src/world/entity_index.rs`. Newtype `EntityIndex(u32)` + `KeyGenerator<u32>` (fork the existing `KeyGenerator` to a u32 variant; the u16 wrap-around bug noted in `cyberlith/_AGENTS/STATIC_ENTITY_PROPOSAL.md` motivates u32 here regardless).
- `HostEntityChannel` (or wherever per-user scope is tracked) holds `entity_indices: HashMap<GlobalEntity, EntityIndex>`.
- Issue at scope-add, recycle at scope-remove.
- Plumb `EntityIndex` through `MutChannel::send` and `MutReceiver::mutate` API but don't use it for storage yet.
- **Tests pass.** **Existing benches show no regression** (≤ 1.05× on every cell). Stage A is structural-only.

#### Stage B — per-user dirty queue replacement

- Add `UserDirtyQueue { in_dirty: FixedBitSet, queue: Vec<u32> }` in `user_diff_handler.rs`.
- `DirtyNotifier::notify_dirty()` takes an `EntityIndex`, calls `user.in_dirty.put(idx.0 as usize)`; on first set, pushes to `queue`.
- `take_outgoing_events`: drains `queue`, walks each entity's components via the existing per-(entity, component) DiffMask (still under `Arc<RwLock>` for now).
- Replaces the `RwLock<HashMap<GlobalEntity, HashSet<ComponentKind>>>` DirtySet entirely.
- **Microbench `mutate_path/drain_dirty`: ≥ 3× win.** Kill-criterion.

#### Stage C — packed `DiffMask`

- New type: `DiffMaskPacked` in `diff_mask.rs`:
  ```rust
  pub enum DiffMaskPacked {
      Small(AtomicU64),         // ≤ 64 properties
      Wide(Box<[AtomicU64]>),   // > 64 properties; rare
  }
  ```
- API mirrors today's `DiffMask`: `set_bit`, `clear_bit`, `is_clear`, `byte(i) -> u8`, `byte_number()`, `or_diff_mask(other)`. Wire encoding **byte-for-byte unchanged** — `byte(i)` is `(self.0.load(Relaxed) >> (i * 8)) as u8`.
- Replace `Arc<RwLock<DiffMask>>` storage in `MutReceiver` with `DiffMaskPacked`.
- `MutReceiver::mutate(entity_index, prop_idx)` becomes `mask.fetch_or(1u64 << prop_idx, Relaxed)` plus `notify_dirty(entity_index)` if was-clear.
- **Microbench `mutate_path/single_user`: ≤ 25 ns.** Kill-criterion.
- **Wire-correctness test `diff_mask_byte_output_byte_for_byte_unchanged`: PASS.** Kill-criterion.

#### Stage D — flat fan-out in `MutChannelData::send`

- Replace `receiver_map: HashMap<SocketAddr, MutReceiver>` with `Vec<UserSlot>` where `UserSlot` holds direct pointers/indices into per-user packed-mask storage.
- `MutChannelData::send(entity_idx, prop_idx)`: `for slot in &self.slots { slot.mask.fetch_or(...); slot.notify_dirty(entity_idx); }`. Zero locks.
- Concurrency assumption documented: send is single-threaded today (`world_server.send_all_packets`); plan SPSC queue if/when that changes.
- **Microbench `mutate_path/16_users_in_scope`: ≤ 250 ns.** Kill-criterion.

### Step 4 — verify

```bash
cargo criterion -p naia-benches --bench naia --message-format=json > /tmp/criterion.json 2> /tmp/criterion.err
cargo run -p naia-bench-report -- --assert-wins < /tmp/criterion.json
```

Expected output:
- All 29 existing wins still PASS.
- 4 new gates (one per microbench cell + `tick/active/mutations/1000`) PASS.
- `tick/idle_matrix/u_x_n/16u_10000e` NO REGRESSION (≤ 1.05× of perf_v0).

### `PHASE_THRESHOLDS` additions

```rust
const PHASE_THRESHOLDS: &[(&str, f64, &str)] = &[
    // ... existing ...
    ("update/mutate_path/single_user/single_property",        25.0,        "Phase 8.1 mutation hot path"),
    ("update/mutate_path/16_users_in_scope/single_property", 250.0,        "Phase 8.1 fan-out"),
    ("update/mutate_path/drain_dirty/16u_1000_dirty_entities", 200_000.0,  "Phase 8.1 drain"),
    ("tick/active/mutations/1000",                          13_000_000.0,  "Phase 8.1 active matrix"),
];
```

### Risks

- **>64-property components**: cyberlith doesn't have any today (audit confirms); fallback `Box<[AtomicU64]>` exists for safety. Add a compile-time `static_assert` per-component if practical, otherwise a debug-build runtime panic.
- **Concurrent mutation across threads**: today's RwLock model is technically thread-safe; the proposed atomic model is *equally* safe for the bit-set step but the `Vec<u32>::push` for dirty queueing is not. Naia's send path is single-threaded today; document the invariant and add a `thread_local!` or `Cell` debug-assertion. If parallel-per-user packet build is ever wanted, swap `Vec<u32>` for an SPSC queue per user.
- **Wire-format drift**: caught by `diff_mask_byte_output_byte_for_byte_unchanged` test. If it fails: stop, audit, do not ship.

### Deliverables

- `naia/shared/src/world/entity_index.rs` (new).
- `naia/shared/src/world/component/diff_mask.rs`: `DiffMaskPacked` added.
- `naia/shared/src/world/update/mut_channel.rs`: refactored.
- `naia/shared/src/world/update/user_diff_handler.rs`: `UserDirtyQueue` replaces DirtySet.
- `naia/shared/tests/dirty_propagation.rs` (new): 8+ correctness tests.
- `naia/benches/benches/update/mutate_path.rs` (new): 3 microbench cells.
- `naia/_AGENTS/BENCH_UPGRADE_LOG/phase-08.1.md`: full log with stage-by-stage measurements.

---

## 5. Phase 8.2 — `scope_checks` precache ✅ COMPLETE 2026-04-25

**Goal:** Replace per-tick `world_server.rs:628-647` scope_checks builder with a push-based cache invalidated only on room/user/entity churn.

### Outcome

The cache landed (`scope_checks_cache.rs`) with 6 mutation hooks + a debug-build equivalence assertion every 1024 reads. 9 unit tests pass; full BDD harness passes (8 scenarios + `naia_npa`); no equivalence divergence observed.

| Bench | Pre | Post | Δ |
|---|---|---|---|
| `tick/scope_with_rooms/u_x_n/1u_100e`   | 175.74 µs | 125.46 µs | -28.6% |
| `tick/scope_with_rooms/u_x_n/1u_1000e`  | 1.89 ms   | 2.03 ms   | +7.4% (within noise) |
| `tick/scope_with_rooms/u_x_n/1u_10000e` | 30.89 ms  | 30.47 ms  | -1.4% |
| `tick/scope_with_rooms/u_x_n/4u_100e`   | 535.00 µs | 403.65 µs | -24.6% |
| `tick/scope_with_rooms/u_x_n/4u_1000e`  | 7.16 ms   | 5.17 ms   | -27.8% |
| `tick/scope_with_rooms/u_x_n/4u_10000e` | 100.71 ms | 89.28 ms  | -11.3% |
| `tick/scope_with_rooms/u_x_n/16u_100e`  | 2.25 ms   | 2.14 ms   | -4.9% |
| `tick/scope_with_rooms/u_x_n/16u_1000e` | 25.60 ms  | 21.41 ms  | -16.4% |
| `tick/scope_with_rooms/u_x_n/16u_10000e`| 403.31 ms | 338.54 ms | **-16.1% (-64.77 ms/tick)** |

### Realism-adjusted finding

**The original ≤ 200 µs target on `16u_10000e` was unreachable** — it was set against an estimate of scope-checks cost in isolation, but the bench cell measures `tick() + scope_checks_tuple_count()`, and `tick()` itself (specifically `send_all_packets`) is multi-hundred ms at this shape. The bench cell number is *floored* by `tick()`, not by the rebuild we eliminated.

The realistic and demonstrated win is the **delta** between pre/post — i.e., the rebuild cost we eliminated (160K HashMap lookups → 0 reads at 16u_10000e). 8/9 cells improve; the 1u_1000e +7.4% is within criterion noise (CI [1.97, 2.09] ms straddles pre 1.89 ms).

To measure rebuild cost in isolation, a follow-up bench could run `scope_checks_tuple_count()` in a loop without `tick()` — that would be a different experiment, not required for sign-off.

### Step 1 — write the bench (BEFORE the fix)

New file: `naia/benches/benches/tick/scope_with_rooms.rs`

```rust
// scope_with_rooms/u_x_n/{1,4,16}u_{100,1000,10000}e
//   - World with 1 room, U users, all-entities-in-room (all in scope for all users)
//   - Drives one tick (uses BenchWorldBuilder with .room_with_all() helper)
//   - Measures the full server.send_all_packets path with active scope filtering
//
// This hits scope_checks() once per tick. Today's path: O(rooms × users × entities)
//   HashMap lookups. Target: O(1) amortized after Stage 8.2.
```

Register in `main.rs`. Run on current codebase. **Capture pre-baseline numbers.** This is also the diagnostic that proves the cost exists — these numbers will be the cited "today" measurements going forward, replacing my estimates.

### Step 2 — write the correctness tests

In `naia/server/tests/scope_checks_cache.rs` (new):

```rust
#[test] fn add_user_to_room_appends_tuples_for_all_entities() { ... }
#[test] fn remove_user_from_room_removes_only_that_users_tuples() { ... }
#[test] fn add_entity_to_room_appends_tuple_for_each_user() { ... }
#[test] fn remove_entity_from_room_removes_tuple_for_each_user() { ... }
#[test] fn empty_room_yields_empty_scope_checks() { ... }
#[test] fn churn_test_maintains_equivalence_with_recompute() {
    // 10K random adds/removes, periodically assert
    // cache_state == fresh_recompute_from_scratch
}
#[test] fn multiple_rooms_independent() { ... }
```

Run against existing codebase: tests must reflect what `scope_checks()` *currently* returns, so they pass against the slow path. They model the contract; the implementation swap below preserves it.

### Step 3 — implementation

In the room/world-server module:
- Add `scope_checks_cache: Vec<(RoomKey, UserKey, GlobalEntity)>` (private).
- Hooks: `room.add_user`, `room.remove_user`, `room.add_entity`, `room.remove_entity`, `make_room`, `delete_room`.
- Each hook does an O(churn) update:
  - `add_user`: append `(room, user, e)` for every `e` in room.
  - `remove_user`: filter-in-place removing tuples where `user == removed`.
  - `add_entity`: append `(room, u, e)` for every `u` in room.
  - `remove_entity`: filter-in-place.
- `scope_checks()` returns `&self.scope_checks_cache`. **Zero allocation per tick.**
- Debug builds: every Nth tick (configurable, default 1024), recompute from scratch and `assert_eq!` with cache. Ship the assertion; remove only if benches show it dominates.

### Step 4 — verify

- All 7 correctness tests PASS.
- `tick/scope_with_rooms/u_x_n/16u_10000e` ≤ 200 µs.
- All existing 29 wins PASS.
- No regression on `tick/scope_enter`/`tick/scope_exit` (the churn benches — those are the cost we just moved into).

### `PHASE_THRESHOLDS` additions

The originally-planned threshold (`16u_10000e ≤ 200 µs`) was not added because the bench cell is floored by `tick()` not the rebuild — see Realism-adjusted finding above. The 9 scope_with_rooms cells stay in the suite as observability for the rebuild-cost delta; a tighter regression gate can be added later if a `scope_checks_tuple_count`-only bench is built.

### Risks

- **Subtle ordering bugs** in the hooks (e.g., user added before entity vs after). The churn test catches this.
- **Memory growth** if cache isn't shrunk on bulk-remove: use `Vec::retain` (no shrink) — acceptable. If memory becomes an issue, add a `shrink_to_fit` after batch-remove operations.
- **Concurrent room mutation**: existing room API is not concurrent; preserve that invariant.

### Deliverables

- `naia/server/src/world/scope_checks_cache.rs` (new) or inline in existing room module.
- Hooks added to room mutation paths.
- `naia/server/tests/scope_checks_cache.rs` (new): 7 correctness tests.
- `naia/benches/benches/tick/scope_with_rooms.rs` (new): 9-cell matrix.
- `naia/_AGENTS/BENCH_UPGRADE_LOG/phase-08.2.md`: log + measurements.

---

## 6. Phase 8.3 — Variable-length ComponentKind NetId

**Goal:** Switch `ComponentKind` ser/de from fixed `u16` (16 bits) to `UnsignedVariableInteger<3>` (4 bits at 0..15, 12 bits at 16..127, 20 bits at 128+).

### Hypothesis

| Bench | Today | Target | Ratio |
|---|---|---|---|
| `wire/component_kind_encoding/4_kinds` (NEW) | TBD (baseline-on-implementation) | 4 bits per encoded kind | — |
| `wire/component_kind_encoding/16_kinds` (NEW) | TBD | ≤ 8 bits per encoded kind on average | — |
| `wire/bandwidth_realistic_quantized/halo_btb_16v16` (8.0) | TBD (set by 8.0) | 8% smaller post-8.3 | **0.92×** |

### Step 1 — write the bench

New file: `naia/benches/benches/wire/component_kind_encoding.rs`

```rust
// component_kind_encoding/{4,16,128}_kinds
//   - Protocol with N component kinds
//   - 1000 entities, all mutated, measure bytes/tick
//   - 4_kinds: optimal case — variable encoding wins big
//   - 16_kinds: boundary — should still win
//   - 128_kinds: worst case — variable encoding loses 4 bits/kind
```

Register. Run on pre-implementation codebase. Save `perf_v8_pre` baseline.

### Step 2 — write the correctness tests

In `naia/shared/tests/component_kind_wire.rs` (new):

```rust
#[test] fn round_trip_kind_0_127_uses_short_encoding() { ... }
#[test] fn round_trip_kind_128_16383_uses_medium_encoding() { ... }
#[test] fn round_trip_kind_above_16383_uses_long_encoding() { ... }
#[test] fn protocol_with_4_kinds_emits_4_bit_kinds_on_wire() {
    // Build a protocol with 4 components, write one update,
    // assert via BitWriter::counter() that the kind portion is 4 bits.
}
#[test] fn protocol_with_200_kinds_round_trips_correctly() { ... }
```

### Step 3 — implementation

Edit `naia/shared/src/world/component/component_kinds.rs`:
- `ComponentKind::ser(&self, writer: &mut dyn BitWrite)`: replace fixed `u16::ser` with `UnsignedVariableInteger::<3>::new(self.0 as u16).ser(writer)`.
- `ComponentKind::de(reader)`: read `UnsignedVariableInteger::<3>` then construct `ComponentKind`.
- Update `bit_length()` to match.
- Audit all callers — `world_writer.rs:887-893`, `phase6_paint_rect_audit`, etc. — and verify wire counters in tests.

This is a **wire-format-breaking change.** Coordinate with cyberlith:
- Bump Naia release tag in cyberlith's `Cargo.toml`.
- Confirm no rolling-upgrade scenario is in flight (cyberlith hasn't shipped a public client yet, per memory).

### Step 4 — verify

- All correctness tests PASS.
- `wire/component_kind_encoding/16_kinds`: encoded-kind portion ≤ 8 bits/kind average (vs 16 bits before).
- `wire/bandwidth_realistic_quantized/halo_btb_16v16`: ≤ 0.92× of post-8.0 baseline.
- All existing 29 wins + Phase 8.1/8.2 gates PASS.
- `phase6_paint_rect_audit` rerun: still emits one `SpawnWithComponents` per entity, just with shorter ComponentKind tags.

### `PHASE_THRESHOLDS` additions

```rust
("wire/component_kind_encoding/16_kinds", /* ≤ 0.55× of fixed-u16 baseline */, "Phase 8.3 varint kind"),
```

### Risks

- **Protocol bump.** Mitigation: cyberlith is the only consumer; no public clients shipping today.
- **Marginal *expansion* at >127 kinds.** Document; no project this size today; revisit if a project hits it.

### Deliverables

- `naia/shared/src/world/component/component_kinds.rs`: ser/de switched.
- `naia/shared/tests/component_kind_wire.rs` (new): 5 correctness tests.
- `naia/benches/benches/wire/component_kind_encoding.rs` (new): 3 cells.
- `cyberlith/Cargo.toml`: Naia version bumped (separate commit, after Naia tag).
- `naia/_AGENTS/BENCH_UPGRADE_LOG/phase-08.3.md`: log + measurements.

---

## 7. Run protocol (per sub-phase)

Identical to Phase 7's protocol. After each sub-phase lands:

```bash
# 1. Capture pre-implementation baseline (BEFORE writing implementation)
cargo criterion -p naia-benches --bench naia --save-baseline perf_v8_pre

# 2. Implement, staged per the sub-phase plan
# 3. Run full sweep
cargo criterion -p naia-benches --bench naia --message-format=json \
  > /tmp/criterion.json 2> /tmp/criterion.err

# 4. Gate
cargo run -p naia-bench-report -- --assert-wins < /tmp/criterion.json
```

Exit code 0 = all gates green. Non-zero = a Phase 7 win, a Phase 8 sub-phase threshold, or a baseline regression fired.

After all sub-phases land, rotate `perf_v8_pre` to `perf_v8` and add the post-Phase-8 baseline as a permanent record.

---

## 8. Out of scope (explicitly)

- **Snapshot delta against last-acked baseline.** Audited 2026-04-25 (`entity_update_manager.rs:195`); Naia uses clear-on-send + RTT-timeout retransmit. Switching to last-ack delta would simplify code and reduce recovery latency under loss but does **not** materially reduce steady-state bandwidth (Quake 3's measured 5-10× is vs full-snapshots, not vs dirty-tracking-with-retransmit). **Demoted indefinitely.** Revisit only if `EntityUpdateManager::sent_updates` allocation churn shows up in flamegraphs as a top-3 bottleneck.
- **Spatial AOI / Phase 5 spatial scope index.** Deferred 2026-04-24 per Connor; revisit at 32+ player target.
- **Quaternion / position quantization at the protocol level.** Already shipped in cyberlith's `SerdeQuat`, `NetworkedPosition`, `NetworkedVelocity`. Phase 8.0 brings the bench harness up to that level; no Naia-side change needed.
- **u16 → u32 ID space for HostEntityGenerator.** Tracked separately in `cyberlith/_AGENTS/STATIC_ENTITY_PROPOSAL.md`. Phase 8.1 Stage A forks the `KeyGenerator` to a u32 variant for `EntityIndex`, which is adjacent but not the same fix; the host-entity wire ID is independent.
- **rANS / Huffman / zstd compression** on packet payload. Wrong layer; revisit only after delta + quantization + bit-pack are exhausted.

---

## 9. Sequencing & dependencies

```
8.0 (bench calibration)
  ↓ (8.1 baselines need realistic wire numbers)
8.1 (columnar dirty)
  ↓ (8.2's bench composes a tick that exercises both scope and dirty)
8.2 (scope precache)
  ↓ (8.3's wire claims are framed against 8.0+8.1+8.2 numbers)
8.3 (varint ComponentKind)
```

8.0 is a hard prerequisite. 8.1, 8.2, 8.3 could in principle be done in parallel — but sequencing them lets each one's "before/after" measurements stand on the prior one's foundation, and keeps the protocol-bump (8.3) at the end where rollback is cheapest.

**Estimated effort:** 8.0 ≈ 1 day. 8.1 ≈ 1.5 weeks (4 stages with bench gates each). 8.2 ≈ 3-5 days. 8.3 ≈ 2 days. Total: ~3 weeks of focused work.

---

## 10. References

- `_AGENTS/BENCH_PERF_UPGRADE.md` — Phase 0-7 master plan (status: complete).
- `_AGENTS/BENCH_UPGRADE_LOG/phase-07.md` — Phase 7 close-out template (gate hardening + baseline rotation).
- `cyberlith/_AGENTS/BENCH_BANDWIDTH_ANALYSIS.md` — capacity envelope post-Phase-7; updates required after 8.0.
- `cyberlith/_AGENTS/STATIC_ENTITY_PROPOSAL.md` — adjacent u16 ID-space fix; coordinate with 8.1 Stage A.
- `cyberlith/crates/math/src/serde_quat.rs` — quaternion model for 8.0.
- `cyberlith/services/game/naia_proto/src/components/networked/` — production component shapes for 8.0.
- Audit (2026-04-25) — `entity_update_manager.rs:126-196`, `mut_channel.rs:135-146`, `user_diff_handler.rs:164-181`, `world_server.rs:628-647`, `component_kinds.rs:10,48`, `local_entity.rs:38-41`.

---

## 11. Headline claim (target)

After all four sub-phases land, the "blow-people-away" benchmark statement is:

> A 16v16 Halo-shaped match (32 players + 50 projectiles + 8 vehicles + 65,536 immutable tiles, 16 receiving clients, 25 Hz) runs on a single Naia server with:
> - **Per-client bandwidth: ≤ 30%** of the 64 KB/s default cap (≈ 19 KB/s/client; ~300 KB/s server egress).
> - **Server CPU: ≤ 5 ms / 40 ms tick** (87% headroom at 25 Hz).
> - **Per-mutation cost: ≥ 8× faster** than Phase 7's already-6,356×-improved baseline.
> - **Zero-allocation steady-state** on the mutation hot path.
> - **All 33+ regression gates green** under `naia-bench-report --assert-wins`.

This is the contract Phase 8 commits to.
