# PERF — Shared Component Update Blob

**Status:** SPEC (not yet implemented)
**Created:** 2026-05-13
**Context:** Driven by sub-phase profiling in cyberlith benchmark — `_AGENTS/CAPACITY_RESULTS.md`
**Touches:** `naia-shared` (BitWriter, WorldWriter, UserDiffHandler), `naia-server` (connection send path)

---

## Problem

Sub-phase profiling at 32 players (release profile, `game_server_tick` bench) shows:

| Phase | % of tick | Root cause |
|---|---:|---|
| `send_packet_loop` | **39.1%** | Per-user component serialization: `Serde::ser` × users × dirty entities |
| `take_update_events` | **25.8%** | Per-user dirty scan: ~5 HashMap lookups × users × dirty entities |

Both costs are **O(dirty\_entities × users)**. With 32 players all moving:
- 32 dirty entities (player avatars) × 32 users = **1,024 Serde::ser calls per tick**
- 32 dirty entities × 32 users × ~5 HashMap lookups = **~5,120 lookups per tick**

Only the per-user local entity ID differs. The component data (ComponentKind + DiffMask + ComponentValue) is **identical for every user** who has the same entity dirty with the same diff mask. We are doing 31/32 of the serialization work redundantly.

---

## Solution Overview

Two independent fixes, each addressing one of the two O(N²) costs:

- **Fix A — BitBlob pre-serialization:** serialize component body once per dirty entity; bitwise-copy into each user's packet (no wasted bits on the wire)
- **Fix B — Shared dirty candidates:** compute the entity-level lookup results once per tick; per-user scan only checks user-specific bits

These can be implemented and gated independently. Fix A is the higher-impact change (39.1% target).

---

## Fix A — BitBlob Pre-serialization

### Design

**Current flow (per user, per dirty entity):**
```
write LocalEntity             // per-user, variable bits
read component from ECS       // ECS read (expensive)
Serde::ser(diff_mask, value)  // bitpack into writer (expensive)
```

**Proposed flow:**
```
// Once per tick, before per-user loop:
for each dirty entity:
    blob = BitBlob::serialize(component_kinds, diff_mask, world)
    //     reads ECS once; bitpacks ComponentKind + DiffMask + ComponentValue into blob

// Per user, per dirty entity:
write LocalEntity             // per-user, variable bits (same as today)
writer.append_blob(&blob)     // bitwise shift-copy; O(blob.bit_count / 64) word ops
```

### BitBlob Type

```rust
/// Pre-serialized component body for a single dirty entity in a single tick.
/// Contains the bits that follow the per-user LocalEntity header in a component
/// update packet section: ComponentContinue + ComponentKind + DiffMask + ComponentValue
/// + ComponentContinue(false) finish bit, starting at bit 0.
pub struct BitBlob {
    pub words: Vec<u64>,
    pub bit_count: usize,
}
```

### BitWriter Extension

```rust
impl BitWriter {
    /// Append all bits from `blob` into `self` at the current write position.
    /// Preserves full bit-packing — zero wasted bits on the wire.
    /// Cost: O(blob.bit_count / 64) word operations.
    pub fn append_blob(&mut self, blob: &BitBlob) {
        let shift = self.bit_count % 64;
        if shift == 0 {
            // Aligned: fast path — extend words directly
            self.words.extend_from_slice(&blob.words);
        } else {
            // Unaligned: shift each word into place
            for &word in &blob.words {
                *self.words.last_mut().unwrap() |= word << shift;
                self.words.push(word >> (64 - shift));
            }
        }
        self.bit_count += blob.bit_count;
    }
}
```

> Note: The exact BitWriter internals may differ — adapt to its actual word storage. The invariant is that `append_blob` produces the same bit sequence as calling `Serde::ser` directly.

### Where Pre-serialization Happens

In `WorldWriter::write_updates` (or a new `write_updates_with_blobs` variant):

