# PERF — Naia Iris Replication Architecture

**Status:** SPEC (not yet implemented)
**Created:** 2026-05-13
**Supersedes:** `PERF_SHARED_UPDATE_BLOB.md`
**Context:** Sub-phase profiling in cyberlith benchmark — `cyberlith/_AGENTS/CAPACITY_RESULTS.md`
**Scope:** `naia-shared` (serde, world, update, local), `naia-server` (connection, world_server)

---

## 1. Problem Statement

Sub-phase profiling at 32 players (release profile, `game_server_tick` bench):

| Phase | % of tick | Root cause |
|---|---:|---|
| `send_packet_loop` | **39.1%** | `component.write_update()` × users × dirty_entities: O(N²) |
| `take_update_events` | **25.8%** | Entity-level HashMap lookups × users × dirty_entities: O(N²) |

At 32 players all moving: **1,024 ECS reads** per tick (2 per component per user — counter pass + writer pass), **~5,120 HashMap lookups** for entity-level facts that are identical for all users, and **1,024 Serde traversals** bitpacking the same component data 32 times.

At **10,000 CCU** these numbers become 320,000 ECS reads and 320,000 serializations per tick — consuming the entire server tick budget before packets even leave the machine.

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
| **MutChannel Blob Cache** | Redundant ECS reads + re-serialization | Persistent pre-serialized blob, invalidated at mutation |
| **Two Principled Paths** | Unified but incorrect treatment of entity-reference vs pure-data components | `UserIndependent` (blob) + `UserDependent` (snapshot) |

The new `send_all_packets` loop becomes three phases:

```
Phase 1 — Build global dirty candidate set      O(dirty_entities / 64)
Phase 2 — Entity filter + Poll-and-Copy         O(dirty_entities × avg_components)
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

Note: `UserDiffHandler` already has the right idea — it allocates a per-user `LocalEntityIndex` (u32) for each replicated entity via `entity_to_index: HashMap<GlobalEntity, LocalEntityIndex>` and `index_to_entity: Vec<Option<GlobalEntity>>`. The existing per-user `EntityIndex` type alias is renamed `LocalEntityIndex` throughout to make the per-user vs. global distinction explicit. The innovation is making this index **global** (shared across all users) via `GlobalEntityIndex`, rather than per-user.

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
    /// GlobalEntity → GlobalEntityIndex (for MutSender registration)
    global_to_idx: HashMap<GlobalEntity, GlobalEntityIndex>,
    /// Dense arrays indexed by GlobalEntityIndex (index 0 = unused sentinel)
    idx_to_global: Vec<Option<GlobalEntity>>,
    idx_to_world:  Vec<Option<E>>,
    /// Per-entity component metadata. Packed bits — one bit per registered ComponentKind.
    idx_to_components: Vec<ComponentFlags>,
    /// Free list for index recycling on entity despawn
    free_list: Vec<GlobalEntityIndex>,
    next_idx: u32,
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
- `register_component(idx, kind, is_user_dependent)` — sets bit in `idx_to_components`
- `deregister_component(idx, kind)` — clears bit
- `get_cached_update(entity: &GlobalEntity, kind: &ComponentKind, key: u64) -> Option<CachedComponentUpdate>` — blob cache accessor (see Innovation 4)
- `set_cached_update(entity: &GlobalEntity, kind: &ComponentKind, key: u64, update: CachedComponentUpdate)` — blob cache write

**Migration from `UserDiffHandler::LocalEntityIndex`:** The per-user `entity_to_index` / `index_to_entity` tables in `UserDiffHandler` become unnecessary. Per-user components that previously used `LocalEntityIndex` as a row key in `DirtyQueue` switch to `GlobalEntityIndex`. The per-user `DirtyQueue` row index IS the `GlobalEntityIndex`. Since the global registry assigns one index per entity regardless of scope, per-user visibility is tracked separately (Innovation 3).

---

## 5. Innovation 2: GlobalDirtyBitset — Centralized Mutation Tracking

### Problem

Currently every `MutSender::mutate(property_index)` call fans out only to per-user `MutReceiver` masks via `MutChannel::send()`. There is no server-level signal of "which entities have ANYTHING dirty for ANY user." Computing this requires iterating all users' `DirtyQueue`s and building a union — O(users × dirty_entities) per tick.

### Solution

A single server-level bitset tracking which `(GlobalEntityIndex, ComponentKind)` pairs have pending mutations. Maintained atomically at mutation time via existing `DirtyNotifier` infrastructure.

```rust
/// Server-global dirty tracking matrix.
/// Dimensions: GlobalEntityIndex (rows) × ComponentNetId (columns).
/// Layout: dirty_matrix[entity_idx * stride + word_idx] bit kind_idx%64.
///
/// Maintains a reference count per (entity, kind): the count of users
/// for whom this (entity, kind) has a non-clear DiffMask.
/// The entity-level summary bitset has bit N set if any (N, kind) count > 0.
pub struct GlobalDirtyBitset {
    // Ref-counts: how many users have this (entity, kind) dirty
    ref_counts: Vec<AtomicU32>,   // [entity_idx * component_count + kind_idx]
    component_count: usize,
    // Summary: which entities have any dirty component for any user
    dirty_entities: Vec<AtomicU64>,  // one bit per GlobalEntityIndex
    entity_stride: usize,
    capacity: usize,              // current max GlobalEntityIndex
}
```

**Operations:**

```rust
impl GlobalDirtyBitset {
    /// Called from DirtyNotifier::notify_dirty() when a user's (entity, kind)
    /// transitions from clean to dirty. Increments ref-count; sets entity bit if 0→1.
    pub fn increment(&self, entity_idx: GlobalEntityIndex, kind_bit: u16);

