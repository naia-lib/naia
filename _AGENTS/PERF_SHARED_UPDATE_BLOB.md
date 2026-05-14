# PERF — Shared Component Update Blob

**Status:** SUPERSEDED — see `PERF_IRIS_REPLICATION.md`
**Created:** 2026-05-13
**Context:** Sub-phase profiling in cyberlith benchmark — `cyberlith/_AGENTS/CAPACITY_RESULTS.md`
**Touches:** `naia-shared` (serde, world, update), `naia-server` (world_server, connection)

---

## Problem

Sub-phase profiling at 32 players (release profile, `game_server_tick` bench):

| Phase | % of tick | Root cause |
|---|---:|---|
| `send_packet_loop` | **39.1%** | `component.write_update(diff_mask, writer, converter)` × users × dirty entities |
| `take_update_events` | **25.8%** | Entity-level HashMap lookups × users × dirty entities |

Both are **O(dirty\_entities × users)**. With 32 players all moving: 1,024 `write_update` calls/tick and ~5,120 entity-level lookups/tick. Only the per-user `LocalEntity` ID differs. The component data (ComponentKind + per-property bits + ComponentValue) is **identical for every user who has the same diff mask**. We are doing 31/32 of the serialization work redundantly.

---

## Solution Overview

**Fix B** (prerequisite): compute the entity-level lookup results once per tick at the `WorldServer` level; per-user scan handles only user-specific checks.

**Fix A** (depends on Fix B): pre-serialise component bodies into `BitBlob`s. The first user for a given `(entity, component, diff_mask)` combination pays the full ECS-read + `write_update` cost and stores the result in a `BlobCache`. Every subsequent user for the same combination reuses the blob via `append_blob` — no ECS read, no Serde traversal.

**Fix A and Fix B are NOT independent.** Fix B must ship first: Fix A's `BlobCache` is populated lazily during the per-user loop and requires `global_dirty` (Fix B's output) to be plumbed through `send_packets`.

---

## Wire Format — Exact Bit Sequence

Verified against `world_writer.rs:write_updates`, `write_update`, and `local_entity.rs:OwnedLocalEntity::ser`.

For each dirty entity (outer loop in `write_updates`):

```
[reserved]  1 bit — ComponentContinue finish placeholder (reserve_bits(1) mechanism)
[1 bit]     UpdateContinue = true
[var bits]  OwnedLocalEntity::ser — per-user
            is_host(1) + is_static(1) + UnsignedVariableInteger::<7>(id)
            → 9 bits for IDs 0–127 (covers all avatar entities in 32-player bench)
--- BLOB BOUNDARY ---
  For each dirty component (inner loop):
    [1 bit]   ComponentContinue = true
    [var]     ComponentKind::ser(component_kinds, writer) — identical across users
    [var]     component.write_update(&diff_mask, writer, converter) — value bits
              → user-independent for non-EntityProperty components
--- END BLOB ---
[1 bit]     ComponentContinue = false (release_bits(1) then write finish bit)
```

After all entities:
```
[1 bit]   UpdateContinue = false
```

**Blobs are per-component, not per-entity.** One blob = one component body: `ComponentContinue=1 + ComponentKind + ComponentValue`. This preserves the existing partial-entity-send semantics: if component A fits but component B of the same entity doesn't, A is written and B is deferred exactly as today.

**Converter usage:** `write_update` receives `&mut dyn LocalEntityAndGlobalEntityConverterMut`. For components with `EntityProperty` fields it converts global→local entity IDs (per-user). For pure-data components (`NetworkedPosition`, `NetworkedVelocity`) it is never called — confirmed by inspecting `Property<T>::write` and the replicate derive.

---

## New Types and Extensions

### `BitBlob` (naia-shared/serde)