```rust
// Phase 0: pre-serialize all dirty entities (before per-user loop)
//   Called once from WorldServer::send_all_packets, before iterating connections.
pub fn precompute_update_blobs(
    component_kinds: &ComponentKinds,
    world: &W,
    update_events: &HashMap<GlobalEntity, HashSet<ComponentKind>>,
) -> HashMap<GlobalEntity, HashMap<DiffMaskKey, BitBlob>> {
    // DiffMaskKey = the specific set of dirty component kinds for this entity.
    // In the common case (single Property<T>), there is exactly one key per entity.
    // ...
}

// Phase 1: per-user write (inside send_all_packets loop)
//   Replaces current per-user ECS read + Serde::ser with blob lookup + append_blob.
fn write_updates_from_blobs(
    writer: &mut BitWriter,
    world_manager: &LocalWorldManager,   // for global→local entity ID
    update_events: &HashMap<GlobalEntity, HashSet<ComponentKind>>,
    blobs: &HashMap<GlobalEntity, HashMap<DiffMaskKey, BitBlob>>,
    entity_priority_order: &[GlobalEntity],
) { ... }
```

### Diff Mask Grouping

The component body is identical across users **only when their diff masks match**. Two cases:

**Single `Property<T>` component (e.g. `NetworkedPosition`, `NetworkedVelocity`):**
- Diff mask = 1 bit; always `1` when the entity is in `update_events`
- All users share the same blob — no grouping needed
- This is the 100% case for our current hot path

**Multi-property component with per-user diff masks:**
- Different users may have received different property subsets from prior packets
- Group users by their diff mask; pre-serialize once per unique mask
- In practice rare (requires bandwidth-deferral mid-send for a multi-property component)
- Fallback: per-user serialization (existing code) when blob lookup misses

### Cost After Fix A

| Operation | Before | After |
|---|---|---|
| ECS reads per dirty entity | 32 (× users) | 1 |
| `Serde::ser` calls per dirty entity | 32 | 1 |
| Bitwise copy per dirty entity per user | 0 | 1 (O(blob\_bits/64) word ops) |
| `write LocalEntity` per user | unchanged | unchanged |

For `NetworkedPosition` (~60 bits): `append_blob` = 1–2 64-bit word operations per user per dirty entity.

---

## Fix B — Shared Dirty Candidates

### Problem Detail

`take_update_events` per user:
1. `build_dirty_candidates_from_receivers()` — scans per-user `DirtySet`
2. For each dirty entity: `entity_is_replicating`, `global_entity_to_entity`, `has_entity`, `has_component`, `diff_mask_is_clear` — ~5 HashMap lookups
3. Retain only entities that pass all checks

Steps 1 and 2 run 32 times (once per user) for the **same 32 dirty entities**. Steps 1 and 2's entity-level checks (is_replicating, has_entity, has_component) don't depend on which user is asking — they're pure entity-level facts.

### Design

Pre-compute a **server-global dirty candidate set** once per tick at the `WorldServer` level, before the per-connection send loop:

```rust
// Once per tick (in WorldServer::send_all_packets, before user loop):
let global_dirty: HashMap<GlobalEntity, HashSet<ComponentKind>> =
    self.compute_global_dirty_candidates(world, converter);
// Filters: entity_is_replicating + has_entity + has_component
// Does NOT filter diff_mask_is_clear (that's per-user)
```

Per user, `take_update_events` is replaced with a cheaper lookup:

```rust
// Per user:
fn take_update_events_from_global(
    &mut self,
    global_dirty: &HashMap<GlobalEntity, HashSet<ComponentKind>>,
) -> HashMap<GlobalEntity, HashSet<ComponentKind>> {
    global_dirty
        .iter()
        .filter_map(|(entity, kinds)| {
            // Only user-specific checks remain:
            if self.paused_entities.contains(entity) { return None; }
            let kinds: HashSet<_> = kinds.iter()
                .filter(|k| !self.updater.diff_mask_is_clear(entity, k))
                .copied()
                .collect();
            if kinds.is_empty() { None } else { Some((*entity, kinds)) }
        })
        .collect()
}
```

### Cost After Fix B