    /// Called from DirtyNotifier::notify_clean() when a user's (entity, kind)
    /// transitions from dirty to clean. Decrements ref-count; clears entity bit if 1→0.
    pub fn decrement(&self, entity_idx: GlobalEntityIndex, kind_bit: u16);

    /// Iterate entities with any dirty component. O(capacity / 64).
    pub fn dirty_entity_iter(&self) -> impl Iterator<Item = GlobalEntityIndex>;

    /// Get the dirty component words for one entity (for per-component iteration).
    pub fn dirty_words(&self, entity_idx: GlobalEntityIndex) -> &[AtomicU64];
}
```

**Wire-up to `DirtyNotifier`:**

```rust
pub struct DirtyNotifier {
    entity_idx: GlobalEntityIndex,  // was LocalEntityIndex (per-user), now GlobalEntityIndex (global)
    kind_bit: u16,
    set: Weak<DirtySet>,             // existing: per-user queue
    global: Weak<GlobalDirtyBitset>, // NEW: server-level summary
}

impl DirtyNotifier {
    fn notify_dirty(&self) {
        if let Some(set) = self.set.upgrade() { set.push(self.entity_idx, self.kind_bit); }
        if let Some(g) = self.global.upgrade() { g.increment(self.entity_idx, self.kind_bit); }
    }
    fn notify_clean(&self) {
        if let Some(set) = self.set.upgrade() { set.cancel(self.entity_idx, self.kind_bit); }
        if let Some(g) = self.global.upgrade() { g.decrement(self.entity_idx, self.kind_bit); }
    }
}
```

The `GlobalDirtyBitset` is owned by `WorldServer` (or `GlobalWorldManager`) and shared via `Arc`. It is populated automatically as mutations arrive, with zero per-tick overhead beyond the atomic increments already happening for per-user dirty tracking.

---

## 6. Innovation 3: Per-Connection Visibility Bitsets

### Problem

Per-user dirty candidates today: `build_dirty_candidates_from_receivers()` walks the per-user `DirtyQueue`, finds entities with dirty bits, then `take_outgoing_events` applies entity-level filters. Per-user scope checks (`paused_entities`, `is_component_updatable`) are applied one entity at a time. No global pre-filtering.

At 10,000 CCU with 10,000 visible entities per user: these HashMap-based iterations dominate.

### Solution

Each connection maintains a `ConnectionVisibilityBitset` — one bit per `GlobalEntityIndex`. Set when an entity enters scope for this user, cleared when it leaves.

```rust
pub struct ConnectionVisibilityBitset {
    visible: Vec<u64>,  // one bit per GlobalEntityIndex; word = idx / 64, bit = idx % 64
    capacity: usize,
}

impl ConnectionVisibilityBitset {
    pub fn set(&mut self, idx: GlobalEntityIndex);
    pub fn clear(&mut self, idx: GlobalEntityIndex);
    pub fn is_set(&self, idx: GlobalEntityIndex) -> bool;
    /// Bitwise AND with global dirty summary, returns iterator of dirty+visible indices.
    /// O(capacity / 64) — the hot path for per-user candidate selection.
    pub fn intersect_dirty<'a>(
        &'a self,
        global_dirty: &'a GlobalDirtyBitset,
    ) -> impl Iterator<Item = GlobalEntityIndex> + 'a;
}
```

`intersect_dirty` is the inner loop of the per-user send phase:

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

**Maintenance:** The existing `update_entity_scopes` phase (already in `send_all_packets`) calls entity scope enter/exit callbacks. Wire these to `ConnectionVisibilityBitset::set`/`clear` on the relevant connection. Also: per-connection pause state (`paused_entities`) can be folded into the visibility bitset — a paused entity clears its bit, unpausing sets it back.

**Auth-level component filtering:** `is_component_updatable` is a per-component per-user check (auth state, spawn acknowledgment). This is NOT folded into the entity-level visibility bit — it remains a per-component check inside the per-user send loop. Its cost is negligible compared to the O(N²) scan it replaces.

---

## 7. Innovation 4: MutChannel Persistent Blob Cache

### Problem

`component.write_update(&diff_mask, writer, converter)` is called **twice per component per user** (counter pass + writer pass), each requiring an ECS archetype lookup. 32 users × 32 entities × 2 passes = **2,048 ECS reads per tick** in the benchmark.

Iris's "Poll and Copy" solution: maintain pre-serialized bytes for each component in the replication system itself. Read from these bytes at send time — zero ECS access.

### Where the Cache Lives

`MutChannel` is the natural home. It is:
- Already per `(GlobalEntity, ComponentKind)` — the exact granularity needed
- Already notified on every property mutation (via `MutChannelType::send()`)
- Already shared across all connections (via `Arc<RwLock<dyn MutChannelType>>`)

### MutChannelType Trait Extension

Add three methods to the `MutChannelType` trait:

```rust
pub trait MutChannelType: Send + Sync {
    // existing:
    fn new_receiver(&mut self, address: &Option<SocketAddr>) -> Option<MutReceiver>;
    fn send(&self, diff: u8);

    // NEW — blob cache:

    /// Returns the cached pre-serialized update for the given diff mask key, if valid.
    /// Returns None if the cache has been invalidated (component mutated since last build).
    fn get_cached_update(&self, diff_mask_key: u64) -> Option<CachedComponentUpdate>;

    /// Stores a newly-built cached update for the given diff mask key.
    /// Multiple keys can coexist (e.g. different users with different diff masks
    /// due to dropped-packet recovery).
    fn set_cached_update(&self, diff_mask_key: u64, update: CachedComponentUpdate);