```rust
/// Pre-serialised single-component body: ComponentContinue=1 + ComponentKind + ComponentValue.
///
/// Stored inline — no heap allocation. 64 bytes (512 bits) covers all
/// reasonable single-component bodies. If a component body would exceed
/// this, blob caching is skipped and per-user serialisation is used as fallback.
pub struct BitBlob {
    pub bytes: [u8; 64],
    pub bit_count: usize,
}

impl BitBlob {
    /// Captures a `BitWriter`'s content as a `BitBlob`.
    /// Returns `None` if the written content exceeds 64 bytes.
    pub fn capture(writer: BitWriter) -> Option<Self> {
        let bit_count = writer.bits_written();
        let raw = writer.to_bytes();
        if raw.len() > 64 {
            return None;
        }
        let mut bytes = [0u8; 64];
        bytes[..raw.len()].copy_from_slice(&raw);
        Some(Self { bytes, bit_count })
    }
}
```

### `BitWriter` extensions (naia-shared/serde)

```rust
impl BitWriter {
    /// Returns the number of bits written so far (before `finalize`).
    pub fn bits_written(&self) -> usize {
        self.current_bits as usize
    }

    /// Appends all bits from `blob` at the current write position.
    /// Bit-accurate: zero wasted bits. Cost: O(blob_bytes) `write_byte` calls
    /// plus O(trailing_bits) `write_bit` calls for the partial last byte.
    pub fn append_blob(&mut self, blob: &BitBlob) {
        if blob.bit_count == 0 { return; }
        let full_bytes = blob.bit_count / 8;
        let trailing_bits = (blob.bit_count % 8) as u32;
        for &byte in &blob.bytes[..full_bytes] {
            self.write_byte(byte);
        }
        if trailing_bits > 0 {
            let last = blob.bytes[full_bytes];
            for bit_idx in 0..trailing_bits {
                self.write_bit((last >> bit_idx) & 1 != 0);
            }
        }
    }
}
```

`BitCounter::count_bits(bits: u32)` already exists and is used for the overflow-check pass on cache hits: `counter.count_bits(blob.bit_count as u32)` — O(1), no ECS read.

### `DiffMask::as_key` (naia-shared)

```rust
impl DiffMask {
    /// Packs the mask into a `u64` for use as a hash key.
    /// Covers components with ≤ 8 bytes (64 properties) of diff mask.
    /// Returns `None` for larger masks; callers fall back to per-user serialisation.
    pub fn as_key(&self) -> Option<u64> {
        if self.mask.len() > 8 { return None; }
        let mut key = 0u64;
        for (i, &byte) in self.mask.iter().enumerate() {
            key |= (byte as u64) << (i * 8);
        }
        Some(key)
    }
}
```

All current cyberlith components have a 1-byte diff mask, so `as_key()` always returns `Some`.

### `Replicate::blob_eligible` (naia-shared/derive)

Add an associated function to the `Replicate` trait:

```rust
pub trait Replicate: ... {
    /// Returns `true` if this component type has no `EntityProperty` fields and
    /// its serialised body is user-independent (safe to cache in `BlobCache`).
    ///
    /// The default returns `true`. The derive macro overrides to `false` for any
    /// component that contains one or more `EntityProperty` fields.
    fn blob_eligible() -> bool where Self: Sized { true }
    // ...existing methods...
}
```

The derive macro already tracks `EntityProperty` vs `Property<T>` at codegen time (line 406 in `replicate.rs`). The commented-out `get_has_entity_properties_method` at line 1362 is the prior art; this replaces it. For components with any `EntityProperty` field, the macro emits:

```rust
fn blob_eligible() -> bool { false }
```

### `ComponentKinds::is_blob_eligible` (naia-shared)

Store the eligibility flag at registration time, indexed by `ComponentKind`:

```rust
// In ComponentKinds:
fn add_component<C: Replicate + 'static>(&mut self, ...) {
    // ... existing registration ...
    self.blob_eligible.insert(kind, C::blob_eligible());
}

pub fn is_blob_eligible(&self, kind: ComponentKind) -> bool {
    self.blob_eligible.get(&kind).copied().unwrap_or(false)
}
```

This is an O(1) HashMap lookup per component per write. Detection is static (compile-time) and cannot be silently bypassed by future additions.

### `BlobCache` (naia-shared or naia-server)

