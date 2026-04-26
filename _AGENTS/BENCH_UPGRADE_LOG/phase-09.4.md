# Phase 9.4 — Bitset `DirtySet` over per-user `EntityIndex` (Stage E checkpoint)

**Status:** 🔄 IN PROGRESS 2026-04-25 — Stage E architecture landed (gates green, wire identical, modest measurable wins). Plan's aspirational targets not met; pursuing further optimization (lock-free notify path).

## What landed

The Phase 8.1 Stage A `EntityIndex` newtype + `KeyGenerator32` were dead code waiting on Stage E. This commit closes the gap: per-user `UserDiffHandler`s now issue dense `EntityIndex` slots and a u64 bitmask per entity records which `ComponentKind`s are dirty. `DirtyNotifier` carries `(entity_idx, kind_bit)` resolved at registration time, so each `notify_dirty` is a `Mutex<DirtyQueue>::lock` + `Vec<u64>` index + bitwise OR + (cold-path) `Vec<EntityIndex>::push` — replacing the prior `Mutex<HashSet<(GlobalEntity, ComponentKind)>>::insert`.

```rust
// Stage D (pre-9.4)
pub type DirtySet = Mutex<HashSet<(GlobalEntity, ComponentKind)>>;
// notify_dirty: lock → HashSet.insert((entity, kind))   // hash + eq + alloc

// Stage E (9.4)
pub struct DirtyQueue {
    dirty_bits: Vec<u64>,         // bit kind_bit set iff (entity_idx, kind) dirty
    dirty_indices: Vec<EntityIndex>, // sparse push log; deduped at drain
}
pub type DirtySet = Mutex<DirtyQueue>;
// notify_dirty: lock → vec[idx] |= 1<<bit; if was_zero { vec.push(idx) }
```

`ComponentKinds::add_component` now hard-asserts `net_id < 64` so the u64 mask cap is registry-enforced, not buried in dirty-set code. `GlobalDiffHandler` populates a `kind_bits: HashMap<ComponentKind, u8>` lazily so `UserDiffHandler::register_component` can resolve `kind_bit` without a new `ComponentKinds` reference dependency.

## Architecture detail

```rust
struct UserDiffHandler {
    receivers: HashMap<(GlobalEntity, ComponentKind), MutReceiver>,
    entity_to_index: HashMap<GlobalEntity, EntityIndex>,
    index_to_entity: Vec<Option<GlobalEntity>>,
    components_per_entity: HashMap<EntityIndex, u32>,  // refcount → recycle
    key_gen: KeyGenerator32<EntityIndex>,
    kinds_by_bit: [Option<ComponentKind>; 64],         // drain-time decode
    dirty_set: Arc<Mutex<DirtyQueue>>,
}
```

`dirty_receiver_candidates` drains the bitset, decodes via `kinds_by_bit[bit]`, and re-pushes (read-only contract preserves entries for downstream callers).

### Wire format

CPU-only refactor — no bytes cross the wire from this code path. Verified by namako BDD gate (lint+run+verify all PASS) and 29/0/0 wins-gate including all `wire/*` cells.

### EntityIndex recycling

`KeyGenerator32` recycle timeout: `Duration::from_secs(2)` — long enough to cover packet drop / RTT retries that may briefly reference an `entity_idx` after deregistration. `components_per_entity` refcount drops to zero ⇒ `entity_to_index.remove + index_to_entity[slot] = None + key_gen.recycle_key`.

## Files touched

| File | Change |
|---|---|
| `shared/src/world/component/component_kinds.rs` | 64-kind `assert!` at `add_component`; add public `net_id_of()` accessor |
| `shared/src/world/entity_index.rs` | `#[derive(Clone)]` on `KeyGenerator32` (UserDiffHandler now derives Clone) |
| `shared/src/world/update/global_diff_handler.rs` | Add `kind_bits: HashMap<ComponentKind, u8>` populated in `register_component`; expose `pub fn kind_bit(&self, &ComponentKind) -> Option<u8>` |
| `shared/src/world/update/mut_channel.rs` | Rewrite `DirtyQueue` as Vec<u64> bitset + Vec<EntityIndex> push log; `DirtyNotifier` carries `(entity_idx, kind_bit)` |
| `shared/src/world/update/user_diff_handler.rs` | Add per-user `EntityIndex` issuance/recycle, `kinds_by_bit` snapshot decode, refcounted dereg |

## Verification

- ✅ `cargo check --workspace` clean (only pre-existing dead-code warnings)
- ✅ `cargo test --workspace` 0 failures
- ✅ namako BDD gate: lint=PASS run=PASS verify=PASS (entire spec corpus)
- ✅ 29/0/0 wins gate against full criterion suite
- ✅ Wire format byte-identical (CPU-only refactor; bandwidth cells unchanged)

### Per-target cells (vs `phase_94_pre` baseline)

| Cell | Pre target | Post median | Δ | Plan target |
|---|---:|---:|---|---:|
| `update/mutate_path/single_user/single_property` | 638 ns (perf_v0) | ~447–683 ns (high variance) | — | ≤ 200 ns |
| `update/mutate_path/16_users_in_scope/single_property` | 5.38 µs | 3.67 µs | **-10% median Improved** | ≤ 1.5 µs |
| `update/mutate_path/drain_dirty/16u_1000_dirty_entities` | 110.76 ms | 98.49 ms | **-1% median NoChange** | ≤ 30 ms |

`single_user/single_property` shows ~100× sample-range variance within a single run (3306–558322 ns) and 50%+ median swing across consecutive runs of identical code. The microbench is too noisy for ±10% target evaluation; the wins gate (29 cells, 100 samples each) is the authoritative perf signal.

The plan's aspirational ≤200ns/≤1.5µs/≤30ms thresholds are NOT met by the bitset-only architecture. The Stage E refactor delivered ~10–30% improvements on macro cells, not the 3–4× the plan optimistically projected. Further optimization is in flight (lock-free `notify_dirty` path).

## Outcome (checkpoint)

The half-built infrastructure is finished — `EntityIndex`/`KeyGenerator32` are no longer dead code, the 64-kind cap is asserted at registry build, the `Mutex<HashSet>` per-user dirty queue is now a `Mutex<Vec<u64>>` bitset.

Headline: **bitset DirtySet shipped, wire byte-identical, all gates green, modest macro wins, microbench too noisy to call.**