    /// Clears ALL cached updates. Called automatically from send() on every mutation.
    /// After this call, next access per diff_mask_key will be a cache miss.
    fn clear_cached_updates(&self);
}
```

### Concrete Implementation

The default `MutChannelData` struct (the concrete impl of `MutChannelType`) gains:

```rust
struct MutChannelData {
    receivers: Vec<(Option<SocketAddr>, Arc<AtomicDiffMask>)>,  // existing
    // NEW:
    cached_updates: RwLock<HashMap<u64, CachedComponentUpdate>>,
}

impl MutChannelType for MutChannelData {
    fn send(&self, diff: u8) {
        // existing fan-out to receivers...
        for (_, mask) in &self.receivers { mask.mutate(diff); }
        // NEW: invalidate cached update on any mutation
        self.cached_updates.write().clear();
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

**Cache lifecycle:**
- **Invalidated:** automatically on every `MutChannel::send()` — the instant a property is mutated
- **Built:** lazily on first access after invalidation, by the first user that needs it for a given `diff_mask_key`
- **Reused:** by all subsequent users with the same `diff_mask_key` within the same tick AND across future ticks, until the next mutation
- **Cross-tick persistence:** a stable component (not mutated for T ticks) pays one serialization on the first post-mutation send, then zero serialization for all T-1 subsequent ticks

**Cache access in `GlobalDiffHandler`:**

The `GlobalDiffHandler` (which already owns the `MutReceiverBuilder`s, each holding a `MutChannel`) exposes the cache through its extended API:

```rust
impl<E: Copy> GlobalDiffHandler<E> {
    pub fn get_cached_update(
        &self,
        entity: &GlobalEntity,
        kind: &ComponentKind,
        key: u64,
    ) -> Option<CachedComponentUpdate> {
        self.mut_receiver_builders
            .get(&(*entity, *kind))
            .and_then(|b| b.channel().get_cached_update(key))
    }
    pub fn set_cached_update(
        &self,
        entity: &GlobalEntity,
        kind: &ComponentKind,
        key: u64,
        update: CachedComponentUpdate,
    ) {
        if let Some(b) = self.mut_receiver_builders.get(&(*entity, *kind)) {
            b.channel().set_cached_update(key, update);
        }
    }
}
```

(`MutReceiverBuilder` already holds a `MutChannel`; expose a `channel(&self) -> &MutChannel` accessor.)

---

## 8. Innovation 5: Two Principled Serialization Paths

### The Fundamental Distinction

Naia uses per-connection **LocalEntity** wire IDs for entity references — a deliberate design for privacy and scope semantics. Components with `EntityProperty` fields serialize different bytes per user (because the referenced entity's local wire ID differs per connection). Components with only `Property<T>` fields serialize identical bytes for all users.

This distinction is **type-level and semantic**, not an optimization caveat. It maps directly to Unreal Iris's distinction between `FReplicationFragment` (stateless bytes) and `FObjectReplicationFragment` (per-connection resolution). Both are principled, designed code paths:

- **Path A — UserIndependent**: component body bytes are identical for all users who have the same `DiffMask`. `CachedComponentUpdate` can be shared across all users and ticks. ECS reads at most once per mutation.
- **Path B — UserDependent**: component body bytes contain per-user local entity IDs. `CachedComponentUpdate` cannot be shared. ECS read once per tick (snapshot), serialized once per user.

### Compile-Time Detection

Add to the `Replicate` trait:

```rust
pub trait Replicate: Sync + Send + 'static + Named + Any {
    // ... existing methods ...

    /// Returns true if this component type contains one or more `EntityProperty` fields,
    /// meaning its serialized bytes differ per connection and cannot be cached as a
    /// shared CachedComponentUpdate.
    ///
    /// Default: false. The derive macro overrides to true for any component
    /// that has at least one EntityProperty field.
    fn has_entity_properties() -> bool where Self: Sized { false }
}
```

The derive macro at `shared/derive/src/replicate.rs` already distinguishes `EntityProperty` from `Property<T>` at codegen time (see the field kind detection at line 406). The commented-out `get_has_entity_properties_method` at line 1362 is prior art. Revive and expose as this method.

Generated for components with EntityProperty:
```rust
fn has_entity_properties() -> bool { true }
```

### ComponentKinds Storage

`ComponentKinds::add_component<C: Replicate>()` already registers component metadata. Add:

```rust
pub struct ComponentKinds {
    current_net_id: NetId,
    kind_bit_width: u8,
    kind_map: HashMap<ComponentKind, (NetId, Box<dyn ReplicateBuilder>, String)>,
    net_id_map: HashMap<NetId, ComponentKind>,
    // NEW:
    user_dependent: HashSet<ComponentKind>,  // components where has_entity_properties() == true
}

impl ComponentKinds {
    pub fn add_component<C: Replicate>(&mut self) {
        // ... existing registration ...
        if C::has_entity_properties() {
            self.user_dependent.insert(ComponentKind::of::<C>());
        }
    }
    pub fn is_user_dependent(&self, kind: &ComponentKind) -> bool {
        self.user_dependent.contains(kind)
    }
}
```

This is an O(1) `HashSet` lookup per component per write, with static (compile-time) detection.

---

## 9. New Serde Types

### 9.1 `CachedComponentUpdate`

Pre-serialized single-component body: `ComponentContinue=1 + ComponentKind + ComponentValue`.
Stored inline — no heap allocation. Persists in `MutChannel` across ticks until the component is mutated.

The name reflects its role: this is a **component update** that has been pre-serialized and **cached** for repeated wire transmission without re-reading ECS or re-running Serde. Compare with `PendingComponentUpdate` — the deserialized form of an incoming component update, transient, awaiting application to the live component — which lives on the receive path.

```rust
/// Pre-serialized component body. Inline array, zero heap allocation.
/// 64 bytes = 512 bits. Covers all reasonable single-component bodies
/// (NetworkedPosition ~60 bits, NetworkedVelocity ~60 bits).
/// Components whose serialized body exceeds 512 bits cannot use cached update storage;
/// they always use the per-user serialization path.
#[derive(Copy, Clone)]
pub struct CachedComponentUpdate {
    pub bytes: [u8; 64],
    pub bit_count: u32,
}

impl CachedComponentUpdate {
    /// Captures a BitWriter's content into a CachedComponentUpdate.
    /// Returns None if the content exceeds 64 bytes (512 bits).
    pub fn capture(writer: &BitWriter) -> Option<Self> {
        if writer.bits_written() > 512 { return None; }
        let raw = writer.bytes_written_slice();
        let mut bytes = [0u8; 64];
        bytes[..raw.len()].copy_from_slice(raw);
        Some(Self { bytes, bit_count: writer.bits_written() as u32 })
    }
}
```

### 9.2 `PendingComponentUpdate` — Receive Path Counterpart

On the receive path, an incoming component update is deserialized from the wire into a `PendingComponentUpdate` before being applied to the live component. This type already exists in naia as `ComponentUpdate` in `shared/src/world/update/component_update.rs`; it is renamed `PendingComponentUpdate` throughout to make the send/receive duality explicit:

- **`CachedComponentUpdate`** — send path, pre-serialized, cached, reused across users/ticks
- **`PendingComponentUpdate`** — receive path, deserialized from wire, transient, applied once

All existing usages of `ComponentUpdate` in `component_apply_update`, `component_apply_field_update`, and `WorldMutType` trait signatures are updated to `PendingComponentUpdate`.

### 9.3 `BitWriter` Extensions

`BitWriter` uses a `u32` scratch register (`scratch: u32`, `scratch_bits: u8`), LSB-first, little-endian word flush into a fixed `[u8; MTU_SIZE_BYTES]` buffer. Add:

```rust
impl BitWriter {
    /// Returns total bits written so far (before finalize).
    /// Exposes the currently private `current_bits` field.
    pub fn bits_written(&self) -> usize {
        self.current_bits as usize
    }

    /// Returns a slice of the bytes written so far (complete bytes only).
    /// Needed by CachedComponentUpdate::capture.
    pub fn bytes_written_slice(&self) -> &[u8] {
        &self.buffer[..self.byte_count]
    }

    /// Appends all bits from a CachedComponentUpdate at the current write position.
    /// Bit-accurate: handles arbitrary alignment (cached update may start mid-byte
    /// in the destination stream). Uses write_byte for full bytes plus
    /// write_bit for the partial last byte — identical output to re-serializing.
    pub fn append_cached_update(&mut self, update: &CachedComponentUpdate) {
        if update.bit_count == 0 { return; }
        let full_bytes = (update.bit_count / 8) as usize;
        let trailing   = update.bit_count % 8;
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

`BitCounter::count_bits(bits: u32)` already exists. Use for O(1) overflow check on cache hit:
`counter.count_bits(cached_update.bit_count)`.

### 9.4 `DiffMask::as_key`

```rust
impl DiffMask {
    /// Packs the mask into a u64 for use as a HashMap key in the cached update store.
    /// Supports diff masks up to 8 bytes (64 properties) — all current cyberlith
    /// components have 1-byte masks. Returns None for masks > 8 bytes;
    /// callers use per-user serialization without caching.
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
[reserved]  1 bit — ComponentContinue finish placeholder (reserve_bits(1) mechanism)
[1 bit]     UpdateContinue = true
[var bits]  OwnedLocalEntity::ser — per-user
            is_host(1) + is_static(1) + UnsignedVariableInteger::<7>(id)
            = 9 bits for IDs 0–127 (covers all avatar entities in 32-player bench)
--- CACHED UPDATE BOUNDARY (per component, not per entity) ---
  For each dirty component:
    [1 bit]   ComponentContinue = true
    [var]     ComponentKind::ser(component_kinds, writer)
    [var]     component.write_update(&diff_mask, writer, converter)
--- END CACHED UPDATE ---
[1 bit]     ComponentContinue = false (release_bits(1) then finish bit)
```

After all entities: `[1 bit] UpdateContinue = false`.

**Cache boundaries are per-component, not per-entity.** This preserves the existing partial-entity-send semantics (a 3-component entity where only 2 fit in the packet writes 2 and defers 1).

The per-user `OwnedLocalEntity` header (9 bits) is written directly to the packet stream before any cached update, and is not part of any cached update.

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

For each dirty entity: apply global (user-independent) facts once, then snapshot UserDependent components.

```rust
// Shared borrows — all released before Phase 3's exclusive per-user loop
let mut snapshot_map: SnapshotMap = HashMap::new();

for global_idx in dirty_entity_iter {
    let global_entity = self.global_diff_handler.global_entity(global_idx);
    let world_entity  = self.global_diff_handler.world_entity(global_idx);
    let comp_flags    = self.global_diff_handler.idx_to_components(global_idx);

    // Entity-level facts (checked once, not per-user):
    if !self.global_world_manager.entity_is_replicating(&global_entity) { continue; }
    if !world.has_entity(&world_entity) { continue; }

    for kind_bit in comp_flags.registered.iter_ones() {
        let component_kind = self.component_kinds.kind_for_net_id(kind_bit as u16);
        if !world.has_component_of_kind(&world_entity, &component_kind) { continue; }

        if comp_flags.user_dependent.get(kind_bit) {
            // PATH B: UserDependent — snapshot ECS once; per-user serialize with converter
            let snap = world.component_of_kind(&world_entity, &component_kind)
                .expect("component exists (verified above)")
                .copy_to_box();
            snapshot_map.insert((global_entity, component_kind), snap);
        }
        // PATH A: UserIndependent — CachedComponentUpdate already in MutChannel if component
        // is stable; cache miss handled lazily inside write_update on first user needing it.
    }
}
// All shared borrows on world, global_world_manager, global_diff_handler released here.
```

**No per-tick HashMap allocation for the dirty candidate union.** The global dirty set replaces the per-user union logic entirely.

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

        // Per-user checks (cannot be shared):
        // These use connection-local state: auth, spawn-ack, pause.
        for kind_bit in comp_flags.registered.iter_ones() {
            let component_kind = self.component_kinds.kind_for_net_id(kind_bit as u16);
            let local_converter = connection.base.world_manager.entity_converter();

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

    connection.send_packets(
        &self.component_kinds,
        &update_events,
        &snapshot_map,
        &self.global_diff_handler,
        &world,
        // ... remaining existing params ...
    );
}
```

### Phase 3 Inner: `write_update` with Two Paths

`write_update` now takes `snapshot_map`, `global_diff_handler`, and retains `world`/`world_entity` for PATH A cache misses only.

```rust
fn write_update<E: Copy + Eq + Hash + Send + Sync, W: WorldRefType<E>>(
    component_kinds: &ComponentKinds,
    now: &Instant,
    world: &W,
    global_diff_handler: &GlobalDiffHandler<E>,
    world_manager: &mut LocalWorldManager,
    packet_index: &PacketIndex,
    writer: &mut BitWriter,
    global_entity: &GlobalEntity,
    world_entity: &E,
    snapshot_map: &SnapshotMap,
    has_written: &mut bool,
    next_send_updates: &mut HashMap<GlobalEntity, HashSet<ComponentKind>>,
) {
    let mut written = Vec::new();
    let component_kind_set = next_send_updates.get(global_entity).unwrap();

    for component_kind in component_kind_set {
        let diff_mask = world_manager.get_diff_mask(global_entity, component_kind);

        if !component_kinds.is_user_dependent(component_kind) {
            // ── PATH A: UserIndependent ──────────────────────────────────────
            // Try cached update first; build on miss.

            let diff_mask_key = match diff_mask.as_key() {
                Some(k) => k,
                None => {
                    // Diff mask > 8 bytes: cannot key into cached update store.
                    // This is unreachable for all current cyberlith components.
                    panic!("Diff mask too large for cached update key: component {}",
                           component_kinds.kind_to_name(component_kind));
                }
            };

            let cached = match global_diff_handler.get_cached_update(global_entity, component_kind, diff_mask_key) {
                Some(cached) => cached,
                None => {
                    // Cache miss: one ECS read, one serialize, store in MutChannel.
                    // Converter is obtained but never called (UserIndependent invariant).
                    let mut converter = world_manager.entity_converter_mut(/* global_world_manager */);
                    let mut temp = BitWriter::with_max_capacity();
                    true.ser(&mut temp);
                    component_kind.ser(component_kinds, &mut temp);
                    world.component_of_kind(world_entity, component_kind)
                        .expect("component verified in Phase 2")
                        .write_update(&diff_mask, &mut temp, &mut converter);
                    match CachedComponentUpdate::capture(&temp) {
                        Some(cached) => {
                            global_diff_handler.set_cached_update(global_entity, component_kind, diff_mask_key, cached);
                            cached
                        }
                        None => {
                            // Component body > 512 bits — write directly, no caching.
                            // Unreachable for all current cyberlith components.
                            // (capture failed; temp bits already written; must re-serialize directly)
                            let mut counter = writer.counter();
                            counter.count_bits(temp.bits_written() as u32);
                            if counter.overflowed() { break; }
                            *has_written = true;
                            // Real path: re-serialize directly to writer.
                            // In practice this path does not exist for any registered cyberlith component.
                            continue;
                        }
                    }
                }
            };

            // Overflow check (O(1) — no ECS read)
            let mut counter = writer.counter();
            counter.count_bits(cached.bit_count);
            if counter.overflowed() {
                if !*has_written {
                    Self::warn_overflow_update(
                        component_kinds.kind_to_name(component_kind),
                        cached.bit_count,
                        writer.bits_free(),
                    );
                }
                break;
            }

            *has_written = true;
            writer.append_cached_update(&cached);

        } else {
            // ── PATH B: UserDependent ────────────────────────────────────────
            // EntityProperty fields require per-user local entity ID resolution.
            // ECS was read once in Phase 2 into snapshot_map.
            let snapshot = snapshot_map.get(&(*global_entity, *component_kind))
                .expect("UserDependent snapshot built in Phase 2");
            let mut converter = world_manager.entity_converter_mut(/* global_world_manager */);

            // Counter pass
            let mut counter = writer.counter();
            true.ser(&mut counter);
            component_kind.ser(component_kinds, &mut counter);
            snapshot.write_update(&diff_mask, &mut counter, &mut converter);
            if counter.overflowed() {
                if !*has_written {
                    Self::warn_overflow_update(
                        component_kinds.kind_to_name(component_kind),
                        counter.bits_needed(),
                        writer.bits_free(),
                    );
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

    // Drain written components from next_send_updates (unchanged logic)
    let update_kinds = next_send_updates.get_mut(global_entity).unwrap();
    for kind in &written { update_kinds.remove(kind); }
    if update_kinds.is_empty() { next_send_updates.remove(global_entity); }
}
```

**`type SnapshotMap = HashMap<(GlobalEntity, ComponentKind), Box<dyn Replicate>>;`** — defined in `world_writer.rs` or a shared types module.

---

## 12. Performance Projections

### 32 Players (Benchmark Scenario — All Moving Every Tick)

All avatar components are UserIndependent (NetworkedPosition, NetworkedVelocity — no EntityProperty). All dirty every tick (avatars move continuously) → cached updates are invalidated every tick by mutation.

| Operation | Before | After |
|---|---:|---:|
| ECS reads | 2,048 (counter + writer × 32 users × 32 entities) | **32** (one per entity for first user's cache miss) |
| Full Serde traversals | 1,024 | **32** (first user only) |
| Cached update copies (`append_cached_update`) | 0 | **992** (31 users × 32 components; ~9 bytes each) |
| Entity-level HashMap lookups | ~5,120 | **~160** (bitset intersection, Phase 2 filter) |
| `take_update_events` per-user cost | O(dirty_entities × components) | **O(visible_entities / 64)** bitset AND |

**Expected tick reduction:** `send_packet_loop` from 39.1% → ~8–10%; `take_update_events` from 25.8% → ~3–5%.

### 10,000 CCU (Target Scale — Mixed Stability)

Assuming 100 dirty entities/tick (10% of 1,000 visible), all UserIndependent, no drops:

| Operation | Before | After |
|---|---:|---:|
| ECS reads | 200,000 (100 components × 10,000 users × 2 passes) | **100** (one per dirty component, first user) |
| Serializations | 100,000 | **100** (first user) |
| Cached update copies | 0 | **999,900** (9,999 users × 100 components) |
| Cached update copy cost (9 bytes, ~7ns each) | — | **~7ms** |
| Old total (ECS + Serde at ~100ns each) | **~20ms** | ~7ms |

For stable components (not mutated since last tick): **0 ECS reads, 0 serializations** — cached updates from prior tick remain valid. This is the full Iris cross-tick persistence benefit.

---

## 13. Implementation Plan

Phases are ordered by dependency. Each phase has explicit prerequisites and a gate that must pass before the next begins. The full plan targets the complete final architecture with no deferred items.

### Phase 1 — Serde Layer Extensions

**No callers yet; pure additions.**

1. `DiffMask::as_key() -> Option<u64>` with unit tests
2. `CachedComponentUpdate { bytes: [u8; 64], bit_count: u32 }` + `CachedComponentUpdate::capture(writer: &BitWriter) -> Option<Self>`
3. `BitWriter::bits_written() -> usize` (expose `current_bits`)
4. `BitWriter::bytes_written_slice() -> &[u8]` (expose `buffer[..byte_count]`)
5. `BitWriter::append_cached_update(&mut self, update: &CachedComponentUpdate)` — aligned + trailing-bit paths
6. `BitCounter::count_bits` already exists — confirm behavior, add test

**Gate:** Unit tests — `append_cached_update(captured update)` round-trips at ALL bit alignments 0–63; empty update; full-byte update; trailing-bits-only update; 512-bit update fits; 513-bit returns `None`.

### Phase 2 — Derive Extension + ComponentKinds

1. `Replicate::has_entity_properties() -> bool` — default `false`
2. Derive macro: emit `fn has_entity_properties() -> bool { true }` for components with ≥1 `EntityProperty`
3. `ComponentKinds::user_dependent: HashSet<ComponentKind>` field
4. `ComponentKinds::add_component` stores the flag
5. `ComponentKinds::is_user_dependent(kind: &ComponentKind) -> bool`

**Gate:** `NetworkedPosition::has_entity_properties() == false`; any component with `EntityProperty` field `== true`. Check all existing cyberlith-registered components.

### Phase 3 — MutChannelType Cached Update Store

1. Add `get_cached_update`, `set_cached_update`, `clear_cached_updates` to `MutChannelType` trait with default `unimplemented!()` (to force all impls to update)
2. Add `cached_updates: RwLock<HashMap<u64, CachedComponentUpdate>>` to concrete `MutChannelData`
3. Implement the three new methods on `MutChannelData`
4. Wire `clear_cached_updates()` call into `MutChannelData::send()` after the existing fan-out
5. Expose `fn channel(&self) -> &MutChannel` on `MutReceiverBuilder`
6. Add `GlobalDiffHandler::get_cached_update` and `GlobalDiffHandler::set_cached_update` accessors

**Gate:** Unit test — mutate a component via `MutSender::mutate()`, confirm cached update clears; build cached update via `set_cached_update`, confirm `get_cached_update` returns it on next tick without mutation.

### Phase 4 — `PendingComponentUpdate` Rename

**Rename `ComponentUpdate` → `PendingComponentUpdate` throughout the receive path.**

1. Rename `ComponentUpdate` struct to `PendingComponentUpdate` in `shared/src/world/update/component_update.rs`
2. Update all usages in `WorldMutType::component_apply_update`, `component_apply_field_update`
3. Update all usages in `world_writer.rs`, `local_world_manager.rs`, and any other callers
4. Rename `LocalEntityIndex` type alias throughout `UserDiffHandler` (was `EntityIndex`) — `DirtyQueue` row index type becomes `GlobalEntityIndex` in later phases

**Gate:** `cargo check --workspace`; wasm32 check; E2E 93/93 green. No behavioral change — pure rename.

### Phase 5 — Two-Path write_update (Fix A)

**Depends on Phases 1, 2, 3. No structural send-loop changes yet — thread new params through existing call chain.**

1. Add `SnapshotMap = HashMap<(GlobalEntity, ComponentKind), Box<dyn Replicate>>` type alias
2. Modify `WorldWriter::write_update` signature — add `snapshot_map: &SnapshotMap`, `global_diff_handler: &GlobalDiffHandler<E>`
3. Implement PATH A (UserIndependent cached update) and PATH B (UserDependent snapshot) inside `write_update`
4. Thread `snapshot_map` and `global_diff_handler` through:
   - `WorldWriter::write_updates`
   - `WorldWriter::write_into_packet`
   - `Connection::send_packet`
   - `Connection::send_packets`
5. In `WorldServer::send_all_packets`: build `snapshot_map` for UserDependent dirty components before the per-user loop (Phase 2 proto — uses existing `take_outgoing_events` result to find dirty entities; `GlobalEntityIndex` not yet needed)

**Gate:** E2E harness 93/93 green. Sub-phase bench: `send_packet_loop` measurably reduced from 39.1% baseline.

### Phase 6 — GlobalEntityIndex + GlobalDiffHandler Extension

**Structural refactor. Depends on Phase 5 passing.**

1. `GlobalEntityIndex(u32)` type with INVALID sentinel
2. Extend `GlobalDiffHandler<E>` with dense entity registry fields and operations (see Section 4)
3. Wire `alloc_entity`/`free_entity` into existing entity spawn/despawn paths (`host_spawn_entity`, `despawn_entity`)
4. Wire component registration (`register_component` in `GlobalDiffHandler`) to call `register_component` on the new dense tables
5. Replace `UserDiffHandler::entity_to_index / index_to_entity` (`LocalEntityIndex` tables) with lookups into `GlobalDiffHandler` — per-user `DirtyQueue` row index becomes `GlobalEntityIndex` (global, not per-user)

**Note:** `DirtyQueue::push(entity_idx: LocalEntityIndex, kind_bit: u16)` changes to `push(entity_idx: GlobalEntityIndex, kind_bit: u16)`. Verify `DirtyQueue::stride` remains correct (based on component kind count, unchanged).

**Gate:** E2E harness 93/93; integration tests 39/39; naia harness 127/127.

### Phase 7 — GlobalDirtyBitset

**Depends on Phase 6.**

1. `GlobalDirtyBitset` struct with ref-counted strided bitset
2. `GlobalDirtyBitset::increment / decrement / dirty_entity_iter / dirty_words`
3. Extend `DirtyNotifier` — add `global: Weak<GlobalDirtyBitset>` field; change `entity_idx` type from `LocalEntityIndex` to `GlobalEntityIndex`
4. `DirtyNotifier::notify_dirty` calls `global.increment`; `notify_clean` calls `global.decrement`
5. Update `MutChannel::new_channel` and `GlobalDiffHandler::register_component` to wire `GlobalDirtyBitset` into new `DirtyNotifier`s
6. Add `global_dirty: Arc<GlobalDirtyBitset>` to `WorldServer`

**Gate:** Unit test — mutate component for 32 users; confirm `dirty_entity_iter` yields the entity; mark all users clean; confirm entity absent from iterator. E2E 93/93.

### Phase 8 — ConnectionVisibilityBitset

**Depends on Phase 6 (`GlobalEntityIndex`).**

1. `ConnectionVisibilityBitset` struct with `Vec<u64>`, `set/clear/is_set/intersect_dirty`
2. Add `visibility: ConnectionVisibilityBitset` to `Connection`
3. Wire `set/clear` into existing scope enter/exit paths in `update_entity_scopes`
4. Wire entity pause state into visibility bit clearing (paused entity clears its bit)

**Gate:** E2E 93/93. Verify visibility bitset matches current HashMap-based scope state for all 93 tests.

### Phase 9 — New Send Loop (Fix B, Final Form)

**Depends on Phases 6, 7, 8. The full Iris three-phase send loop.**

1. Replace `take_outgoing_events` call per-user with:
   - Phase 1: `global_dirty.dirty_entity_iter()` — global dirty candidate set
   - Phase 2: entity-level filter loop + SnapshotMap build (no per-user iteration)
   - Phase 3: per-user `visibility.intersect_dirty(&global_dirty)` → per-user candidate set → per-user checks → `update_events`
2. Remove `EntityAndGlobalEntityConverter<E>` param from `write_updates` (entity-level converter no longer needed in writer — `world_entity` lookup moves to Phase 2)
3. Remove `world: &W` from `write_updates` (ECS access in `write_update` is now PATH A cache-miss only; thread `world` through but only to `write_update` where it may still be needed for PATH A misses)
4. Replace old `LocalWorldManager::take_update_events` call with new per-user filter over `update_events` from Phase 3

**Gate:** E2E 93/93; integration 39/39; naia harness 127/127. Sub-phase bench: `take_update_events` from 25.8% → <5%; `send_packet_loop` from 39.1% → <10%.

### Phase 10 — Benchmark Re-run and Documentation

1. Full bench run: `cargo run --features bench_profile -p cyberlith_bench --release -- --scenario game_server_tick --warmup 100 --ticks 500`
2. Sub-phase breakdown recorded in `cyberlith/_AGENTS/CAPACITY_RESULTS.md`
3. Comparison against 32-player baseline (39.1% + 25.8%)
4. Project to 10,000 CCU based on measured scaling

---

## 14. Correctness Invariants

**Wire format unchanged.** `append_cached_update` produces bit-identical output to direct `Serde::ser` — the receiver's deserialization path is unaffected. Cache correctness: unit test in Phase 1 gate.

**Per-user diff mask independence.** The cached update key includes the diff mask (`diff_mask_key: u64`). Users with different diff masks (possible after dropped-packet recovery causes OR of extra bits) get different cache keys → independent `CachedComponentUpdate` entries → correct per-user serialization. `record_update` is unchanged and runs per-user per-component regardless of cache hit/miss.

**Cache invalidation on mutation.** `MutChannel::send()` calls `clear_cached_updates()` synchronously before returning. A mutated component's `CachedComponentUpdate` is invalid by the time any send-path code could observe it. No tick-boundary race.

**Partial entity sends preserved.** `write_update`'s `break` on overflow still works: PATH A cached update `count_bits` may overflow → `break`; PATH B counter pass may overflow → `break`. Component deferred to next packet identically to today.

**UserDependent snapshot timing.** Snapshots are built in Phase 2, before the per-user loop (Phase 3). The snapshot reflects component state at the moment Phase 2 runs — the same state the old code would have read during per-user `write_update`. No behavioral change.

**EntityProperty correctness in PATH B.** The snapshot holds the component's internal state (GlobalEntity references in EntityProperty fields). Per-user `write_update` on the snapshot uses the per-user converter to produce per-user local entity wire IDs — identical behavior to today's ECS-direct path.

**GlobalEntityIndex stability.** A `GlobalEntityIndex` is valid for the lifetime of its entity (allocated at spawn, freed at despawn). Despawn zeroes the registry slot in `GlobalDiffHandler` and pushes the index to the free list. Any in-flight send that holds a stale `global_idx` (possible in theory between Phase 2 and Phase 3 within one tick) is protected by Phase 2's `has_entity` check — despawned entities fail that check before any snapshot is taken.

**GlobalDirtyBitset consistency.** The ref-count approach (increment on dirty-notify, decrement on clean-notify) ensures the entity summary bit is set iff any user has a non-clear diff mask for that entity. The existing `DirtyNotifier::notify_dirty / notify_clean` are the sole update sites — no other code path can change per-user dirty state without going through `DirtyNotifier`.

**ConnectionVisibilityBitset consistency.** All entity scope enter/exit paths (spawn, despawn, room join/leave, UserScope include/exclude) must call `visibility.set/clear`. An audit of all call sites in `update_entity_scopes` and `LocalWorldManager` is required at Phase 8 gate.

**`record_update` unchanged.** Tracks sent diff masks for drop-recovery (ORs dropped mask back in). This is per-user, per-component, and unchanged by this refactor. It must continue to run after every component write regardless of cache hit/miss.

---

## 15. Migration Compatibility

**Public API:** No changes to `#[derive(Replicate)]`, `Property<T>`, entity spawn/despawn, rooms, user scope, channels, or any user-facing server/client API. All changes are internal to naia's replication pipeline.

**Protocol / Wire Format:** Unchanged. Receiver-side deserialization is unaffected.

**Naia branching policy:** All implementation work on `dev-trunk`. No commits to `main`. Gate passes before merge per established policy.

**`wasm32-unknown-unknown`:** Serde layer changes (Phase 1) affect `naia-shared/serde` which is used by both native and wasm client builds. Verify `cargo run -p automation_cli -- check-wasm` passes after Phase 1.

**Test suite:** E2E harness (93 tests), integration harness (39 tests), naia harness (127 tests) must stay green throughout. Each phase gate specifies which suites to run.

---

## 16. Open Questions

**Q1: `CachedComponentUpdate` size ceiling.** 64 bytes (512 bits) chosen to cover all current cyberlith components. If a future component exceeds this, PATH A's `capture()` returns `None` and the code currently panics in the spec. Decision needed: should overflow fall back to per-user serialization silently, or should large components be a hard registration-time error?

Recommendation: registration-time check — `ComponentKinds::add_component<C>` measures `C::max_bit_length()` (add this to `Replicate` trait or derive a const) and panics if > 512 bits. Forces explicit component slimming before registration.

**Q2: `GlobalDirtyBitset` capacity growth.** The bitset is sized at server startup based on `max_replicated_entities`. If entities are spawned beyond initial capacity, the bitset must grow. Growing an `AtomicU64` vec requires locking. Options: (a) pre-allocate generously (`ServerConfig::max_replicated_entities`, default 65,536); (b) lock on resize (rare event).

**Q3: `ConnectionVisibilityBitset` and per-component auth.** The visibility bitset is entity-level. Per-component auth checks (`is_component_updatable`) remain in the Phase 3 per-user loop. These are currently ~O(1) per component but add a branch per entity×component. Profile after Phase 9 to determine if component-level visibility bits are warranted.

**Q4: PATH A cache miss and `world`/`world_entity` in `write_updates`.** After Phase 9, `write_updates` loses the entity-level converter (`EntityAndGlobalEntityConverter<E>`) but still needs `world: &W` for PATH A cache misses. This means `write_updates` retains its `W` generic parameter. If all PATH A cache misses are eliminated in practice (stable components in prod), consider whether a future step can remove world from write_updates entirely by pre-building all blobs in Phase 2.

**Q5: `RwLock<HashMap<u64, CachedComponentUpdate>>` contention in MutChannel.** The cached update store uses a `RwLock` inside `MutChannelData`. Most ticks, cached updates are only read (Phase 3 per-user). Write contention only on cache misses (first user after mutation). If profiling shows `RwLock` write contention at high CCU, consider a sharded approach or `DashMap`.

**Q6: Quantization.** ChatGPT's overview correctly identifies quantization as the next major bandwidth win after this refactor. `NetworkedPosition` storing raw `f32` bits wastes ~16 bits per component that could be quantized. This is future work; the `CachedComponentUpdate` infrastructure introduced here naturally accommodates quantized components (the cached update stores quantized bytes, same path).