```rust
/// Per-tick lazy cache of pre-serialised component bodies.
///
/// Keyed by (GlobalEntity, ComponentKind, diff_mask_as_u64). Populated by
/// the first user that serialises a given combination; all subsequent users
/// reuse it via `append_blob` — no ECS read, no Serde traversal.
///
/// Created fresh in `send_all_packets` each tick. Passed as `&mut BlobCache`
/// through the send call chain. Never shared across ticks.
pub struct BlobCache {
    map: HashMap<(GlobalEntity, ComponentKind, u64), BitBlob>,
}

impl BlobCache {
    pub fn new() -> Self {
        Self { map: HashMap::new() }
    }
}
```

---

## Fix B — Shared Dirty Candidates

### What moves to the global pre-pass

`LocalWorldManager::take_update_events` (called per-user today) has two stages:

**Stage 1** — per-user, cannot share:
- `paused_entities.contains(&entity)` — per-user entity pause state
- `host.is_component_updatable(local_converter, entity, kind)` — depends on per-user local entity state (spawn acknowledgment, auth state)
- `remote.is_component_updatable(...)` — same

**Stage 2** (inside `EntityUpdateManager::take_outgoing_events`) — entity-level facts, identical for all users:
- `global_world_manager.entity_is_replicating(entity)`
- `converter.global_entity_to_entity(entity)` → `world.has_entity(world_entity)`
- `world.has_component_of_kind(world_entity, kind)`
- `diff_handler.diff_mask_is_clear(entity, kind)` — per-user, stays per-user

Fix B lifts Stage 2's entity-level checks into a global pre-pass; the per-user path handles only Stage 1 plus the per-user `diff_mask_is_clear`.

### Global pre-pass (in `WorldServer::send_all_packets`)

```rust
// --- Pre-pass: shared borrows, all released before per-user loop ---
let global_dirty: HashMap<GlobalEntity, HashSet<ComponentKind>> = {
    let mut union: HashMap<GlobalEntity, HashSet<ComponentKind>> = HashMap::new();

    // Union of all per-user dirty candidates (calls build_candidates()
    // once per user — same total invocation count as today).
    // &self.user_connections is a shared borrow; build_dirty_candidates_from_receivers()
    // is &self on LocalWorldManager (interior mutability via Mutex inside DirtySet).
    for addr in &user_addresses {
        let conn = self.user_connections.get(addr).unwrap();
        for (entity, kinds) in conn.base.world_manager.build_dirty_candidates_from_receivers() {
            union.entry(entity).or_default().extend(kinds);
        }
    }

    // Entity-level filter: facts that are identical for all users.
    // Uses world converter from global_entity_map (immutable borrow).
    union.retain(|global_entity, kinds| {
        if !self.global_world_manager.entity_is_replicating(global_entity) {
            return false;
        }
        let Ok(world_entity) = self.global_entity_map.global_entity_to_entity(global_entity)
        else {
            return false;
        };
        if !world.has_entity(&world_entity) {
            return false;
        }
        kinds.retain(|k| world.has_component_of_kind(&world_entity, k));
        !kinds.is_empty()
    });

    union
};
// All shared borrows on self.user_connections dropped here.

// --- Per-user loop: exclusive borrows ---
let mut blob_cache = BlobCache::new();
for addr in user_addresses {
    let connection = self.user_connections.get_mut(&addr).unwrap();
    // ...
    connection.send_packets(
        ...,
        &global_dirty,
        &mut blob_cache,
    );
}
```

**Borrow note:** the pre-pass iterates `user_connections` by `&conn` (shared); the per-user loop re-iterates by `&mut conn` (exclusive). These are sequential scopes — the Rust borrow checker is satisfied. `DirtySet::build_candidates()` (called inside `build_dirty_candidates_from_receivers`) takes the indices list and refeeds dirty entries back; it is safe and idempotent within a tick.

### Per-user replacement for `take_update_events`

Add `LocalWorldManager::take_update_events_from_global`:

```rust
pub fn take_update_events_from_global(
    &mut self,
    global_dirty: &HashMap<GlobalEntity, HashSet<ComponentKind>>,
) -> HashMap<GlobalEntity, HashSet<ComponentKind>> {
    let local_converter = self.entity_map.entity_converter();
    let mut result: HashMap<GlobalEntity, HashSet<ComponentKind>> = HashMap::new();

    for (global_entity, kinds) in global_dirty {
        // Per-user check 1: entity paused for this user
        if self.paused_entities.contains(global_entity) {
            continue;
        }
        for kind in kinds {
            // Per-user check 2: component updatable from this user's perspective
            let updatable = self.host.is_component_updatable(local_converter, global_entity, kind)
                || self.remote.is_component_updatable(local_converter, global_entity, kind);
            if !updatable {
                continue;
            }
            // Per-user check 3: diff mask is non-clear for this user
            if self.updater.diff_mask_is_clear(global_entity, kind) {
                continue;
            }
            result.entry(*global_entity).or_default().insert(*kind);
        }
    }

    result
}
```

This replaces the two-stage `take_update_events` call in `connection.send_packets`. The entity-level checks (`entity_is_replicating`, `has_entity`, `has_component`) no longer run per-user — they ran once in the pre-pass.

### Cost After Fix B

| Operation | Before | After |
|---|---:|---:|
| `entity_is_replicating` lookups | 1 × users × dirty | 1 × dirty (pre-pass) |
| `has_entity` checks | 1 × users × dirty | 1 × dirty (pre-pass) |
| `has_component` checks | 1 × users × dirty × kinds | 1 × dirty × kinds (pre-pass) |
| `diff_mask_is_clear` checks | 1 × users × dirty × kinds | 1 × users × dirty × kinds (unchanged) |
| `DirtySet::build_candidates` calls | 1 × users | 1 × users (unchanged, pre-pass) |

For 32 users × 32 dirty entities: ~5,120 entity-level lookups → ~160 (pre-pass only).

---

## Fix A — Lazy BlobCache Serialisation

### How the cache works

`BlobCache` is created once per tick in `send_all_packets` and threaded as `&mut BlobCache` through the call chain. Inside `write_update`, for each component:

1. Check `component_kinds.is_blob_eligible(component_kind)`. If false (EntityProperty component), use per-user Serde::ser as today — no change.
2. Compute `diff_mask = world_manager.get_diff_mask(global_entity, component_kind)`.
3. Compute `blob_key`: if `diff_mask.as_key()` returns `None` (>8-byte mask), use per-user Serde::ser as fallback.
4. **Cache hit:** use the cached blob — counter pass via `count_bits`, writer pass via `append_blob`.
5. **Cache miss (first user for this key):** serialise into a temp `BitWriter`, capture into `BitBlob`, store in cache, write to main writer via `append_blob`.

### Modified `write_update` logic (pseudocode)

```rust
fn write_update(..., blob_cache: &mut BlobCache) {
    for component_kind in component_kind_set {
        let diff_mask = world_manager.get_diff_mask(global_entity, component_kind);

        // Determine cache key; fall back to per-user path if not eligible
        let cache_key: Option<(GlobalEntity, ComponentKind, u64)> =
            if component_kinds.is_blob_eligible(component_kind) {
                diff_mask.as_key().map(|k| (*global_entity, *component_kind, k))
            } else {
                None
            };

        // --- Counter pass (overflow check) ---
        let mut counter = writer.counter();
        if let Some(ref key) = cache_key {
            if let Some(blob) = blob_cache.map.get(key) {
                counter.count_bits(blob.bit_count as u32); // O(1), no ECS read
            } else {
                // No blob yet — count as if serialising (same as today)
                true.ser(&mut counter);
                component_kind.ser(component_kinds, &mut counter);
                world.component_of_kind(world_entity, component_kind)
                    .unwrap()
                    .write_update(&diff_mask, &mut counter, &mut converter);
            }
        } else {
            // Non-eligible: per-user Serde path (unchanged)
            true.ser(&mut counter);
            component_kind.ser(component_kinds, &mut counter);
            world.component_of_kind(world_entity, component_kind)
                .unwrap()
                .write_update(&diff_mask, &mut counter, &mut converter);
        }
        if counter.overflowed() { break; }

        // --- Writer pass ---
        *has_written = true;

        if let Some(ref key) = cache_key {
            if let Some(blob) = blob_cache.map.get(key) {
                // Cache hit: no ECS read, no Serde, O(blob_bytes) byte copies
                writer.append_blob(blob);
            } else {
                // Cache miss: serialise once into temp writer, capture blob
                let mut temp = BitWriter::with_max_capacity();
                true.ser(&mut temp);
                component_kind.ser(component_kinds, &mut temp);
                world.component_of_kind(world_entity, component_kind)
                    .unwrap()
                    .write_update(&diff_mask, &mut temp, &mut null_converter);
                if let Some(blob) = BitBlob::capture(temp) {
                    writer.append_blob(&blob);
                    blob_cache.map.insert(key.clone(), blob);
                } else {
                    // Blob overflow (>64 bytes) — write directly (no caching)
                    // Note: this path unreachable for all current cyberlith components
                    true.ser(writer);
                    component_kind.ser(component_kinds, writer);
                    world.component_of_kind(world_entity, component_kind)
                        .unwrap()
                        .write_update(&diff_mask, writer, &mut converter);
                }
            }
        } else {
            // Non-eligible: per-user Serde path (unchanged)
            true.ser(writer);
            component_kind.ser(component_kinds, writer);
            world.component_of_kind(world_entity, component_kind)
                .unwrap()
                .write_update(&diff_mask, writer, &mut converter);
        }

        world_manager.record_update(now, packet_index, global_entity, component_kind, diff_mask);
        written_component_kinds.push(*component_kind);
    }
    // ... rest of write_update unchanged ...
}
```

