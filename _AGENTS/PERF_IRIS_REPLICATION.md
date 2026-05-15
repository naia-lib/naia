# PERF — Naia Iris Replication Architecture

**Status:** COMPLETE — Phases 1–10 implemented and benchmarked (2026-05-14)
**Created:** 2026-05-13
**Supersedes:** `PERF_SHARED_UPDATE_BLOB.md`
**Context:** Sub-phase profiling in cyberlith benchmark — `cyberlith/_AGENTS/CAPACITY_RESULTS.md`
**Scope:** `naia-shared` (serde, world, update, local), `naia-server` (connection, world_server)
**Branch:** `dev` (dev-trunk model — NEVER commit to `main`)

---

## 0. Key Files and Gate Commands

### Files Modified by Phase

| Phase | Files modified / created |
|---|---|
| 1 (renames) | `shared/src/world/update/component_update.rs`, `shared/src/world/world_writer.rs`, `shared/src/world/local/local_world_manager.rs`, `shared/src/world/update/user_diff_handler.rs` |
| 2 (serde) | `shared/serde/src/bit_writer.rs`, `shared/serde/src/lib.rs` (new `CachedComponentUpdate`), `shared/src/world/update/diff_mask.rs` |
| 3 (derive) | `shared/derive/src/replicate.rs` (line 1362 — revive commented impl), `shared/src/world/component/component_kinds.rs` |
| 4 (cache store) | `shared/src/world/update/mut_channel.rs` (trait + `DirtyNotifier`), `server/src/world/mut_channel.rs` (`MutChannelData`) |
| 5 (two-path write) | `shared/src/world/world_writer.rs` (major rewrite of `write_update`) |
| 6 (GlobalEntityIndex) | NEW: `shared/src/world/update/global_entity_index.rs`; `shared/src/world/update/global_diff_handler.rs`; `shared/src/world/update/user_diff_handler.rs`; `shared/src/world/update/mut_channel.rs` |
| 7 (GlobalDirtyBitset) | NEW: `shared/src/world/update/global_dirty_bitset.rs`; `shared/src/world/update/mut_channel.rs`; `server/src/server/server_config.rs`; `server/src/server/world_server.rs` |
| 8 (VisibilityBitset) | NEW: `shared/src/world/update/connection_visibility_bitset.rs`; server connection struct; `server/src/server/world_server.rs` |
| 9 (send loop) | `server/src/server/world_server.rs` (replace `send_all_packets`); `shared/src/world/local/local_world_manager.rs` (remove `take_update_events`); `shared/src/world/update/user_diff_handler.rs` (remove `DirtyQueue`/`DirtySet`); `shared/src/world/update/mut_channel.rs` (remove `DirtySet` from `DirtyNotifier`) |

### Other Key Files (Read Before Editing)

- `shared/src/world/update/global_diff_handler.rs` — `GlobalDiffHandler<E>` (fields to extend in Phases 4, 6, 7)
- `shared/src/world/update/user_diff_handler.rs` — `UserDiffHandler`, `DirtyQueue`, `DirtySet` (removed in Phase 9)
- `server/src/server/world_server.rs` — `WorldServer`, `send_all_packets` (primary orchestration site)
- `shared/src/world/component/replica_ref.rs` — `ReplicaDynRefWrapper` (Deref → `&dyn Replicate`; `copy_to_box()` is accessible via this Deref)
- `shared/src/world/component/replicate.rs` — `Replicate` trait (has `copy_to_box(&self) -> Box<dyn Replicate>` at line 74 — **no new method needed for snapshotting**)

### Gate Commands

Run these from the naia repo root unless noted:

```bash
# After every phase — must be warning-clean:
RUSTFLAGS="-D warnings" cargo check --workspace --all-targets --quiet

# After phases touching shared/serde or shared/derive (Phases 2, 3):
RUSTFLAGS="-D warnings" cargo check -p naia-shared --target wasm32-unknown-unknown --features wbindgen --quiet
RUSTFLAGS="-D warnings" cargo check -p naia-client --target wasm32-unknown-unknown --features wbindgen --quiet
RUSTFLAGS="-D warnings" cargo check -p naia-bevy-client --target wasm32-unknown-unknown --quiet

# Unit + integration tests:
cargo test --workspace

# Namako BDD gate (332 scenarios as of 2026-05-10; count may grow):
cargo run --manifest-path /home/connor/Work/specops/namako/Cargo.toml -p namako-cli -- \
    gate --specs-dir test/specs \
    --adapter-cmd "cargo run --manifest-path test/npa/Cargo.toml --"

# Cyberlith full-stack bench (Phase 10 only — run in cyberlith repo after updating naia dep):
cargo run --features bench_profile -p cyberlith_bench --release -- \
    --scenario game_server_tick --warmup 100 --ticks 500
```

**What "gate green" means in each phase:** `cargo check` warning-clean + `cargo test --workspace` all pass + namako gate passes. The cyberlith bench is only required at Phase 10.

---

## 1. Problem Statement

Sub-phase profiling at 32 players (release profile, `game_server_tick` bench) **before implementation**:

| Phase | % of tick | Root cause |
|---|---:|---|
| `send_packet_loop` | **39.1%** | `component.write_update()` × users × dirty_entities: O(N²) |
| `take_update_events` | **25.8%** | Entity-level HashMap lookups × users × dirty_entities: O(N²) |

At 32 players all moving: **1,024 ECS reads** per tick (2 per component per user — counter pass + writer pass), **~5,120 HashMap lookups** for entity-level facts that are identical for all users, and **1,024 Serde traversals** bitpacking the same component data 32 times.

At **10,000 CCU** these numbers become 320,000 ECS reads and 320,000 serializations per tick — consuming the entire server tick budget before packets even leave the machine.

### Post-Implementation Results (2026-05-14, apples-to-apples on same machine)

| Phase | Before | After | Delta |
|---|---:|---:|---|
| `take_update_events` | 25.8% | **0.0% (ELIMINATED)** | GlobalDirtyBitset + bitset intersection |
| `send_packet_loop` | 39.1% → 31.0% (re-baselined) | **25.6%** | PATH A cached updates −28.6% |
| `cell::update [total]` p99 | 5,137 µs | 4,278 µs | **−16.7%** |
| Server RSS | 24.66 MB | 12.98 MB | **−47.3%** (N_ram 33→63 cells) |

**Remaining bottleneck — `write_updates` = 36.8% of tick (2026-05-14 sub-breakdown):**

Post-Iris profiling with `bench_instrumentation` shows:
- `scope_entry_spawns = 0` in 500-tick steady state — tile serialization hypothesis ruled out
- `write_updates` (entity/component serialization in `WorldWriter`) = 93% of `write_packet`
- `write_packet` = 97% of `send_packet_loop` — io_send (transport) is only 1.1%
- Root: `get_cached_update()` still uses `mut_receiver_builders: HashMap<(GlobalEntity, ComponentKind), ...>`
  (~20–30 ns/lookup × ~1,800 component×user visits/tick)

The `mut_receiver_builders` HashMap → dense 2D array conversion (Innovation 1, specified but not yet
implemented in Phase 6) is the next optimization target. `GlobalEntityIndex` is live; the array
indexing design is fully specified in §4 below.

---

## 2. Guiding Principle — Iris Architecture Translated to Naia

Unreal Engine 5's Iris replication system defines a five-phase pipeline. Translating each phase to naia's existing architecture:

| Iris Phase | What It Does | Naia Equivalent | Gap |
|---|---|---|---|
| **Filtering** | Which objects replicate to which connections | Rooms + `UserScope` + `is_component_updatable` | HashMap-based, not bitset-based |
| **Poll and Copy** | Copy gameplay state into replication system's own memory | **Missing entirely** — naia reads from ECS at send time | This is the O(N²) root cause |
| **Quantization** | Convert floats to packed network format | User-defined Serde impls | Not built-in; future work |
| **Prioritization** | Score and sort objects by importance | Priority accumulator (COMPLETE) | None — already done |
| **Serialization** | Write sorted objects into packet budget | `write_update` per user | Reads ECS per user instead of own storage |

The **Poll and Copy gap** is the entire performance problem. Iris does one read of gameplay state (per dirty component per tick) and stores it in its own network-native representation. All subsequent per-connection work reads from that representation — never from gameplay storage again.

The core lesson: **filter first, then work only on what passed the filter; copy state once, serialize from the copy N times; never touch gameplay storage (ECS) during serialization.**

---

## 3. Architecture Overview

Five coordinated innovations, each targeting a specific O(N²) source:

| Innovation | Eliminates | Replaces |
|---|---|---|
| **GlobalEntityIndex** | HashMap lookups for entity identity | Dense `u32` index → array access |
| **GlobalDirtyBitset** | Per-user dirty candidate scanning | One shared bitset, updated at mutation time |
| **Visibility Bitsets** | Per-user per-entity scope iteration | Bitwise AND of global dirty × per-user visible |
| **MutChannel Cached Update Store** | Redundant ECS reads + re-serialization | Persistent pre-serialized bytes, invalidated at mutation |
| **Two Principled Paths** | Unified but incorrect treatment of entity-reference vs pure-data components | `UserIndependent` (cached update) + `UserDependent` (snapshot) |

The new `send_all_packets` loop becomes three phases:

```
Phase 1 — Build global dirty candidate set      O(dirty_entities / 64)
Phase 2 — Entity filter + Poll-and-Copy         O(dirty_entities × avg_dirty_components)
Phase 3 — Per-user send                         O(users × avg_dirty_visible × blob_bytes)
```

---

## 4. Innovation 1: GlobalEntityIndex — Dense Global Entity Handle

### Problem

`GlobalEntity(u64)` is used as a `HashMap` key in every hot path:
- `GlobalDiffHandler::mut_receiver_builders: HashMap<(GlobalEntity, ComponentKind), MutReceiverBuilder>`
- `UserDiffHandler::receivers: HashMap<(GlobalEntity, ComponentKind), MutReceiver>`
- `EntityUpdateManager::sent_updates: HashMap<PacketIndex, (Instant, HashMap<(GlobalEntity, ComponentKind), DiffMask>)>`
- The global dirty candidate union in `send_all_packets`

HashMap operations are ~10–30ns per lookup due to hashing + pointer chasing. Array indexing by `u32` is ~1–3ns (direct memory address computation).

`UserDiffHandler` already has the right idea — it allocates a per-user `LocalEntityIndex` (u32) for each replicated entity via `entity_to_index: HashMap<GlobalEntity, LocalEntityIndex>` and `index_to_entity: Vec<Option<GlobalEntity>>`. The existing per-user `EntityIndex` type alias is renamed `LocalEntityIndex` throughout to make the per-user vs. global distinction explicit. The innovation is making this index **global** (shared across all users) via `GlobalEntityIndex`, rather than per-user.

### Solution

```rust
/// Dense index for a server-replicated entity. 0 is reserved (invalid sentinel).
/// Shared across all connections — the same entity has the same index for every user.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct GlobalEntityIndex(pub u32);

impl GlobalEntityIndex {
    pub const INVALID: Self = Self(0);
    pub fn is_valid(self) -> bool { self.0 != 0 }
    pub fn as_usize(self) -> usize { self.0 as usize }
}
```

### GlobalDiffHandler Extended with Dense Entity Tables

The existing `GlobalDiffHandler` (lives in `global_diff_handler.rs`, currently holds `mut_receiver_builders`, `kind_bits`, `max_kind_count`) is extended to also own the dense entity registry. This co-location is natural: `GlobalDiffHandler` already knows every `(GlobalEntity, ComponentKind)` pair that is registered for replication, and the dense index is needed to drive the `GlobalDirtyBitset` (Innovation 2).