| Operation | Before | After |
|---|---|---|
| Entity-level HashMap lookups | 5 × dirty × users | 5 × dirty (once) + 1 × dirty × users |
| DirtySet scan | 1 × users | 1 × users (unchanged — still per-user) |

For 32 dirty entities × 32 users: 5,120 → ~160 entity-level lookups + 1,024 diff-mask checks.

### Interaction with DirtySet

Each user still has its own `DirtySet` (needed for per-user delivery tracking and diff mask management). Fix B does not change that. It only caches the entity-level (non-user-specific) part of the candidate computation.

The `global_dirty` set is computed from the **union of dirty signals** — entities where at least one user has a non-clear diff mask. In practice (public entities like avatars), this is the same set for every user, so the global set is computed correctly.

---

## Implementation Plan

### Phase 1 — BitWriter::append_blob (no-op without callers)

1. Add `BitBlob { words: Vec<u64>, bit_count: usize }` to `naia-shared`
2. Add `BitWriter::append_blob(&self, blob: &BitBlob)` (aligned + unaligned paths)
3. Add `BitWriter::to_blob(&self) -> BitBlob` for pre-serialization
4. **Gate:** unit test round-trip: `append_blob(to_blob(writer)) == original bits` at all alignments (0–63)

### Phase 2 — Precompute blobs in WorldWriter (Fix A)

1. Add `WorldWriter::precompute_update_blobs(...)` — pre-serializes component body for all dirty entities
2. Add `WorldWriter::write_updates_from_blobs(...)` — replaces inner ECS read + Serde::ser with blob lookup + `append_blob`
3. Wire `precompute_update_blobs` into `WorldServer::send_all_packets` before the per-user loop
4. Pass blobs into `connection.send_packets` and down to `write_updates_from_blobs`
5. **Gate:** E2E harness 93/93, bench profile shows `send_packet_loop` reduction

### Phase 3 — Shared dirty candidates (Fix B)

1. Add `WorldServer::compute_global_dirty_candidates(world, converter)` — entity-level filter
2. Replace per-user `take_update_events` with `take_update_events_from_global(global_dirty)`
3. **Gate:** E2E harness 93/93, bench profile shows `take_update_events` reduction

### Phase 4 — Bench re-run and documentation

1. Run `cargo run --features bench_profile -p cyberlith_bench --release -- --scenario game_server_tick --warmup 100 --ticks 500`
2. Record full sub-phase breakdown in `CAPACITY_RESULTS.md`
3. Compare against pre-fix baseline (39.1% + 25.8%)

---

## Correctness Invariants

- **Wire format unchanged:** `append_blob` produces identical bits to `Serde::ser` — the receiver's deserialization path is unaffected
- **Per-user diff mask independence:** blobs are only shared when diff masks are equal. Per-user diff mask management (cancel on delivery, OR on retransmit) is unaffected — only the serialization step is shared, not the tracking state
- **Delivery tracking unchanged:** `EntityUpdateManager::sent_updates` still tracks what was sent per-user per-packet-index for retransmit/drop handling
- **Priority order unchanged:** entity priority sort happens before blob lookup; blob content does not affect ordering
- **Fallback:** any entity/component combination that cannot share a blob (multi-property, mismatched diff masks) falls back to today's per-user serialization

---

## Open Questions

1. **BitWriter internals:** what is the exact storage layout (u64 words vs u8 bytes, endianness)? `append_blob` must match exactly. Verify against existing `BitWriter` before implementing.
2. **`to_blob` starting point:** pre-serialization must start the blob AFTER the LocalEntity write and BEFORE `write_update` is called. The exact call site in `write_updates` needs review to confirm the bit boundary.
3. **Multi-property components with diverged diff masks:** how common is this in practice? If rare (no current components hit this), the fallback path can be a `panic!` guarded by a flag in test mode to catch future regressions.
4. **global_dirty ownership:** `compute_global_dirty_candidates` needs read access to all per-user `EntityUpdateManager`s. Currently those are behind `connection.base.world_manager`. May need a borrow restructure in `send_all_packets`.