**`null_converter` for cache-miss path:** a ZST that implements `LocalEntityAndGlobalEntityConverterMut` with panicking stubs. It is never called because `is_blob_eligible` guarantees no `EntityProperty` fields reach this branch. If the eligibility check is ever wrong (future derive change), the panic provides an immediate, loud signal rather than silent corruption.

### Call chain threading

`BlobCache` is added as `&mut BlobCache` to:
1. `WorldServer::send_all_packets` — creates it
2. `Connection::send_packets(...)` — receives and forwards
3. `Connection::send_packet(...)` — receives and forwards
4. `WorldWriter::write_into_packet(...)` — receives and forwards
5. `WorldWriter::write_updates(...)` — receives and forwards
6. `WorldWriter::write_update(...)` — uses it

This is 5 additional parameters across the existing call chain. Consider wrapping in a `WriteHints<'_>` struct alongside any future per-tick context (e.g., priority data) to stop the parameter count growing further.

### Cost After Fix A

| Operation | Before | After (N=32 users) |
|---|---:|---|
| ECS reads per dirty entity | 32 | 1 (cache-miss user) |
| `write_update` calls per dirty entity | 32 | 1 (cache-miss user) |
| Counter pass per subsequent user | ECS read + Serde | `count_bits(N)` — O(1) |
| Writer pass per subsequent user | ECS read + Serde | `append_blob` — O(blob_bytes) |
| `record_update` per user | unchanged | unchanged |

For `NetworkedPosition` (~60 bits, ~9 blob bytes): `append_blob` per user = 1 `write_byte` call × 8 + 4 `write_bit` calls. Compare to today: ECS archetype lookup + full property struct traversal + 15+ bitpack operations.

With 32 users: user 1 pays ~100% of today's per-entity cost. Users 2–32 pay ~5% (blob byte copies only). **Average per-user cost: ~8% of today's.** Same asymptotic class as a pre-computation pre-pass, achieved with far less architectural complexity.

---

## Implementation Plan

### Phase 1 — Foundational extensions (no callers yet)

1. `BitWriter::bits_written() -> usize` — expose private `current_bits` field
2. `BitWriter::append_blob(&mut self, blob: &BitBlob)` — aligned + trailing-bit paths
3. `BitBlob { bytes: [u8; 64], bit_count: usize }` + `BitBlob::capture(writer: BitWriter) -> Option<Self>`
4. `DiffMask::as_key() -> Option<u64>`
5. **Gate:** unit tests — `append_blob(capture(writer)) == original bits` at all bit alignments 0–63; test empty blob, aligned blob, all-trailing-bits blob

### Phase 2 — Blob eligibility (derive + ComponentKinds)