New fields added to `GlobalDiffHandler`:

```rust
pub struct GlobalDiffHandler<E: Copy> {
    // Existing fields:
    mut_receiver_builders: HashMap<(GlobalEntity, ComponentKind), MutReceiverBuilder>,
    kind_bits: HashMap<ComponentKind, u16>,
    max_kind_count: u16,

    // NEW — dense entity registry:
    /// GlobalEntity → GlobalEntityIndex
    global_to_idx: HashMap<GlobalEntity, GlobalEntityIndex>,
    /// Dense arrays indexed by GlobalEntityIndex (index 0 = unused sentinel)
    idx_to_global: Vec<Option<GlobalEntity>>,
    idx_to_world:  Vec<Option<E>>,
    /// Per-entity component metadata — one bit per registered ComponentKind.
    idx_to_components: Vec<ComponentFlags>,
    /// Free list for index recycling on entity despawn.
    free_list: Vec<GlobalEntityIndex>,
    next_idx: u32,

    // NEW — inverse kind-bit lookup (hot-path, O(1) array):
    /// Indexed by kind_bit (== NetId). Populated at register_component time.
    /// Enables `kind_for_bit(kind_bit) -> ComponentKind` without a HashMap lookup.
    bit_to_kind: Vec<Option<ComponentKind>>,
}

/// Per-entity component metadata. Packed bits — one bit per registered ComponentKind.
pub struct ComponentFlags {
    /// Which component kinds are currently registered for this entity.
    registered: BitVec,
    /// Which registered component kinds have EntityProperty fields.
    /// Set from ComponentKinds::has_entity_properties() at registration time.
    user_dependent: BitVec,
}
```

**New operations on `GlobalDiffHandler`:**
- `alloc_entity(global: GlobalEntity, world: E) -> GlobalEntityIndex` — O(1) amortized (free list or bump)
- `free_entity(idx: GlobalEntityIndex)` — O(1)
- `global_to_idx(global: &GlobalEntity) -> Option<GlobalEntityIndex>` — O(1) HashMap
- `world_entity(idx: GlobalEntityIndex) -> E` — O(1) array
- `global_entity(idx: GlobalEntityIndex) -> GlobalEntity` — O(1) array
- `kind_for_bit(kind_bit: u16) -> ComponentKind` — O(1) array; inverse of `kind_bits` map (see below)
- `register_component(idx, kind, is_user_dependent)` — sets bit in `idx_to_components`
- `deregister_component(idx, kind)` — clears bit
- `get_cached_update(entity, kind, key) -> Option<CachedComponentUpdate>` — cached update accessor (see Innovation 4)
- `set_cached_update(entity, kind, key, update: CachedComponentUpdate)` — cached update write

**`kind_bit` ↔ `ComponentKind` mapping:** `GlobalDiffHandler::kind_bits: HashMap<ComponentKind, u16>` stores the NetId (the same u16 used on the wire) as the bit-index for the dirty bitset. These are the same value — `kind_bit` IS the NetId. The existing `ComponentKinds::net_id_map: HashMap<NetId, ComponentKind>` provides the inverse lookup but is currently accessed only through the private `net_id_to_kind`. Two additions are needed:

1. Expose a public method on `ComponentKinds`: `pub fn kind_for_net_id(&self, net_id: u16) -> Option<ComponentKind>` — a direct lookup into the existing `net_id_map`.
2. Add `bit_to_kind: Vec<ComponentKind>` to `GlobalDiffHandler` (indexed by kind_bit) for O(1) hot-path lookup without going through `ComponentKinds`. Populated when a component is registered via `register_component`. The send loop uses `self.global_diff_handler.kind_for_bit(kind_bit)` — one array access, no HashMap.

**Migration from `UserDiffHandler::LocalEntityIndex`:** The per-user `entity_to_index` / `index_to_entity` tables in `UserDiffHandler` become unnecessary. Per-user components that previously used `LocalEntityIndex` as a row key in `DirtyQueue` switch to `GlobalEntityIndex`. Since the global registry assigns one index per entity regardless of scope, per-user visibility is tracked separately (Innovation 3).

---

## 5. Innovation 2: GlobalDirtyBitset — Centralized Mutation Tracking

### Problem

Currently every `MutSender::mutate(property_index)` call fans out only to per-user `MutReceiver` masks via `MutChannel::send()`. There is no server-level signal of "which entities have ANYTHING dirty for ANY user." Computing this requires iterating all users' `DirtyQueue`s and building a union — O(users × dirty_entities) per tick.

### Solution

A single server-level bitset tracking which `(GlobalEntityIndex, ComponentKind)` pairs have pending mutations for any user. Maintained atomically at mutation time via the existing `DirtyNotifier` infrastructure.

```rust
/// Server-global dirty tracking matrix.
///
/// Three layers:
///   ref_counts:        per (entity, kind) — count of users with non-clear DiffMask
///   dirty_components:  per (entity, kind) — summary bit: ref_count > 0 ↔ bit set
///   dirty_entities:    per entity         — summary bit: any dirty_component bit set
///
/// Layout for dirty_components:
///   word index  = entity_idx * component_stride + kind_bit / 64
///   bit  index  = kind_bit % 64
///
/// component_stride = (max_kind_count + 63) / 64  (1 for ≤64 component kinds)
pub struct GlobalDirtyBitset {
    ref_counts:        Vec<AtomicU32>,   // [entity_idx * component_count + kind_bit]
    component_count:   usize,
    dirty_components:  Vec<AtomicU64>,   // [entity_idx * component_stride + kind_bit/64]
    component_stride:  usize,
    dirty_entities:    Vec<AtomicU64>,   // [entity_idx / 64], bit = entity_idx % 64
    capacity:          usize,
}
```

**Operations:**

```rust
impl GlobalDirtyBitset {
    /// Called from DirtyNotifier::notify_dirty() — user's (entity, kind) goes clean→dirty.
    /// Increments ref-count; on 0→1 transition sets dirty_components bit and,
    /// if entity_components word was zero, sets dirty_entities bit.
    pub fn increment(&self, entity_idx: GlobalEntityIndex, kind_bit: u16);

    /// Called from DirtyNotifier::notify_clean() — user's (entity, kind) goes dirty→clean.
    /// Decrements ref-count; on 1→0 transition clears dirty_components bit and,
    /// if entity_components word becomes zero, clears dirty_entities bit.
    pub fn decrement(&self, entity_idx: GlobalEntityIndex, kind_bit: u16);

    /// Returns true if this (entity, kind) is dirty for any user. O(1).
    pub fn is_component_dirty(&self, entity_idx: GlobalEntityIndex, kind_bit: u16) -> bool {
        let rc_idx = entity_idx.as_usize() * self.component_count + kind_bit as usize;
        self.ref_counts[rc_idx].load(Ordering::Relaxed) > 0
    }

    /// Iterates entities with any dirty component. O(capacity / 64).
    pub fn dirty_entity_iter(&self) -> impl Iterator<Item = GlobalEntityIndex>;

    /// Returns the component-level dirty words for one entity.
    /// Slice length = component_stride. Bit kind_bit%64 in word kind_bit/64 is set
    /// iff this component is dirty for at least one user.
    pub fn dirty_words(&self, entity_idx: GlobalEntityIndex) -> &[AtomicU64] {
        let start = entity_idx.as_usize() * self.component_stride;
        &self.dirty_components[start..start + self.component_stride]
    }
}
```

**Capacity:** Pre-allocated at server startup from `ServerConfig::max_replicated_entities` (default 65,536). Memory cost at 65,536 entities × 16 component kinds: dirty_entities ~8KB, dirty_components ~8KB, ref_counts ~4MB — acceptable at startup, zero runtime allocation. If entities are spawned beyond initial capacity, `GlobalDiffHandler::alloc_entity` panics with a clear message; the operator must increase the config.

**Wire-up to `DirtyNotifier`:**

**Current form** (in `shared/src/world/update/mut_channel.rs:305`):
```rust
pub struct DirtyNotifier {
    entity_idx: EntityIndex,   // per-user LocalEntityIndex (Phase 6 changes to GlobalEntityIndex)
    kind_bit:   u16,
    set:        Weak<DirtySet>, // Phase 9 removes this field entirely
}
```

Phase 6 changes `entity_idx` to `GlobalEntityIndex`. Phase 7 adds `global: Weak<GlobalDirtyBitset>` and Phase 9 removes `set` entirely (the per-user `DirtyQueue` is eliminated — see Section 11 and Phase 9 note):

```rust
// Final form after Phase 9:
pub struct DirtyNotifier {
    entity_idx: GlobalEntityIndex,
    kind_bit:   u16,
    global:     Weak<GlobalDirtyBitset>,
}

impl DirtyNotifier {
    fn notify_dirty(&self) {
        if let Some(g) = self.global.upgrade() { g.increment(self.entity_idx, self.kind_bit); }
    }
    fn notify_clean(&self) {
        if let Some(g) = self.global.upgrade() { g.decrement(self.entity_idx, self.kind_bit); }
    }
}
```

The `GlobalDirtyBitset` is owned by `WorldServer` and shared via `Arc`. It is populated automatically as mutations arrive, with zero per-tick overhead beyond the atomic operations already needed for per-user dirty tracking.

---

## 6. Innovation 3: Per-Connection Visibility Bitsets

### Problem

Per-user dirty candidates today: `build_dirty_candidates_from_receivers()` walks the per-user `DirtyQueue`, finds entities with dirty bits, then `take_outgoing_events` applies entity-level filters. Per-user scope checks (`paused_entities`, `is_component_updatable`) are applied one entity at a time. No global pre-filtering.

At 10,000 CCU with 10,000 visible entities per user: these HashMap-based iterations dominate.

### Solution

Each connection maintains a `ConnectionVisibilityBitset` — one bit per `GlobalEntityIndex`. Set when an entity enters scope for this user, cleared when it leaves. Sized to match `GlobalDirtyBitset` capacity (same `max_replicated_entities`).

```rust
pub struct ConnectionVisibilityBitset {
    visible:  Vec<u64>,  // one bit per GlobalEntityIndex; word = idx/64, bit = idx%64
    capacity: usize,
}

impl ConnectionVisibilityBitset {
    pub fn set(&mut self, idx: GlobalEntityIndex);
    pub fn clear(&mut self, idx: GlobalEntityIndex);
    pub fn is_set(&self, idx: GlobalEntityIndex) -> bool;

    /// Bitwise AND with global dirty entity summary.
    /// Returns iterator over indices that are both visible and globally dirty.
    /// O(capacity / 64) — the hot path for per-user candidate selection.
    pub fn intersect_dirty<'a>(
        &'a self,
        global_dirty: &'a GlobalDirtyBitset,
    ) -> impl Iterator<Item = GlobalEntityIndex> + 'a;
}
```

`intersect_dirty` implementation:

```rust
fn intersect_dirty(&self, global: &GlobalDirtyBitset) -> impl Iterator<Item = GlobalEntityIndex> {
    self.visible.iter()
        .zip(global.dirty_entities.iter())
        .enumerate()
        .flat_map(|(word_idx, (vis_word, dirty_word))| {
            let combined = vis_word & dirty_word.load(Ordering::Relaxed);
            BitIterator::new(combined, word_idx * 64)
        })
}
```