1. Add `fn blob_eligible() -> bool where Self: Sized { true }` to the `Replicate` trait
2. Derive macro emits `fn blob_eligible() -> bool { false }` for any component with `≥1` `EntityProperty` field (uncomment / revise the commented-out `get_has_entity_properties_method` at line 1362 in `replicate.rs`)
3. `ComponentKinds::add_component` stores the flag; `ComponentKinds::is_blob_eligible(kind) -> bool`
4. **Gate:** `NetworkedPosition::blob_eligible() == true`; `AssetRef<_>::blob_eligible() == false`

### Phase 3 — Fix B: shared dirty candidates

1. `LocalWorldManager::take_update_events_from_global(&self, global_dirty)` as above
2. `WorldServer::send_all_packets`: pre-pass building `global_dirty` (shared-borrow loop), entity-level filter
3. Thread `&global_dirty` into `connection.send_packets`; replace `take_update_events` call with `take_update_events_from_global`
4. **Gate:** E2E harness 93/93; `take_update_events` sub-phase drops from 25.8%

### Phase 4 — Fix A: BlobCache lazy serialisation

1. `BlobCache` struct + `BlobCache::new()`
2. `NullConverter` ZST implementing `LocalEntityAndGlobalEntityConverterMut` with `unimplemented!()` bodies
3. Modify `WorldWriter::write_update` with the cache-check / lazy-populate logic above
4. Thread `&mut blob_cache` through the 5-function call chain
5. **Gate:** E2E harness 93/93; `send_packet_loop` sub-phase drops from 39.1%

### Phase 5 — Bench re-run and documentation

1. `cargo run --features bench_profile -p cyberlith_bench --release -- --scenario game_server_tick --warmup 100 --ticks 500`
2. Record full sub-phase breakdown in `cyberlith/_AGENTS/CAPACITY_RESULTS.md`
3. Compare against baseline (39.1% + 25.8%)

---

## Correctness Invariants

- **Wire format unchanged:** `append_blob` produces identical bits to direct `Serde::ser` — the receiver's deserialisation path is unaffected.
- **Per-user diff mask independence:** the cache key includes the diff mask. Users with different diff masks (possible if a prior packet was dropped and re-ORed) get different blobs automatically. `record_update` stores the exact diff mask that was serialised, ensuring correct retransmit/drop recovery.
- **Partial entity sends preserved:** blobs are per-component, not per-entity. A 3-component entity where only 2 components fit in the packet still writes 2 this tick and defers 1, exactly as today.
- **`record_update` unchanged:** runs per-user per-component regardless of cache hit/miss. Tracks delivered diff masks for drop recovery. Not on the hot path.
- **EntityProperty components unaffected:** `is_blob_eligible` returning `false` routes the entire component through the existing per-user Serde path — no change to their wire format, delivery tracking, or converter usage.
- **Priority order unchanged:** entity priority sort (Phase B in `send_packets`) happens before `write_update` is called; blob content does not affect ordering.
- **`DirtySet::build_candidates` idempotency:** verified in `mut_channel.rs:230-288` — the method takes the indices list and refeeds dirty entries back. The pre-pass and per-user loop each call it exactly once per user (same total count as today), and the results are stable within a tick.

---

## Open Questions

1. **`WriteHints<'_>` struct:** the `BlobCache` is the first per-tick context object threaded through the write chain. If future work adds more (e.g., priority hints), wrap them in a single `WriteHints<'_>` struct rather than adding another parameter. Decide at Phase 4 implementation time.

2. **`NullConverter` verbosity:** `LocalEntityAndGlobalEntityConverterMut` may require many trait methods. Assess at Phase 4 whether a blanket macro or a single `impl` file suffices, or whether we need `#[allow(unused)]` suppression.

3. **Multi-packet tick flushing:** `send_packets` loops until the packet is full, then starts a new packet. `BlobCache` persists across all packets for all users within a tick — this is correct (the blob content for a component doesn't change mid-tick) but should be verified that `record_update`'s packet-index tracking remains correct across packet boundaries (it should: packet_index is passed per-call).

4. **`BlobCache::new()` allocation:** creates an empty `HashMap` per tick. At 20 ticks/sec this is 20 HashMap allocations/sec with a pre-warmed entry count. Consider `BlobCache::with_capacity(dirty_entity_count * avg_components_per_entity)` once Fix B provides the dirty entity count cheaply.