**Maintenance:** Wire `set`/`clear` into the existing scope enter/exit callbacks in `update_entity_scopes`. Per-connection pause state (`paused_entities`) is folded into the visibility bit — pausing an entity clears its bit, unpausing sets it.

**Auth-level component filtering:** `is_component_updatable` is a per-component per-user check (auth state, spawn acknowledgment). It is NOT folded into the entity-level visibility bit — it remains a per-component guard inside the Phase 3 per-user loop. Its cost is negligible compared to the O(N²) scan it replaces.

---

## 7. Innovation 4: MutChannel Cached Update Store

### Problem

`component.write_update(&diff_mask, writer, converter)` is called **twice per component per user** (counter pass + writer pass), each requiring an ECS archetype lookup. 32 users × 32 entities × 2 passes = **2,048 ECS reads per tick** in the benchmark.

Iris's "Poll and Copy" solution: maintain pre-serialized bytes for each component in the replication system's own storage. Read from those bytes at send time — zero ECS access.

### Where the Cache Lives

`MutChannel` is the natural home. It is:
- Already per `(GlobalEntity, ComponentKind)` — the exact granularity needed
- Already notified on every property mutation (via `MutChannelType::send()`)
- Already shared across all connections (via `Arc<RwLock<dyn MutChannelType>>`)

### MutChannelType Trait Extension

Add three methods to the `MutChannelType` trait (all take `&self` — interior mutability required):

```rust
pub trait MutChannelType: Send + Sync {
    // existing:
    fn new_receiver(&mut self, address: &Option<SocketAddr>) -> Option<MutReceiver>;
    fn send(&self, diff: u8);

    // NEW — cached update store:

    /// Returns the cached pre-serialized update for the given diff mask key, if valid.
    /// Returns None if the cache has been invalidated (component mutated since last build).
    fn get_cached_update(&self, diff_mask_key: u64) -> Option<CachedComponentUpdate>;

    /// Stores a newly-built cached update for the given diff mask key.
    /// Multiple keys can coexist — different users can have different diff masks
    /// after dropped-packet recovery adds extra bits.
    fn set_cached_update(&self, diff_mask_key: u64, update: CachedComponentUpdate);

    /// Clears ALL cached updates. Called automatically from send() on every mutation.
    fn clear_cached_updates(&self);
}
```

### Concrete `MutChannelData` Implementation

The server-side `MutChannelData` struct (concrete impl of `MutChannelType`, lives in `server/src/world/mut_channel.rs`) gains:

```rust
struct MutChannelData {
    receivers: Vec<(Option<SocketAddr>, Arc<AtomicDiffMask>)>,  // existing
    cached_updates: RwLock<HashMap<u64, CachedComponentUpdate>>,  // NEW
}

impl MutChannelType for MutChannelData {
    fn send(&self, diff: u8) {
        for (_, mask) in &self.receivers { mask.mutate(diff); }
        self.cached_updates.write().clear();   // invalidate on any mutation
    }
    fn get_cached_update(&self, key: u64) -> Option<CachedComponentUpdate> {
        self.cached_updates.read().get(&key).copied()
    }
    fn set_cached_update(&self, key: u64, update: CachedComponentUpdate) {
        self.cached_updates.write().insert(key, update);
    }
    fn clear_cached_updates(&self) {
        self.cached_updates.write().clear();
    }
}
```

**Lock note:** `MutChannel::send()` acquires a READ lock on the outer `Arc<RwLock<dyn MutChannelType>>` (to call `data.send()`). Inside `MutChannelData::send`, `clear_cached_updates` acquires a WRITE lock on the separate inner `cached_updates: RwLock<…>`. These are two distinct locks — no deadlock risk.

### `MutChannel` Public Wrapper Methods

`MutChannel` (the public struct wrapping `Arc<RwLock<dyn MutChannelType>>`) adds matching methods that delegate through the outer READ lock (consistent with how `send()` already works):

```rust
impl MutChannel {
    pub fn get_cached_update(&self, key: u64) -> Option<CachedComponentUpdate> {
        self.data.read().ok()?.get_cached_update(key)
    }
    pub fn set_cached_update(&self, key: u64, update: CachedComponentUpdate) {
        if let Ok(data) = self.data.read() {
            data.set_cached_update(key, update);
        }
    }
    // clear_cached_updates is called internally via send() — no public exposure needed.
}
```

### `MutReceiverBuilder` Channel Accessor

`MutReceiverBuilder` already holds `channel: MutChannel` (private field). Add:

```rust
impl MutReceiverBuilder {
    pub fn channel(&self) -> &MutChannel { &self.channel }
}
```

### Cache Access in `GlobalDiffHandler`

```rust
impl<E: Copy> GlobalDiffHandler<E> {
    pub fn get_cached_update(
        &self, entity: &GlobalEntity, kind: &ComponentKind, key: u64,
    ) -> Option<CachedComponentUpdate> {
        self.mut_receiver_builders
            .get(&(*entity, *kind))
            .and_then(|b| b.channel().get_cached_update(key))
    }
    pub fn set_cached_update(
        &self, entity: &GlobalEntity, kind: &ComponentKind, key: u64,
        update: CachedComponentUpdate,
    ) {
        if let Some(b) = self.mut_receiver_builders.get(&(*entity, *kind)) {
            b.channel().set_cached_update(key, update);
        }
    }
}
```

**Cache lifecycle:**
- **Invalidated:** automatically on every `MutChannel::send()` — the instant a property is mutated
- **Built:** lazily on first access after invalidation, by the first user that needs this `diff_mask_key`
- **Reused:** by all subsequent users with the same `diff_mask_key` within the same tick, and across future ticks until the next mutation
- **Cross-tick persistence:** a stable component pays one serialization on the first post-mutation send, then **zero** for all subsequent ticks — this is the full Iris benefit

**Why lazy (not eager in Phase 2):** PATH A caches are per `diff_mask_key`, and the diff mask is per-user (users can have different masks after dropped-packet recovery). Phase 2 runs before per-user visibility intersection, so we don't yet know which users will need which components or what their diff masks will be. Eager pre-building would require iterating all users × dirty components, recreating the O(N²) problem. Lazy first-user-wins gives O(1) cache build cost amortized across all users in steady state (no drops).

---

## 8. Innovation 5: Two Principled Serialization Paths

### The Fundamental Distinction

Naia uses per-connection **LocalEntity** wire IDs for entity references — a deliberate design for privacy and scope semantics. Components with `EntityProperty` fields serialize different bytes per user (because the referenced entity's local wire ID differs per connection). Components with only `Property<T>` fields serialize identical bytes for all users.

This distinction is **type-level and semantic**, not an optimization caveat. It maps directly to Unreal Iris's distinction between `FReplicationFragment` (stateless bytes) and `FObjectReplicationFragment` (per-connection resolution). Both are principled, designed code paths:

- **Path A — UserIndependent**: component bytes are identical for all users with the same `DiffMask`. `CachedComponentUpdate` is shared across all users and ticks. ECS is read at most once per mutation cycle.
- **Path B — UserDependent**: component bytes contain per-user local entity IDs. `CachedComponentUpdate` cannot be shared. ECS read once per tick (snapshot in Phase 2), serialized once per user in Phase 3.

### Compile-Time Detection

Add to the `Replicate` trait:

```rust
pub trait Replicate: Sync + Send + 'static + Named + Any {
    // ... existing methods ...

    /// True if this component contains one or more `EntityProperty` fields,
    /// meaning its serialized bytes differ per connection and cannot be cached
    /// in a shared CachedComponentUpdate. Default: false.
    /// The derive macro overrides to true for any component with ≥1 EntityProperty field.
    fn has_entity_properties() -> bool where Self: Sized { false }

    /// Upper bound on this component's serialized bit length (all fields dirty).
    /// Generated as a compile-time constant by the derive macro.
    /// Used by ComponentKinds::add_component to enforce the 512-bit ceiling
    /// on CachedComponentUpdate storage at registration time.
    fn max_bit_length() -> u32 where Self: Sized;
}
```

The derive macro at `shared/derive/src/replicate.rs` already distinguishes `EntityProperty` from `Property<T>` at codegen time. The commented-out `get_has_entity_properties_method` at line 1362 is prior art — revive and expose as `has_entity_properties()`. `max_bit_length()` is a new compile-time const emitted by the derive macro summing each field's maximum bit width.

### ComponentKinds Storage

```rust
pub struct ComponentKinds {
    current_net_id: NetId,
    kind_bit_width: u8,
    kind_map: HashMap<ComponentKind, (NetId, Box<dyn ReplicateBuilder>, String)>,
    net_id_map: HashMap<NetId, ComponentKind>,
    user_dependent: HashSet<ComponentKind>,  // NEW: components where has_entity_properties() == true
}

impl ComponentKinds {
    pub fn add_component<C: Replicate>(&mut self) {
        // ... existing registration ...
        assert!(
            C::max_bit_length() <= 512,
            "Component {} serializes to {} bits, exceeding the 512-bit \
             CachedComponentUpdate ceiling. Slim the component before registering.",
            std::any::type_name::<C>(), C::max_bit_length()
        );
        if C::has_entity_properties() {
            self.user_dependent.insert(ComponentKind::of::<C>());
        }
    }
    pub fn is_user_dependent(&self, kind: &ComponentKind) -> bool {
        self.user_dependent.contains(kind)
    }
}
```

Registration-time panic is the correct policy: it forces explicit component slimming before the issue manifests at runtime, gives a clear error message, and eliminates the unreachable runtime branch in PATH A's cache-miss path.

---

## 9. New Serde Types

### 9.1 `CachedComponentUpdate`

Pre-serialized single-component body: `ComponentContinue=1 + ComponentKind + ComponentValue`.
Stored inline — no heap allocation. Persists in `MutChannel` across ticks until the component is mutated.

The name reflects its role: a **component update** pre-serialized and **cached** for repeated wire transmission without re-reading ECS or re-running Serde. Compare with `PendingComponentUpdate` — the deserialized form of an *incoming* component update, transient, awaiting application to the live component — which lives on the receive path.

```rust
/// Pre-serialized component body. Inline array, zero heap allocation.
/// 64 bytes = 512 bits. All registered components must fit within this limit
/// (enforced at ComponentKinds::add_component time via Replicate::max_bit_length()).
#[derive(Copy, Clone)]
pub struct CachedComponentUpdate {
    pub bytes:     [u8; 64],
    pub bit_count: u32,
}

impl CachedComponentUpdate {
    /// Captures a BitWriter's current content into a CachedComponentUpdate.
    /// Must be called before finalize() — captures both flushed bytes and
    /// pending scratch register bits.
    /// Returns None if total bit_count > 512 (guaranteed not to happen when
    /// max_bit_length() is enforced at registration time).
    pub fn capture(writer: &BitWriter) -> Option<Self> {
        let bit_count = writer.bits_written() as u32;
        if bit_count > 512 { return None; }

        let flushed = writer.bytes_written_slice();  // complete 32-bit words only
        let (scratch, scratch_bits) = writer.scratch_bits_pending();  // 0–31 pending bits

        let mut bytes = [0u8; 64];
        bytes[..flushed.len()].copy_from_slice(flushed);

        // Copy pending scratch bits as little-endian bytes (mirrors finalize() logic)
        if scratch_bits > 0 {
            let scratch_bytes = scratch.to_le_bytes();
            let n = (scratch_bits as usize).div_ceil(8);
            bytes[flushed.len()..flushed.len() + n].copy_from_slice(&scratch_bytes[..n]);
        }

        Some(Self { bytes, bit_count })
    }
}
```

### 9.2 `PendingComponentUpdate` — Receive Path Counterpart

On the receive path, an incoming component update is deserialized from the wire into a `PendingComponentUpdate` before being applied to the live component. This type already exists as `ComponentUpdate` in `shared/src/world/update/component_update.rs`; it is renamed `PendingComponentUpdate` throughout to make the send/receive duality explicit:

- **`CachedComponentUpdate`** — send path, pre-serialized, cached, reused across users/ticks
- **`PendingComponentUpdate`** — receive path, deserialized from wire, transient, applied once

All usages of `ComponentUpdate` in `component_apply_update`, `component_apply_field_update`, and `WorldMutType` trait signatures are updated to `PendingComponentUpdate`.

### 9.3 `BitWriter` Extensions

`BitWriter` uses a `u32` scratch register (`scratch: u32`, `scratch_bits: u32`), LSB-first. Bits accumulate into `scratch`; when `scratch_bits == 32`, `flush_word()` writes all 4 bytes to `buffer[byte_count..byte_count+4]` and `byte_count += 4`. `finalize()` writes the remaining 0–31 pending scratch bits as 0–4 bytes. Add:

```rust
impl BitWriter {
    /// Total bits written so far (flushed words + scratch register).
    /// Exposes the private `current_bits` field.
    pub fn bits_written(&self) -> u32 {
        self.current_bits
    }

    /// Slice of fully-flushed bytes (complete 32-bit words only).
    /// Does NOT include bits still in the scratch register.
    /// Use scratch_bits_pending() to get those.
    pub fn bytes_written_slice(&self) -> &[u8] {
        &self.buffer[..self.byte_count]
    }

    /// Returns (scratch_value, scratch_bit_count) — bits not yet flushed to buffer.
    /// scratch_bit_count is in [0, 31]. scratch_value holds scratch_bit_count valid
    /// LSB-first bits; upper bits are zero.
    pub fn scratch_bits_pending(&self) -> (u32, u32) {
        (self.scratch, self.scratch_bits)
    }

    /// Appends all bits from a CachedComponentUpdate at the current write position.
    /// Bit-accurate at any alignment: handles arbitrary destination stream alignment.
    /// Uses write_byte for full bytes and write_bit for the partial trailing byte —
    /// produces bit-identical output to re-serializing the component.
    pub fn append_cached_update(&mut self, update: &CachedComponentUpdate) {
        if update.bit_count == 0 { return; }
        let full_bytes = (update.bit_count / 8) as usize;
        let trailing   =  update.bit_count % 8;
        for &byte in &update.bytes[..full_bytes] {
            self.write_byte(byte);
        }
        if trailing > 0 {
            let last = update.bytes[full_bytes];
            for bit in 0..trailing {
                self.write_bit((last >> bit) & 1 != 0);
            }
        }
    }
}
```

`BitCounter::count_bits(bits: u32)` already exists — use for O(1) overflow check on cache hit:
`counter.count_bits(cached_update.bit_count)`.

### 9.4 `DiffMask::as_key`

```rust
impl DiffMask {
    /// Packs the mask into a u64 for use as a HashMap key in the cached update store.
    /// Supports masks up to 8 bytes (64 properties). All current cyberlith components
    /// have 1-byte masks. Returns None for masks > 8 bytes; callers fall back to
    /// per-user serialization without caching (this path is unreachable for all
    /// registered cyberlith components).
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

---

## 10. Wire Format Reference

Verified against `world_writer.rs:write_updates` and `write_update`, `local_entity.rs:OwnedLocalEntity::ser`.

For each dirty entity (outer loop in `write_updates`):

```
[reserved]  1 bit  — ComponentContinue finish placeholder (reserve_bits(1) mechanism)
[1 bit]     UpdateContinue = true
[var bits]  OwnedLocalEntity::ser — written directly to stream, NOT part of CachedComponentUpdate
            is_host(1) + is_static(1) + UnsignedVariableInteger::<7>(id)
            = 9 bits for IDs 0–127 (covers all avatar entities in 32-player bench)

For each dirty component on this entity:
    ┌─ CachedComponentUpdate boundary (PATH A) — or direct write (PATH B) ──┐
    │  [1 bit]   ComponentContinue = true                                    │
    │  [var]     ComponentKind::ser(component_kinds, writer)                 │
    │  [var]     component.write_update(&diff_mask, writer, converter)       │
    └────────────────────────────────────────────────────────────────────────┘

[1 bit]     ComponentContinue = false (release_bits(1) then finish bit)
```

After all entities: `[1 bit] UpdateContinue = false`.

**Cache boundaries are per-component, not per-entity.** This preserves the existing partial-entity-send semantics (a 3-component entity where only 2 fit in the packet writes 2 and defers 1). PATH A entities use `append_cached_update` to emit the pre-captured bytes. PATH B entities use the existing two-pass (counter + writer) path. The wire format is identical in both cases — the receiver is unaffected.

---

## 11. The New Send Loop — Three Phases

### Phase 1: Build Global Dirty Entity Set

O(capacity / 64) — a scan of `GlobalDirtyBitset::dirty_entities`.

```rust
// In WorldServer::send_all_packets
let dirty_entity_iter: impl Iterator<Item = GlobalEntityIndex> =
    self.global_dirty.dirty_entity_iter();
```

No allocation. Reads only `GlobalDirtyBitset` (maintained automatically at mutation time).

### Phase 2: Entity Filter + Poll and Copy

For each dirty entity, check global facts once and snapshot UserDependent components. Uses `GlobalDirtyBitset::dirty_words` to iterate only dirty components (not all registered) — O(dirty_components) not O(all_registered_components).

```rust
// Shared borrows — all released before Phase 3's exclusive per-user loop
let mut snapshot_map: SnapshotMap = HashMap::new();

for global_idx in dirty_entity_iter {
    let global_entity = self.global_diff_handler.global_entity(global_idx);
    let world_entity  = self.global_diff_handler.world_entity(global_idx);
    let comp_flags    = self.global_diff_handler.idx_to_components(global_idx);

    // Entity-level facts checked once — not per-user:
    if !self.global_world_manager.entity_is_replicating(&global_entity) { continue; }
    if !world.has_entity(&world_entity) { continue; }

    // Iterate only components that are actually dirty for some user:
    for (word_idx, dirty_word) in self.global_dirty.dirty_words(global_idx).iter().enumerate() {
        let mut word = dirty_word.load(Ordering::Relaxed);
        while word != 0 {
            let bit_pos  = word.trailing_zeros() as usize;
            word        &= word - 1;  // clear lowest set bit
            let kind_bit = (word_idx * 64 + bit_pos) as u16;
            let component_kind = self.global_diff_handler.kind_for_bit(kind_bit);
            if !world.has_component_of_kind(&world_entity, &component_kind) { continue; }

            if comp_flags.user_dependent.get(kind_bit as usize).unwrap_or(false) {
                // PATH B: UserDependent — snapshot ECS once; per-user serialize with converter
                let snap = world.component_of_kind(&world_entity, &component_kind)
                    .expect("component exists (verified above)")
                    .copy_to_box();
                snapshot_map.insert((global_entity, component_kind), snap);
            }
            // PATH A: UserIndependent — CachedComponentUpdate already in MutChannel if
            // component is stable. Cache miss handled lazily in write_update for the first
            // user that needs it (cannot pre-build eagerly: diff mask is per-user).
        }
    }
}
// All shared borrows on world, global_world_manager, global_diff_handler released here.
```

### Phase 3: Per-User Send Loop

```rust
// Exclusive per-user borrows — sequential, safe after Phase 2 shared borrows dropped
for addr in &user_addresses {
    let connection = self.user_connections.get_mut(addr).unwrap();

    // Bitset intersection: O(capacity / 64) — replaces HashMap dirty candidate union
    let user_dirty_iter = connection.visibility.intersect_dirty(&self.global_dirty);

    // Build per-user update_events from bitset intersection + per-user checks
    let mut update_events: HashMap<GlobalEntity, HashSet<ComponentKind>> = HashMap::new();
    for global_idx in user_dirty_iter {
        let global_entity = self.global_diff_handler.global_entity(global_idx);
        let comp_flags    = self.global_diff_handler.idx_to_components(global_idx);

        for (word_idx, dirty_word) in self.global_dirty.dirty_words(global_idx).iter().enumerate() {
            let mut word = dirty_word.load(Ordering::Relaxed);
            while word != 0 {
                let bit_pos  = word.trailing_zeros() as usize;
                word        &= word - 1;
                let kind_bit = (word_idx * 64 + bit_pos) as u16;
                let component_kind = self.global_diff_handler.kind_for_bit(kind_bit);
                let local_converter = connection.base.world_manager.entity_converter();

                // Per-user auth checks (cannot be shared):
                let updatable =
                    connection.base.world_manager.host.is_component_updatable(
                        local_converter, &global_entity, &component_kind)
                    || connection.base.world_manager.remote.is_component_updatable(
                        local_converter, &global_entity, &component_kind);
                if !updatable { continue; }

                // Per-user diff mask check (may differ due to dropped-packet recovery):
                if connection.base.world_manager.updater
                    .diff_handler.diff_mask_is_clear(&global_entity, &component_kind) {
                    continue;
                }

                update_events.entry(global_entity).or_default().insert(component_kind);
            }
        }
    }

    connection.send_packets(
        &self.component_kinds,
        &update_events,
        &snapshot_map,
        &self.global_diff_handler,
        &world,
        &*self.global_world_manager,
        // ... remaining existing params ...
    );
}
```

### Phase 3 Inner: `write_update` with Two Paths

`write_update` adds `snapshot_map` and `global_diff_handler`; retains `global_world_manager` (required by `entity_converter_mut` in both paths) and `world` (required for PATH A cache misses — see Section 15, Q4).

```rust
fn write_update<E: Copy + Eq + Hash + Send + Sync, W: WorldRefType<E>>(
    component_kinds:      &ComponentKinds,
    now:                  &Instant,
    world:                &W,
    global_world_manager: &dyn GlobalWorldManagerType,   // retained: entity_converter_mut
    global_diff_handler:  &GlobalDiffHandler<E>,         // NEW: cached update store access
    world_manager:        &mut LocalWorldManager,
    packet_index:         &PacketIndex,
    writer:               &mut BitWriter,
    global_entity:        &GlobalEntity,
    world_entity:         &E,
    snapshot_map:         &SnapshotMap,                  // NEW: UserDependent ECS snapshots
    has_written:          &mut bool,
    next_send_updates:    &mut HashMap<GlobalEntity, HashSet<ComponentKind>>,
) {
    let mut written = Vec::new();
    let component_kind_set = next_send_updates.get(global_entity).unwrap();

    for component_kind in component_kind_set {
        let diff_mask = world_manager.get_diff_mask(global_entity, component_kind);

        if !component_kinds.is_user_dependent(component_kind) {
            // ── PATH A: UserIndependent ──────────────────────────────────────────
            let diff_mask_key = diff_mask.as_key()
                .expect("diff mask > 8 bytes; unreachable for all registered components");

            let cached = match global_diff_handler.get_cached_update(
                global_entity, component_kind, diff_mask_key)
            {
                Some(cached) => cached,
                None => {
                    // Cache miss: one ECS read, one serialize, store result in MutChannel.
                    // converter obtained but not called (UserIndependent — no EntityProperty).
                    let mut converter = world_manager.entity_converter_mut(global_world_manager);
                    let mut temp = BitWriter::with_max_capacity();
                    true.ser(&mut temp);
                    component_kind.ser(component_kinds, &mut temp);
                    world.component_of_kind(world_entity, component_kind)
                        .expect("component verified in Phase 2")
                        .write_update(&diff_mask, &mut temp, &mut converter);
                    let cached = CachedComponentUpdate::capture(&temp)
                        .expect("component exceeds 512 bits; impossible after registration check");
                    global_diff_handler.set_cached_update(
                        global_entity, component_kind, diff_mask_key, cached);
                    cached
                }
            };

            // Overflow check — O(1), no ECS read
            let mut counter = writer.counter();
            counter.count_bits(cached.bit_count);
            if counter.overflowed() {
                if !*has_written {
                    Self::warn_overflow_update(
                        component_kinds.kind_to_name(component_kind),
                        cached.bit_count, writer.bits_free());
                }
                break;
            }

            *has_written = true;
            writer.append_cached_update(&cached);

        } else {
            // ── PATH B: UserDependent ────────────────────────────────────────────
            // EntityProperty fields require per-user local entity ID resolution.
            // ECS was read once in Phase 2 into snapshot_map.
            let snapshot = snapshot_map.get(&(*global_entity, *component_kind))
                .expect("UserDependent snapshot built in Phase 2");
            let mut converter = world_manager.entity_converter_mut(global_world_manager);

            // Counter pass
            let mut counter = writer.counter();
            true.ser(&mut counter);
            component_kind.ser(component_kinds, &mut counter);
            snapshot.write_update(&diff_mask, &mut counter, &mut converter);
            if counter.overflowed() {
                if !*has_written {
                    Self::warn_overflow_update(
                        component_kinds.kind_to_name(component_kind),
                        counter.bits_needed(), writer.bits_free());
                }
                break;
            }

            *has_written = true;

            // Writer pass
            true.ser(writer);
            component_kind.ser(component_kinds, writer);
            snapshot.write_update(&diff_mask, writer, &mut converter);
        }

        written.push(*component_kind);
        world_manager.record_update(now, packet_index, global_entity, component_kind, diff_mask);
    }

    let update_kinds = next_send_updates.get_mut(global_entity).unwrap();
    for kind in &written { update_kinds.remove(kind); }
    if update_kinds.is_empty() { next_send_updates.remove(global_entity); }
}
```

**`type SnapshotMap = HashMap<(GlobalEntity, ComponentKind), Box<dyn Replicate>>;`** — defined at the top of `world_writer.rs`.

---

## 12. Performance Projections

### 32 Players (Benchmark Scenario — All Moving Every Tick)

All avatar components are UserIndependent (NetworkedPosition, NetworkedVelocity — no EntityProperty). All dirty every tick (avatars move continuously) → cached updates are invalidated every tick by mutation → every tick is a cache miss for the first user, cache hit for the remaining 31.

| Operation | Before | After |
|---|---:|---:|
| ECS reads | 2,048 (counter + writer × 32 users × 32 entities) | **32** (one per entity, first user's cache miss) |
| Full Serde traversals | 1,024 | **32** (first user only) |
| `append_cached_update` copies | 0 | **992** (31 users × 32 components, ~9 bytes each) |
| Entity-level HashMap lookups | ~5,120 | **~160** (bitset intersection + dirty_words scan) |
| `take_update_events` per-user cost | O(dirty_entities × components) | **O(visible_entities / 64)** bitset AND |

**Expected tick reduction:** `send_packet_loop` from 39.1% → ~8–10%; `take_update_events` from 25.8% → ~3–5%.

### 10,000 CCU (Target Scale — Mixed Stability)

Assuming 100 dirty entities/tick (10% of 1,000 visible per user), all UserIndependent, no drops:

| Operation | Before | After |
|---|---:|---:|
| ECS reads | 200,000 (100 components × 10,000 users × 2 passes) | **100** (one per dirty component, first user) |
| Serializations | 100,000 | **100** (first user) |
| `append_cached_update` copies | 0 | **999,900** (9,999 users × 100 components) |
| Copy cost (9 bytes @ ~7ns each) | — | **~7ms** |
| Old total (ECS + Serde @ ~100ns each) | **~20ms** | **~7ms** |

For stable components (not mutated since the previous tick): **0 ECS reads, 0 serializations** — cached updates from the prior tick remain valid. This is the full Iris cross-tick persistence benefit.

---

## 13. Implementation Plan

Phases are ordered by dependency. Each phase has explicit prerequisites and a gate that must pass before the next begins.

### Phase 1 — Renames (No Dependencies) [X]

**Pure mechanical rename — no behavioral change. Can be done before any other phase.**

0. **Record pre-implementation baseline.** Before any code changes, run the sub-phase bench in the cyberlith repo:
   ```
   cargo run --features bench_profile -p cyberlith_bench --release -- \
       --scenario game_server_tick --warmup 100 --ticks 500
   ```
   Record results in `cyberlith/_AGENTS/CAPACITY_RESULTS.md`. Known values are `send_packet_loop` 39.1% / `take_update_events` 25.8%; re-confirming on the same machine ensures comparison validity.
1. Rename `ComponentUpdate` → `PendingComponentUpdate` in `shared/src/world/update/component_update.rs`
2. Update all usages: `WorldMutType::component_apply_update`, `component_apply_field_update`, `world_writer.rs`, `local_world_manager.rs`, and all other callers
3. Rename type alias `EntityIndex` → `LocalEntityIndex` throughout `UserDiffHandler` and any other per-user use sites

**Gate:** `cargo check` warning-clean; wasm32 checks (see §0 gate commands); `cargo test --workspace` green; namako gate green. No behavioral change.

### Phase 2 — Serde Layer Extensions [X]

**No callers yet — pure additions to `naia-shared/serde`.**

1. `BitWriter::bits_written() -> u32` (expose `current_bits`)
2. `BitWriter::bytes_written_slice() -> &[u8]` (expose `buffer[..byte_count]`; documents "complete 32-bit words only")
3. `BitWriter::scratch_bits_pending() -> (u32, u32)` (expose `(scratch, scratch_bits)`)
4. `CachedComponentUpdate { bytes: [u8; 64], bit_count: u32 }` + `CachedComponentUpdate::capture(writer: &BitWriter) -> Option<Self>`
5. `BitWriter::append_cached_update(&mut self, update: &CachedComponentUpdate)` — aligned + trailing-bit paths
6. `DiffMask::as_key() -> Option<u64>`
7. `BitCounter::count_bits` already exists — add test confirming behavior

**Gate:** Unit tests:
- `append_cached_update(captured)` round-trips at ALL bit alignments 0–63 in destination stream
- `capture` with pending scratch bits: write 7 bits → capture → append → read back correctly
- `capture` with word-boundary crossing: write 33 bits → capture → append at non-zero alignment
- 512-bit capture succeeds; 513-bit returns `None`
- `as_key` round-trips for 1, 4, 8 byte masks; returns `None` for 9-byte mask

### Phase 3 — Derive Extension + ComponentKinds [X]

1. `Replicate::has_entity_properties() -> bool` — default `false` (revive commented-out derive impl at line 1362 of `shared/derive/src/replicate.rs`)
2. `Replicate::max_bit_length() -> u32` — new derive-generated compile-time constant summing field bit widths
3. Derive macro: emit `fn has_entity_properties() -> bool { true }` for components with ≥1 `EntityProperty`
4. Derive macro: emit `fn max_bit_length() -> u32 { ... }` for all components
5. `ComponentKinds::user_dependent: HashSet<ComponentKind>` field
6. `ComponentKinds::add_component` — assert `max_bit_length() <= 512`; store `user_dependent` flag
7. `ComponentKinds::is_user_dependent(kind: &ComponentKind) -> bool`
8. Expose `pub fn kind_for_net_id(&self, net_id: u16) -> Option<ComponentKind>` on `ComponentKinds` — a direct lookup into the existing private `net_id_map: HashMap<NetId, ComponentKind>`; currently accessed only through the private `net_id_to_kind`

**Gate:** `cargo check` warning-clean; `cargo test --workspace` green. Unit assertions: `NetworkedPosition::has_entity_properties() == false`; any component with an `EntityProperty` field returns `true`. All existing cyberlith-registered components pass the 512-bit assertion (verified via E2E in Phase 10). Wasm32 checks green (this phase touches `shared/derive`).

### Phase 4 — MutChannelType Cached Update Store [X]

1. Add `get_cached_update`, `set_cached_update`, `clear_cached_updates` to `MutChannelType` trait with `unimplemented!()` defaults (forces all impls to update)
2. Add `cached_updates: RwLock<HashMap<u64, CachedComponentUpdate>>` to concrete `MutChannelData`
3. Implement the three new methods on `MutChannelData`; wire `clear_cached_updates()` into `MutChannelData::send()` after existing fan-out
4. Add `get_cached_update` and `set_cached_update` wrapper methods to `MutChannel` struct
5. Add `fn channel(&self) -> &MutChannel` accessor to `MutReceiverBuilder`
6. Add `GlobalDiffHandler::get_cached_update` and `GlobalDiffHandler::set_cached_update` accessors

**Gate:** Unit test — mutate component via `MutSender::mutate()`, confirm cached update clears; store via `set_cached_update`, confirm `get_cached_update` returns it next tick without mutation; mutate again, confirm it clears.

### Phase 5 — Two-Path `write_update` (Fix A) [X]

**Depends on Phases 2, 3, 4. No structural send-loop changes yet — thread new params through existing call chain.**

1. Add `type SnapshotMap = HashMap<(GlobalEntity, ComponentKind), Box<dyn Replicate>>` at top of `world_writer.rs`
2. Extend `WorldWriter::write_update` signature: add `snapshot_map: &SnapshotMap` and `global_diff_handler: &GlobalDiffHandler<E>`; **retain `global_world_manager`** (required by `entity_converter_mut` in both paths)
3. Implement PATH A (UserIndependent cached update) and PATH B (UserDependent snapshot) in `write_update`
4. Thread `snapshot_map` and `global_diff_handler` through the call chain: `write_updates` → `write_into_packet` → `Connection::send_packet` → `Connection::send_packets`
5. In `WorldServer::send_all_packets`: build `snapshot_map` for UserDependent dirty components before the per-user loop (uses existing `take_outgoing_events` result to find dirty entities; `GlobalEntityIndex` not yet needed)

**Gate:**
- E2E harness 93/93 green.
- Integration test added to naia harness: send a `UserIndependent` component (no `EntityProperty`) and a `UserDependent` component (with `EntityProperty`) through the new two-path `write_update`; receive on client side; assert deserialized values match the originals. Both paths must be exercised and green before Phase 5 is gated.
- Run sub-phase bench (command in Phase 10 step 1) and record Phase 5 partial-optimization results in `cyberlith/_AGENTS/CAPACITY_RESULTS.md`. `GlobalDirtyBitset` and bitset intersection are not yet in place — expect reduction in `send_packet_loop` from PATH A/B, but `take_update_events` reduction does not arrive until Phase 9.

### Phase 6 — GlobalEntityIndex + GlobalDiffHandler Extension [X]

**Structural refactor. Depends on Phase 5 passing.**

1. Add `GlobalEntityIndex(u32)` type with `INVALID` sentinel to `naia-shared`
2. Extend `GlobalDiffHandler<E>` with dense entity registry fields and all operations (see Section 4) — **NOTE**: `GlobalDiffHandler` is NOT generic over `E` (object-safe trait constraint prevents it); `idx_to_world: Vec<Option<E>>` and `world_entity(idx) -> E` were not implemented. `global_entity_map.global_entity_to_entity()` (HashMap) is used instead for Phase 2 world entity lookup.
3. Wire `alloc_entity`/`free_entity` into existing entity spawn/despawn paths (`host_spawn_entity`, `despawn_entity`)
4. Wire `register_component`/`deregister_component` into `GlobalDiffHandler`'s existing component registration path; populate `bit_to_kind: Vec<Option<ComponentKind>>` at registration time (extend vec if kind_bit ≥ current length)
5. `idx_to_components: Vec<ComponentFlags>` with `user_dependent: Vec<bool>` per entity [X] **IMPLEMENTED** — added alongside Phase 9 cleanup. `GlobalDiffHandler::is_component_user_dependent(idx, kind_bit)` provides O(1) array access; replaces `ComponentKinds::is_user_dependent()` HashSet lookup in Phase 2 hot path. Wire: `alloc_entity` grows and resets the slot; `register_component` sets the flag using `component_kinds.is_user_dependent()`.
6. Replace `UserDiffHandler::entity_to_index / index_to_entity` (`LocalEntityIndex` tables) with lookups into `GlobalDiffHandler`; per-user `DirtyQueue` row index changes from `LocalEntityIndex` to `GlobalEntityIndex`
7. **`DirtyNotifier::entity_idx` type change:** currently `EntityIndex` (per-user) → `GlobalEntityIndex`. The same entity now has one index shared across all users. Per-user `DirtySet` push/cancel continues to use this index as the row key (DirtyQueue now uses `GlobalEntityIndex` row indices). The `global` field added in Phase 7 also references it.

**Note:** `DirtyQueue::push(entity_idx: LocalEntityIndex, kind_bit: u16)` becomes `push(entity_idx: GlobalEntityIndex, kind_bit: u16)`. Verify `DirtyQueue::stride` (based on component kind count — unchanged).

**Gate:** `cargo check` warning-clean; `cargo test --workspace` green; namako gate green.

### Phase 7 — GlobalDirtyBitset [X]

**Depends on Phase 6.**

1. Add `GlobalDirtyBitset` struct with `ref_counts`, `dirty_components`, `dirty_entities`, `component_stride`, `component_count`, `capacity` fields
2. Implement `increment`, `decrement`, `is_component_dirty`, `dirty_entity_iter`, `dirty_words`
3. `increment`: fetch_add ref_count; on 0→1, set bit in `dirty_components` word; if word was 0 before, set entity summary bit in `dirty_entities`
4. `decrement`: fetch_sub ref_count; on 1→0, clear bit in `dirty_components`; if word becomes 0, clear entity summary bit
5. Extend `DirtyNotifier` — add `global: Weak<GlobalDirtyBitset>`; keep `set: Weak<DirtySet>` until Phase 9
6. `DirtyNotifier::notify_dirty` calls BOTH `set.push` and `global.increment`; `notify_clean` calls BOTH `set.cancel` and `global.decrement`
7. Update `MutChannel::new_channel` and `GlobalDiffHandler::register_component` to wire `GlobalDirtyBitset` into new `DirtyNotifier`s
8. Add `max_replicated_entities: u32` to `ServerConfig` (default 65,536); add `global_dirty: Arc<GlobalDirtyBitset>` to `WorldServer` and initialize from that field

**Gate:**
- Unit test — mutate component for 32 users; confirm `dirty_entity_iter` yields the entity; confirm `is_component_dirty` true; mark all users clean; confirm entity absent from iterator.
- Disconnect test — mutate an entity for 2 users; disconnect one user (drop all their `MutReceiver`s); confirm `dirty_entity_iter` still yields the entity (remaining user's ref-count non-zero); disconnect the second user; confirm the entity is no longer in `dirty_entity_iter` (ref-counts have reached zero, verifying §14's "User disconnect cleanup" invariant).
- E2E 93/93.

### Phase 8 — ConnectionVisibilityBitset [X]

**Depends on Phase 6 (`GlobalEntityIndex`).**

1. Add `ConnectionVisibilityBitset` struct with `Vec<u64>`, `set`/`clear`/`is_set`/`intersect_dirty`; capacity from `ServerConfig::max_replicated_entities`
2. Add `visibility: ConnectionVisibilityBitset` to `Connection`
3. Wire `set`/`clear` into all scope enter/exit paths in `update_entity_scopes` and `LocalWorldManager`
4. Wire entity pause state: pausing clears the visibility bit; unpausing sets it
5. **Dual-mode correctness gate:** temporarily keep the existing HashMap-based scope state alongside `ConnectionVisibilityBitset`, inserting a debug assertion at every scope enter/exit that both agree. Run E2E 93/93 in this dual-mode configuration. Once all 93 tests pass with the assertion active, remove the HashMap-based scope tracking.

**Gate:** E2E 93/93 with dual-mode assertion active (step 5 above); then E2E 93/93 after HashMap scope tracking removed. Full audit of all `update_entity_scopes` and `LocalWorldManager` call sites confirms no scope-transition path is unwired.

### Phase 9 — New Send Loop (Fix B) + DirtyQueue Removal [X]

**Depends on Phases 6, 7, 8. The full Iris three-phase send loop.**

1. Replace the per-user `take_outgoing_events` / `build_dirty_candidates_from_receivers` call with the three-phase loop from Section 11:
   - Phase 1: `global_dirty.dirty_entity_iter()`
   - Phase 2: entity-level filter + SnapshotMap build using `dirty_words`
   - Phase 3: per-user `visibility.intersect_dirty(&global_dirty)` → per-user diff mask checks → `update_events`
2. **Preserve priority ordering.** The current `write_updates` accepts `entity_priority_order: Option<&[GlobalEntity]>` controlling the order entities are written into the packet budget (priority accumulator — COMPLETE). After Phase 9, `update_events: HashMap<GlobalEntity, HashSet<ComponentKind>>` has no inherent order. Before calling `connection.send_packets`, sort the `update_events` keys by the priority accumulator's score for this user (same logic used today) and pass the sorted slice as `entity_priority_order`. No change to the priority accumulator or `write_updates` signature — only the Phase 3 loop gains a sort step over its already-small per-user candidate set.
3. [X] **Remove `EntityAndGlobalEntityConverter<E>` param from `write_updates`** — **IMPLEMENTED**. Changed `update_events: HashMap<GlobalEntity, HashSet<ComponentKind>>` + `entity_priority_order` + converter lookup in `write_updates` to `update_list: &mut Vec<(GlobalEntity, E, HashSet<ComponentKind>)>`. Priority advance+sort+world_entity resolve moved to caller (`world_server.rs`). `write_updates` and `write_update` are now pure serialization loops with no entity resolution. `send_packets` / `send_packet` / `write_packet` and `write_into_packet` all updated accordingly on both server and client paths. Gate: `cargo check --workspace` warning-clean, `cargo test --workspace` green.
4. **Remove `DirtyQueue` / `DirtySet` from `UserDiffHandler`:** [X] **IMPLEMENTED IN SPIRIT** — `dirty_set: Option<Arc<DirtySet>>` is `None` on the server path (when `GlobalDirtyBitset` is present). Server path never allocates a DirtySet; `DirtyNotifier.set` is a dead `Weak::new()` with no-op push/cancel. Client path retains `Some(Arc<DirtySet>)` for its dirty candidate tracking. Full letter removal (deleting DirtySet from shared code) is blocked by client path dependency.
5. **Remove `set: Weak<DirtySet>` from `DirtyNotifier`:** [X] **IMPLEMENTED IN SPIRIT** — on the server path, `DirtyNotifier.set` is always `Weak::new()` (from step 4), so `set.upgrade()` returns `None` and `push`/`cancel` are no-ops. The field exists in shared code for client compatibility only.
6. Remove `build_dirty_candidates_from_receivers` and `take_update_events` from `LocalWorldManager` — [X] **SERVER CALL SITES ALREADY REMOVED** (Phase 9 replaced them with the GlobalDirtyBitset path). `grep` of `server/src/` finds zero call sites for either method — only a comment in `connection.rs`. The methods remain in shared code as client-path infrastructure (`take_outgoing_events` at `client/src/connection/connection.rs` calls both). No server code calls them.

**Gate:** `cargo check` warning-clean; `cargo test --workspace` green; namako gate green. Run the sub-phase bench (command in Phase 10 step 1) and record results in `cyberlith/_AGENTS/CAPACITY_RESULTS.md`. Targets: `take_update_events` from 25.8% → <5%; `send_packet_loop` from 39.1% → <10%. Compare against the Phase 5 partial-optimization baseline to isolate the GlobalDirtyBitset + bitset-intersection contribution.

### Phase 10 — Benchmark + Documentation [X]

0. **Update cyberlith's naia dependency** to the Phase 9 commit on `dev-trunk`. The bench lives in the cyberlith repo and exercises naia's pipeline through the full game-server stack; it must consume the new code before results are meaningful.
1. Full bench run (in cyberlith repo): `cargo run --features bench_profile -p cyberlith_bench --release -- --scenario game_server_tick --warmup 100 --ticks 500`
2. Record sub-phase breakdown in `cyberlith/_AGENTS/CAPACITY_RESULTS.md`
3. Compare against pre-Phase-1 baseline (39.1% + 25.8%), Phase 5 partial-optimization checkpoint, and Phase 9 final results
4. Project to 10,000 CCU from measured scaling (reference §12 projection table; note which numbers are now measured vs. estimated)

---

## 14. Correctness Invariants

**Wire format unchanged.** `append_cached_update` produces bit-identical output to direct `Serde::ser` — the receiver's deserialization path is unaffected. Correctness gate: Phase 2 unit tests round-trip at all alignments.

**`CachedComponentUpdate::capture` is lossless.** Captures both flushed words (via `bytes_written_slice`) and pending scratch bits (via `scratch_bits_pending`), mirroring `BitWriter::finalize`. A captured update contains all bits written to the temp writer at capture time.

**Per-user diff mask independence.** The cached update key is the full diff mask (`diff_mask_key: u64`). Users with different diff masks (e.g. after dropped-packet recovery ORs extra bits back in) get different cache keys → independent `CachedComponentUpdate` entries → correct per-user serialization. `record_update` is per-user, per-component, and runs after every component write regardless of cache hit/miss.

**Cache invalidation on mutation.** `MutChannel::send()` calls `clear_cached_updates()` synchronously before returning. A mutated component's `CachedComponentUpdate` is invalid before any send-path code can observe it. No tick-boundary race.

**Partial entity sends preserved.** PATH A: `count_bits(cached.bit_count)` may overflow → `break`. PATH B: counter pass may overflow → `break`. Both defer the component to the next packet identically to today.

**UserDependent snapshot timing.** Snapshots are built in Phase 2, before the per-user loop (Phase 3). The snapshot reflects component state at Phase 2 time — the same state the old code read during per-user `write_update`. No behavioral change.

**EntityProperty correctness in PATH B.** The snapshot holds the component's internal state (GlobalEntity refs in EntityProperty fields). Per-user `write_update` on the snapshot uses the per-user converter to produce per-user local entity wire IDs — identical behavior to the current ECS-direct path.

**GlobalEntityIndex stability.** A `GlobalEntityIndex` is valid for the lifetime of its entity. Despawn zeroes the registry slot in `GlobalDiffHandler` and pushes the index to the free list. Phase 2's `has_entity` check protects against stale indices from entities despawned between the start of Phase 1 and Phase 2.

**GlobalDirtyBitset consistency.** The ref-count approach ensures the entity summary bit is set iff any user has a non-clear diff mask for that entity. `DirtyNotifier::notify_dirty / notify_clean` are the sole update sites — no other code path changes per-user dirty state without going through them.

**DirtyQueue removal correctness.** After Phase 9, per-user dirty state is tracked entirely by `AtomicDiffMask` (per-user per-component diff mask). The `DirtyQueue` was an intermediate data structure derived from diff mask transitions; its removal does not change what gets sent — only the mechanism for discovering candidates (GlobalDirtyBitset + bitset intersection replaces queue drain).

**User disconnect cleanup.** When a user disconnects, their `MutReceiver`s drop. Each `AtomicDiffMask` that was non-zero at drop time fires `notify_clean` via the `DirtyNotifier`, which calls `GlobalDirtyBitset::decrement`. Ref-counts correctly reach zero as all receivers for that user are dropped, and entity summary bits clear. No leaked dirty state.

**ConnectionVisibilityBitset consistency.** All entity scope enter/exit paths (spawn, despawn, room join/leave, UserScope include/exclude, pause/unpause) must call `visibility.set`/`clear`. An audit of all call sites in `update_entity_scopes` and `LocalWorldManager` is required at Phase 8 gate.

**`record_update` unchanged.** Tracks sent diff masks for drop-recovery (ORs dropped mask back in on NACK). Per-user, per-component, runs after every component write regardless of PATH A/B or cache hit/miss.

---

## 15. Migration Compatibility

**Public API:** No changes to `#[derive(Replicate)]`, `Property<T>`, entity spawn/despawn, rooms, user scope, channels, or any user-facing server/client API. All changes are internal to naia's replication pipeline.

**Protocol / Wire Format:** Unchanged. Receiver-side deserialization is unaffected.

**Naia branching policy:** All implementation work on `dev-trunk`. No commits to `main`. Gate passes before merge per established policy.

**`wasm32-unknown-unknown`:** Phase 2 (`naia-serde`) and Phase 3 (`naia-shared/derive`) affect crates used by wasm client builds. Run `cargo run -p automation_cli -- check-wasm` after each of these phases.

**Test suite:** `cargo test --workspace` + namako gate (332 scenarios as of 2026-05-10) must stay green throughout. See §0 for exact gate commands. Cyberlith E2E verification runs only at Phase 10 after updating cyberlith's naia dependency.

---

## 16. Open Questions

**Q1: PATH A cache miss and `world`/`world_entity` in `write_updates`.** After Phase 9, `write_updates` loses the entity-level converter (`EntityAndGlobalEntityConverter<E>`) but still needs `world: &W` for PATH A cache misses. The `W` generic parameter remains.

PATH A cache misses cannot be pre-built in Phase 2 because the diff mask key is per-user: after dropped-packet recovery, user A may have diff_mask=0b11 (both fields dirty) while user B has diff_mask=0b01 (field 0 only). Phase 2 runs before per-user intersection, so we don't yet know which diff masks will be requested. The first user with any given key triggers a cache miss (one ECS read, one serialize); all subsequent users with that key get a hit. In steady state (no drops), all users share the same diff mask → single cache entry built on the first user's request.

If profiling after Phase 10 shows PATH A cache misses are negligible in practice, this is not worth further optimization.

**Q2: `ConnectionVisibilityBitset` and per-component auth.** The visibility bitset is entity-level. Per-component auth checks (`is_component_updatable`) remain in the Phase 3 per-user loop at ~O(1) per component. Profile after Phase 10 to determine if component-level visibility bits would provide meaningful further reduction.

**Q3: `RwLock<HashMap<u64, CachedComponentUpdate>>` contention.** Most ticks, cached updates are read-only (Phase 3 per-user path). Write contention occurs only on cache misses (first user after mutation). If Phase 10 profiling shows `RwLock` write contention at high CCU, consider a sharded lock or `DashMap`.

**Q4: Quantization.** `NetworkedPosition` storing raw `f32` bits wastes ~16 bits per component that quantization could recover. This is future work; the `CachedComponentUpdate` infrastructure naturally accommodates quantized components — the cached update stores quantized bytes, same path.

---

## 17. C.7 — write_updates Hot-Path Optimization (spec 2026-05-14)

**Status:** COMPLETE — C.7.A ✅ (a9808eed) + C.7.B compact-key ✅ (1859d651) + record_update_dense ✅ (665bb9ec) + C.7.B full-flat-Vec ✅ (0358daa6) + C.7.C+D wire-cache ✅ (04e6d2fc)
**Proposed:** 2026-05-14, based on `send_packet_loop` sub-breakdown bench + full code audit
**Branch:** `dev` (never commit to `main`)

---

### 17.1 Findings Summary

After Iris Phases 1–10, `write_updates` (entity/component serialization in `WorldWriter`) = 36.8%
of tick in the adversarial oscillating-player bench. Code audit of the hot path identified:

**Four remaining bottlenecks:**

| # | Location | Type | Calls/tick (32p, 10 dirty ents) |
|---|---|---|---|
| A | `UserDiffHandler.receivers` | `HashMap<(GlobalEntity, ComponentKind), MutReceiver>` | ~1,800 Phase 3 + ~1,800 write_update = ~3,600 |
| B | `GlobalDiffHandler.mut_receiver_builders` | `HashMap<(GlobalEntity, ComponentKind), MutReceiverBuilder>` | ~1,800 PATH A cache operations |
| C | `MutChannelData.cached_updates` | `RwLock<HashMap<u64, CachedComponentUpdate>>` (2 lock + 1 HashMap per hit) | ~1,800 PATH A cache hits |
| D | `component_kinds.is_user_dependent()` | `HashSet<ComponentKind>` lookup in `write_update` | ~1,800 (one per component×user visit) |

**Estimated HashMap contribution to write_updates:** ~1,920 HashMap + lock operations/tick ×
~35 ns each ≈ **67 µs out of 1,763 µs write_updates (~4%).** Eliminating all of them will not
close the 36.8% gap — the remaining 96% is actual serialization and memcpy work.

**Critical bench caveat:** The 36.8% figure is adversarial. In the oscillating-player bench,
all players move every tick → all position/velocity components dirty every tick → PATH A cache
is invalidated every tick. PATH A's cross-tick amortization provides zero benefit in this case.
In production (sparse movement, stable terrain), write_updates approaches near-zero between
player events. The HashMap optimization improves the adversarial floor and minimum per-operation
latency, but production write_updates overhead is already low without it.

**Pattern 3 — DROPPED from C.7.** Code audit confirms quantization is already fully implemented:
- `NetworkedPosition` uses `SignedVariableFloat<14, 0>` tile-grid delta encoding
- `NetworkedVelocity` / `NetworkedAngularVelocity` use `SignedVariableFloat<11, 2>`
- `NetworkedRotation` uses smallest-three quaternion (21 bits)
- DiffMask already provides field-level dirty tracking; only changed fields are sent

True frame-to-frame delta encoding (send `current − previous`) would require: a protocol-breaking
wire format change, server-side previous-state storage, client-side delta accumulation, and version
negotiation. The bandwidth headroom remaining after variable-length encoding is marginal and does
not justify this cost. **Do not implement.**

---

### 17.2 What Would Actually Need to Change

The four sub-tasks below are ordered by implementation size and coupling. C.7.A is independent;
C.7.B/C/D are coupled and should be done together.

---

#### C.7.A — Trivial: replace `is_user_dependent()` HashSet with array call in `write_update`

**Problem:** `write_update` (`world_writer.rs`) calls `component_kinds.is_user_dependent(component_kind)` —
a `HashSet<ComponentKind>` lookup — to dispatch PATH A vs PATH B. An equivalent O(1) array lookup
already exists: `GlobalDiffHandler.is_component_user_dependent(entity_idx, kind_bit)` backed by
`idx_to_components: Vec<ComponentFlags>` (Phase 6, implemented).

**What changes:**
- `write_update` receives `entity_idx: GlobalEntityIndex` and `kind_bit: u16` from its caller
  (both are available in `send_all_packets`'s Phase 3 loop where `write_updates` is called)
- Replace `component_kinds.is_user_dependent(component_kind)` → `global_diff_handler.is_component_user_dependent(entity_idx, kind_bit)`
- Files: `shared/src/world/world_writer.rs` (write_update signature + call site);
  `server/src/server/world_server.rs` (thread entity_idx + kind_bit through to write_updates)

**Tradeoffs:**
- PRO: eliminates one HashSet lookup per component per user per tick; no architectural change
- CON: widens the `write_update` / `write_updates` signature by two parameters; client path
  does not have `GlobalEntityIndex` — client's `write_update` must retain the `component_kinds.is_user_dependent()` path or get a dummy `None` for the array lookup

**Risk:** Low. The client path can continue using the HashSet; server path gains the array.
**Gate:** `cargo check --workspace` warning-clean + `cargo test --workspace` green + namako gate.

---

#### C.7.B — Medium: replace `UserDiffHandler.receivers` HashMap with stride-indexed flat array

**Problem:** `receivers: HashMap<(GlobalEntity, ComponentKind), MutReceiver>` is the highest-frequency
hot-path HashMap, accessed in:
- Phase 3 build: `is_component_dirty_and_delivered_for_entity` → `receivers.get(...)` — one lookup per component per user per dirty entity
- `write_update`: `get_diff_mask` → `receivers.get(...)` — same rate
- Hot path total: ~3,600 lookups/tick in the bench

**Target structure:**
```rust
// UserDiffHandler — new field
receivers_dense: Vec<Option<MutReceiver>>,  // flat; stride = kind_count
kind_count: usize,                           // fixed at construction

// Slot calculation (O(1) multiply + add):
// fn slot(entity_idx: GlobalEntityIndex, kind_bit: u16) -> usize {
//     entity_idx.as_usize() * self.kind_count + kind_bit as usize
// }
```

**Memory:** At 256 entities × 16 kind_bits × 48 bytes per `Option<MutReceiver>` × 32 users
≈ 6.3 MB total. 9× increase vs. current HashMap per user; acceptable in absolute terms.
`Option<MutReceiver>` is 48 bytes (three Arc clones = three pointer-sized fields).
The dense array improves CPU cache locality vs. HashMap pointer chasing.

**Implementation steps:**
1. Add `kind_count: usize` and `receivers_dense: Vec<Option<MutReceiver>>` to `UserDiffHandler`
2. Add `fn ensure_dense_capacity(&mut self, entity_idx: GlobalEntityIndex)` — grows `receivers_dense`
   to `(entity_idx.as_usize() + 1) * self.kind_count` slots if needed (mirrors `DirtyQueue::ensure_capacity`)
3. In `register_component(entity_idx, kind_bit, receiver)`:
   call `ensure_dense_capacity(entity_idx)` then `receivers_dense[slot] = Some(receiver.clone())`
4. In `deregister_component(entity_idx, kind_bit)`: `receivers_dense[slot] = None`
5. In all hot-path methods, callers must supply `(GlobalEntityIndex, kind_bit)` — available in
   Phase 3's dirty_words loop. Fallback: when only `(GlobalEntity, ComponentKind)` is available,
   resolve via `GlobalDiffHandler.global_to_idx` + `kind_bits` before the call
6. Keep `receivers: HashMap<...>` for cold-path methods during the transition; remove after all
   hot paths are migrated and gate passes

**Callers that need updating:**
- `server/src/server/world_server.rs`: Phase 3 inner loop already has `global_idx: GlobalEntityIndex`
  and `kind_bit: u16` — these thread directly to `is_component_dirty_and_delivered_for_entity`
- `shared/src/world/world_writer.rs`: `write_update` needs `entity_idx + kind_bit` (from C.7.A work)
- `server/src/connection/connection.rs`: any call sites in `send_packet` / `write_packet`

**Key correctness invariant:** When an entity is freed (`free_entity`), all of its dense slots must
be cleared: `for k in 0..kind_count { receivers_dense[slot(entity_idx, k)] = None; }`. Missing this
causes stale `MutReceiver` access for a recycled `GlobalEntityIndex`.

**Gate:** E2E 93/93 + namako gate. Add integration test: register 32 entities × 8 components, verify
correct receiver returned for all (entity, kind) pairs after a random alloc/free/re-alloc sequence
to exercise the free-list + slot reuse path.

---

#### C.7.C + C.7.D — Coupled: relocate wire cache from `MutChannelData` to `GlobalDiffHandler` dense array

These two sub-tasks are entangled and should be implemented atomically.

**C.7.C Problem:** `MutChannelData.cached_updates: RwLock<HashMap<u64, CachedComponentUpdate>>` stores
the per-component wire cache inside the `MutChannelType` trait object. Every PATH A cache hit pays:
- `Arc<RwLock<dyn MutChannelType>>::read()` — lock acquire #1
- `MutChannelData.cached_updates` RwLock read — lock acquire #2
- HashMap lookup by `u64` key

The RwLock is not contended today (single-threaded send loop), but the lock acquisition overhead
and HashMap indirection are still real. The HashMap key space for all Cyberlith components is
exactly 2 values (single-property components: key `0x01` = property dirty; `0x00` = nothing dirty
and would not reach the cache). Multi-property components use a larger key space but are rare.

**C.7.D Problem:** `GlobalDiffHandler.mut_receiver_builders: HashMap<(GlobalEntity, ComponentKind), MutReceiverBuilder>`
is the outer lookup gate for PATH A cache access. Every `get_cached_update` and `set_cached_update`
call pays one HashMap lookup here before reaching the inner cache.

**Unified solution — move wire cache to GlobalDiffHandler:**

```rust
// GlobalDiffHandler — new fields
wire_cache: Vec<Option<(u64, CachedComponentUpdate)>>,  // flat; stride = kind_count
wire_cache_kind_count: usize,

// Accessors (O(1)):
pub fn get_wire_cache(&self, entity_idx: GlobalEntityIndex, kind_bit: u16, key: u64)
    -> Option<CachedComponentUpdate>
{
    let slot = entity_idx.as_usize() * self.wire_cache_kind_count + kind_bit as usize;
    self.wire_cache.get(slot)?.and_then(|(k, v)| if *k == key { Some(*v) } else { None })
}

pub fn set_wire_cache(&mut self, entity_idx: GlobalEntityIndex, kind_bit: u16, key: u64,
    update: CachedComponentUpdate)
{
    let slot = entity_idx.as_usize() * self.wire_cache_kind_count + kind_bit as usize;
    if let Some(entry) = self.wire_cache.get_mut(slot) {
        *entry = Some((key, update));
    }
}

pub fn clear_wire_cache(&mut self, entity_idx: GlobalEntityIndex, kind_bit: u16) {
    let slot = entity_idx.as_usize() * self.wire_cache_kind_count + kind_bit as usize;
    if let Some(entry) = self.wire_cache.get_mut(slot) {
        *entry = None;
    }
}
```

**Cache invalidation wiring:** `MutChannelData.send()` must clear the corresponding slot in
`GlobalDiffHandler.wire_cache`. Options:

- **Option A (preferred):** Give `DirtyNotifier` a `Weak<Mutex<GlobalCacheStore>>` field where
  `GlobalCacheStore` is a thin wrapper around `wire_cache` owned by `GlobalDiffHandler` and shared
  via `Arc`. On `notify_dirty()`, call `cache.clear_slot(entity_idx, kind_bit)`. This is consistent
  with how `GlobalDirtyBitset` is already wired to `DirtyNotifier` (same `Weak` pattern, Phase 7).

- **Option B (simpler but coarser):** In `world_server::send_all_packets`, at the start of Phase 2,
  clear all wire_cache slots for every entity in `dirty_entity_iter()`. Since Phase 2 already
  iterates dirty entities, this is O(dirty_entities × dirty_components) with no additional traversal.
  Simpler to implement; correct because Phase 2 runs before Phase 3 uses the cache. Downside:
  clears the cache even for entities that ended up not sending (priority-deferred), forcing a
  re-build next tick.

**Option A is the architecturally clean choice.** Option B is acceptable as a first pass.

**Cache key:** The single inline entry uses `(u64, CachedComponentUpdate)` — key is the DiffMask
packed as u64. For dropped-packet recovery (ORed diff mask produces a different key), the entry
simply misses → re-serialize → correct behavior. No multi-entry needed.

**Memory:** Dynamic Vec growing with entity alloc, same as receivers_dense. At 256 entities ×
16 kind_bits × 72 bytes per `Option<(u64, CachedComponentUpdate)>` = 295 KB. Acceptable.

**Remove from `MutChannelData`:** Once `GlobalDiffHandler.wire_cache` is the authoritative store:
- Remove `cached_updates: RwLock<HashMap<u64, CachedComponentUpdate>>` from `MutChannelData`
- Remove `get_cached_update`, `set_cached_update`, `clear_cached_updates` from `MutChannelType` trait
- Remove the three wrapper methods from `MutChannel`
- Remove the three accessor methods from `GlobalDiffHandler` that go through `mut_receiver_builders`
- `mut_receiver_builders` itself may still be needed for the cold path (`has_component`, `deregister_component`, `receiver`) — keep or migrate those as a follow-up

**Files modified:**
- `shared/src/world/update/global_diff_handler.rs` — add `wire_cache` field + accessors + `ensure_wire_cache_capacity`
- `shared/src/world/update/mut_channel.rs` — add `Weak<GlobalCacheStore>` to `DirtyNotifier` (Option A); wire `notify_dirty` → cache clear
- `server/src/world/mut_channel.rs` — remove `cached_updates` from `MutChannelData`; remove `MutChannelType` cache trait methods
- `shared/src/world/world_writer.rs` — replace `global_diff_handler.get_cached_update(entity, kind, key)` with `global_diff_handler.get_wire_cache(entity_idx, kind_bit, key)`; same for set
- `server/src/server/world_server.rs` — thread `&mut global_diff_handler` into write path where needed

**Gate:** Same as C.7.B. Add unit test: mutate component via `MutSender::mutate()`; confirm `get_wire_cache` returns None; set via `set_wire_cache`; confirm hit; mutate again (triggering `notify_dirty`); confirm None again.

---

### 17.3 Tradeoffs and Risks

**Dense array memory vs. HashMap memory:**

| Structure | Current (HashMap) | After C.7 (dense array) |
|---|---|---|
| `UserDiffHandler.receivers` per user | ~200 entries × ~80B = 16 KB | 256 ents × 16 kinds × 48B = 196 KB |
| `GlobalDiffHandler.wire_cache` (global) | via mut_receiver_builders HashMap | 256 ents × 16 kinds × 72B = 295 KB |
| Total delta | — | ~6.6 MB at 32 users |

6.6 MB is within acceptable bounds (server RSS was 12.98 MB post-Iris). N_ram is not the binding
constraint.

**Sender-side symmetry:** Naia's design is sender-side symmetric (client uses the same `world_writer.rs`
path as server). The client does NOT have `GlobalEntityIndex` or `GlobalDiffHandler`. Any change that
threads `GlobalEntityIndex` through `write_update` must supply a `None`/fallback on the client path
so both compile cleanly under `#[cfg(feature = "client")]` vs. `#[cfg(feature = "server")]` guards.
C.7.A's client fallback (keep `component_kinds.is_user_dependent()` for the client path) is the model
for all C.7 sub-tasks that widen `write_update`'s signature.

**`GlobalDiffHandler` is not `Send + Sync` by itself:** The dense Vec fields are plain `Vec<T>`.
`GlobalDiffHandler` is accessed exclusively from the single-threaded send loop today. If parallel
per-user sends are ever introduced, `wire_cache` would need interior mutability. The `Cell` approach
would work for single-threaded parallel only; an `AtomicCell` or sharded structure would be needed
for true multi-thread. This is fine for current architecture; document as a known limitation.

**`max_replicated_entities` cap vs. dynamic Vec:** `GlobalDirtyBitset` and `ConnectionVisibilityBitset`
are pre-allocated to `max_replicated_entities = 65,536` at server startup. The new `wire_cache` and
`receivers_dense` Vecs grow dynamically with `alloc_entity`. If an entity's `GlobalEntityIndex`
exceeds the pre-allocated bitset capacity, the dirty/visibility tracking silently fails (existing
latent bug — not introduced by C.7). Documenting this mismatch is important; fixing it (cap the
dynamic Vecs to `max_replicated_entities` with a panic on overflow) is a correctness hardening step
that should accompany C.7.D.

---

### 17.4 What This Actually Buys

Given that HashMaps account for ~4% of write_updates and the adversarial bench misrepresents
production steady state, the honest projection is:

- **Adversarial bench (all players moving every tick):** ~4–6% reduction in write_updates (67 µs
  saved out of 1,763 µs). The remaining ~94% is path-A memcpy work (`append_cached_update` byte loop)
  and ECS reads for first-user cache misses. These are harder to attack without SIMD bulk copy.

- **Production steady state (sparse movement):** write_updates is already near-zero between events.
  C.7 reduces the per-event cost but this is not the binding constraint.

- **Real value of C.7:** Better cache locality throughout the hot path (dense arrays fit in L1/L2;
  scattered HashMaps with pointer chains do not). Improved readability and explicit data layout.
  Enables future SIMD acceleration of the send loop (dense arrays are SIMD-friendly; HashMaps are not).
  Removes the `RwLock` indirection, making the codebase ready for parallel per-user sends if that
  ever becomes necessary.

- **`append_cached_update` bulk-copy fast path:** Not part of C.7 but worth profiling post-C.7. For
  byte-aligned writes (common after the component kind header), replacing the byte-by-byte loop with
  a `ptr::copy_nonoverlapping` / SIMD unroll would address the remaining ~94% of the adversarial
  bench cost. This is a naia-serde-level change with no architectural impact.

---

### 17.5 Implementation Order and Gates

| Step | Sub-task | Depends on | Gate |
|---|---|---|---|
| 1 | C.7.A — `is_user_dependent` array call | — | `cargo check --workspace` warning-clean + `cargo test --workspace` |
| 2 | C.7.B — `receivers_dense` flat array | C.7.A (for entity_idx threading) | E2E 93/93 + namako gate + alloc/free integration test |
| 3 | C.7.C+D — `wire_cache` in `GlobalDiffHandler` | C.7.B (entity_idx + kind_bit available at call sites) | E2E 93/93 + namako gate + cache invalidation unit test |

Run `cargo run --features bench_profile -p cyberlith_bench --release -- --scenario game_server_tick --warmup 100 --ticks 500` before and after each step to confirm no regression and measure actual gain. Record in `cyberlith/_AGENTS/CAPACITY_RESULTS.md`.

All work on `dev` branch. Never commit to `main`. Pre-push check: `cargo check -p naia-client --target wasm32-unknown-unknown` (C.7.A touches world_writer.rs which is shared with client path).
