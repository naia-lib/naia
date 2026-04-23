# Scaling Naia to 10K+ Replicated Entities per Room

**Status:** Proposed — 2026-04-23
**Motivation:** cyberlith levels want `NetworkedTile` per tile (10K+ tiles per level), preserving the existing entity API (`commands.spawn((NetworkedTile, ...))`, `ReplicationConfig::Delegated`, `Property<T>`, authority handoff). Naia's per-tick work is currently proportional to *entities-in-scope*, not *entities-that-changed*; beyond ~1K entities this becomes the dominant server cost.

**Constraints:**
- Zero breakage of the existing user API. Existing apps compile and behave identically.
- Tiles-as-entities stays viable. No substrate rewrite. No "move tiles off ECS."
- Bevy archetype tables remain the storage; Naia's per-client metadata layer is where the surgery happens.

---

## 1. Investigation summary — what Naia actually does today

I traced the mutation path, scope-update path, update-send path, and wire format. What I found:

### 1.1 Mutation path is already push-based

Not a poll loop. Flow:

- `Property::deref_mut` (`shared/src/world/component/property.rs:330`) calls `mutate()`.
- `HostOwnedProperty::mutate` (property.rs:379) calls `PropertyMutator::mutate(index)`.
- `MutChannelData::send` (`server/src/world/mut_channel.rs:31`) iterates the component's per-client `receiver_map: HashMap<SocketAddr, MutReceiver>` and sets the dirty bit in each user's `DiffMask`.

Conclusion: the *per-property dirty bit* is already push-driven into per-client state. There is no global scan. This is good news and corrects prior assumptions.

### 1.2 Where per-entity-proportional cost actually lives

Three places scan per-entity-per-tick regardless of whether anything changed:

| # | Location | Cost |
|---|---|---|
| **A** | `WorldServer::update_entity_scopes` — `server/src/server/world_server.rs:2619-2740` | For each room, for each user, iterates `room.entities()` fully every tick. 10K entities × 8 users = 80K lookups per tick against `entity_scope_map`, `has_global_entity`, `entity_is_public_and_owned_by_user`. |
| **B** | `HostWorldManager::get_updatable_world` — `shared/src/world/host/host_world_manager.rs:162-184` | Per client per tick, iterates every host-world entity channel, cross-referenced against delivered-world, to build a candidate `HashMap<GlobalEntity, HashSet<ComponentKind>>`. `EntityUpdateManager::take_outgoing_events` (`shared/src/world/update/entity_update_manager.rs:37-66`) then filters by `diff_mask_is_clear`. For 10K mostly-idle entities, this map is built and mostly-filtered-empty every tick. |
| **C** | Spawn burst at scope entry — `host_world_manager.rs:115-137` via `init_entity_send_host_commands` | Per-entity `EntityCommand::Spawn` + one `EntityCommand::InsertComponent` per component, each a separate reliable-channel message. 10K entities with 1 component = ~20K reliable messages. |

### 1.3 Scope-exit behavior is unconditionally destructive

`world_server.rs:2735`: when an entity leaves a user's scope, `connection.base.world_manager.despawn_entity(global_entity)` runs unconditionally. The client destroys the entity. On re-entry, Naia does a full re-spawn + re-insert-component + full initial-state write. There is no "pause updates, keep entity on client" path.

For tiles this is the wrong default. For a future fog-of-war mechanic it's actively harmful (clients would lose tile content every time a unit leaves the area, rather than keeping it as fog).

### 1.4 Wire format is already tight on the payload side

Outgoing packet structure (`base_connection.rs:151-193`, `world_writer.rs:30-70`):

- **Standard header** (ack manager).
- **Messages** (reliable + unreliable channels).
- **Updates** (`write_updates`): for each dirty `GlobalEntity`: 1 continue bit + `LocalEntity` (`bool + UnsignedVariableInteger<7>`) + foreach dirty component: 1 continue bit + `ComponentKind` (`ConstBitLength` — positional, not name-based) + diff-mask-gated payload (only dirty property bits written).
- **Commands** (`write_commands`): foreach queued `EntityCommand`: CommandId (indexed delta) + `EntityMessageType` tag + payload.

Observations:
- `LocalEntity` uses a varint — small IDs pack to 1 byte. No fat headers.
- `ComponentKind` uses const bit length at schema-registration time — no type-name strings on the wire.
- `DiffMask` is per-component bit array; only dirty property bits are written. Unchanged fields cost nothing.
- **The payload wire format is already quite good.** There isn't much fat to trim on update packets.

The one real wire inefficiency is **level-load framing overhead**: for 10K entities, each gets a separate `Spawn` reliable message AND a separate `InsertComponent` reliable message. That's 20K reliable-channel entries with their own CommandId deltas, type tags, and ack-tracking records. The per-message framing dwarfs the per-entity payload at level load.

---

## 2. Proposal — four targeted changes

### Win 1 — Refactor `ReplicationConfig` into a struct; add `scope_exit` behavior

**Current:**

```rust
// server/src/world/replication_config.rs
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ReplicationConfig {
    Private,
    Public,
    Delegated,
}
```

**Proposed:**

```rust
// server/src/world/replication_config.rs
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Publicity {
    Private,
    Public,
    Delegated,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub enum ScopeExit {
    #[default]
    Despawn,   // current behavior
    Persist,   // keep entity + last-known state on client; pause updates
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct ReplicationConfig {
    pub publicity: Publicity,
    pub scope_exit: ScopeExit,
    // room for future fields without breaking the API
}

impl ReplicationConfig {
    pub const fn private() -> Self {
        Self { publicity: Publicity::Private, scope_exit: ScopeExit::Despawn }
    }
    pub const fn public() -> Self {
        Self { publicity: Publicity::Public, scope_exit: ScopeExit::Despawn }
    }
    pub const fn delegated() -> Self {
        Self { publicity: Publicity::Delegated, scope_exit: ScopeExit::Despawn }
    }
    pub const fn with_scope_exit(mut self, exit: ScopeExit) -> Self {
        self.scope_exit = exit; self
    }
    pub const fn persist_on_scope_exit(self) -> Self {
        self.with_scope_exit(ScopeExit::Persist)
    }
}
```

**User-facing ergonomics** (preserves existing code):

```rust
// Old code — still compiles via a Deprecated-hinting re-export path if we want,
// or just update call sites (there are ~20 across the repo):
commands.spawn((NetworkedTile::new(tx, ty, *tile),))
    .configure_replication(ReplicationConfig::delegated().persist_on_scope_exit());
```

**Call sites to update** (enumerated by `grep -rn "ReplicationConfig" naia/`):

- `server/src/world/replication_config.rs` — definition.
- `server/src/world/global_entity_record.rs:17-19` — default construction, switch `Public` / `Private` to struct form.
- `server/src/world/global_world_manager.rs:201,207,219,261,270,274,302` — field-access the `.publicity` instead of raw enum match.
- `server/src/world/entity_mut.rs:68,75` — API surface.
- `server/src/world/entity_ref.rs:35` — API surface.
- `server/src/server/world_server.rs:817-911,1379-1386` — `configure_entity_replication` state machine and scope-check in `user_scope_has_entity` need `.publicity` access.
- `adapters/bevy/server/src/commands.rs:16,54` and `adapters/bevy/server/src/server.rs:126,438` — `ConfigureReplicationCommand`, bevy `CommandsExt`.
- `adapters/bevy/client/src/commands.rs:19,77,145,150` and `adapters/bevy/client/src/client.rs:19,167` — client-side API.

**Wire-out scope-exit behavior** (coupled with Win 1 because that's where the config comes from):

At `world_server.rs:2735`, change:

```rust
} else if currently_in_scope {
    connection.base.world_manager.despawn_entity(global_entity);
}
```

to:

```rust
} else if currently_in_scope {
    let scope_exit = self.global_world_manager
        .entity_replication_config(global_entity)
        .map(|c| c.scope_exit)
        .unwrap_or(ScopeExit::Despawn);
    match scope_exit {
        ScopeExit::Despawn => {
            connection.base.world_manager.despawn_entity(global_entity);
        }
        ScopeExit::Persist => {
            connection.base.world_manager.pause_entity_for_user(global_entity);
            // New API (to add): stop sending updates for this entity,
            // but keep LocalEntity, HostEntityChannel, diff-mask receiver,
            // and client-side materialization intact. See §2.1 below.
        }
    }
}
```

#### 2.1 New `LocalWorldManager::pause_entity_for_user` and resume path

For `Persist`, introduce a per-user-per-entity "paused" flag on the `HostEntityChannel`:

- When paused: `get_updatable_world` skips this entity; `despawn_entity` is not sent.
- The `MutReceiver` continues to accumulate dirty bits in the DiffMask as Property mutations occur — so when the user re-enters scope, we know exactly which properties changed while they were away.
- On scope re-entry for a paused entity, do NOT `host_init_entity`. Instead: clear the paused flag. Next `write_packet` naturally includes any accumulated diff. If DiffMask is clean (nothing changed during absence), **zero bytes sent** — the client's local state is already correct.

This is exactly the dspr `fogAmount > 0` discovered-set behavior, but keyed off per-user-per-entity paused state that's already straightforward to track next to the existing `HostEntityChannel`.

**Edge cases:**
- Entity actually despawned on server while user was paused → on resume, send Despawn (existing infra).
- Component added/removed while user was paused → tracked existing path for InsertComponent/RemoveComponent replay; send those commands on resume. The reliable-channel already handles this ordering.
- User disconnects entirely while paused → use existing `remove_user` cleanup.

---

### Win 2 — Push-based scope-change tracking

`update_entity_scopes` scans all (room, user, entity) every tick. Scope state only changes when:

1. `UserScope::include()` / `exclude()` is called (`user_scope.rs:37-50`).
2. A room's membership (user or entity) changes.
3. An entity spawns or despawns.
4. An entity's replication config changes (affects `user_scope_has_entity` decision).

All four are explicit API calls.

**Proposal:** maintain a `scope_change_queue: VecDeque<ScopeChange>` on `WorldServer`:

```rust
enum ScopeChange {
    EntityAdded(UserKey, GlobalEntity),       // need to re-evaluate inclusion
    EntityRemoved(UserKey, GlobalEntity),     // need to despawn/pause
    UserAddedToRoom(UserKey, RoomKey),        // expand evaluation to that user's room entities
    UserRemovedFromRoom(UserKey, RoomKey),
    RoomEntityAdded(RoomKey, GlobalEntity),   // expand to that room's users
    RoomEntityRemoved(RoomKey, GlobalEntity),
    ScopeToggled(UserKey, GlobalEntity, bool),// via UserScope::include/exclude
}
```

Populate the queue at each API entry point (~8 functions in `world_server.rs`). Rooms already have a `entity_removal_queue` (world_server.rs:2621) — this generalizes the same pattern.

Replace the body of `update_entity_scopes` with a drain-the-queue loop: for each change, compute *only the affected (user, entity) pairs* and apply the spawn/despawn/pause decision. Room size does not factor into the cost.

**Idle-room tick cost:** O(1) (empty queue). 10K tiles in a static-scope editor = zero scope-update work per tick.

**Change-heavy tick cost:** O(changes) — no worse than current worst case.

**Correctness:** the current behavior (`user_scope_has_entity` derivation) is preserved exactly; we just evaluate it lazily instead of per-tick-eagerly.

---

### Win 3 — Push-based update candidate set

`HostWorldManager::get_updatable_world` (host_world_manager.rs:162-184) builds a fresh `HashMap<GlobalEntity, HashSet<ComponentKind>>` every tick by iterating every host-world entity. `EntityUpdateManager::take_outgoing_events` (entity_update_manager.rs:37-66) then filters by `diff_mask_is_clear`. For 10K entities with 1 dirty, we build a 10K-entry map and filter down to 1.

We already know at the exact moment a mutation happens (`MutChannelData::send`). Extend that path to push into a per-client dirty set:

**Changes:**

1. Add to `MutChannelData` (or somewhere it can reach per-user state): for each `receiver_map` entry, in addition to calling `receiver.mutate(property_index)`, also insert `(GlobalEntity, ComponentKind)` into a per-user `dirty_components: HashSet`. The `GlobalEntity`/`ComponentKind` are already known at the component-register site (where `MutReceiver` is created — `global_diff_handler.rs` gives `receiver(address, entity, component_kind)`), so we can stash them in the receiver or an adjacent struct.

2. Replace `get_updatable_world` + `take_outgoing_events` with a single `drain_dirty_updates(&mut self) -> HashMap<GlobalEntity, HashSet<ComponentKind>>` that drains and returns the per-client dirty set.

3. Validate membership (entity still replicates, component still present, diff-mask still dirty) at drain time — these are the same checks currently done in `take_outgoing_events`.

**Idle tick cost:** dirty set is empty → `drain_dirty_updates` returns an empty map → the update-write branch is skipped entirely. Zero work for 10K idle tiles.

**Mutation tick cost:** O(mutations). One hash insert per mutation (already O(1) per receiver iteration — marginal cost above the existing DiffMask bit-set).

**Together with Win 2:** for an editor room with 10K tiles and 50 active units, server per-tick CPU is proportional to ~50 + recent-scope-changes, not 10K.

---

### Win 4 — Coalesced spawn+insert on scope entry

`init_entity_send_host_commands` (host_world_manager.rs:115-137) sends:

1. One `EntityCommand::Spawn` reliable message.
2. One `EntityCommand::InsertComponent` reliable message per component in the entity's initial component set.

Each reliable message carries its own `CommandId` delta, its own `EntityMessageType` tag, its own ack-tracking record. For 10K tiles × 1 component: 20K reliable-channel messages at level load, where the *payload* for each is trivial but the *framing* adds up.

**Proposal:** add a new `EntityCommand::SpawnWithComponents(global_entity, component_kinds)` and matching `EntityMessageType::SpawnWithComponents`. Wire format:

```
CommandId delta
EntityMessageType::SpawnWithComponents tag
HostEntity (varint)
u8 component_count
[ComponentKind + write(...) payload]×component_count
```

One reliable message replaces (1 + K) messages. Halves (or better) the reliable-message count at level load.

Dispatch logic in `init_entity_send_host_commands`: if `component_kinds.len() >= 1` (i.e., always, for spawn at scope entry), emit the combined command instead of separate Spawn + N InsertComponent commands.

Receiver (`WorldReader::read_world_events` and downstream `RemoteEngine`): add a handler that splits it back into `spawn` + `insert_component×N` events for the client's ECS integration. The client-side API (`EntityEvent`) stays unchanged.

**Preserves everything else:** reliable channel, ack tracking, the existing `InsertComponent` path for later-added components, the existing `Spawn` path for entities that spawn without any components immediately. This is purely a coalescing optimization for the common case.

**Not implementing:**
- Bulk spawn across entities (one message = N entities). Connor ruled out chunking. Per-entity coalescing only.
- Schema-precompiled wire codec. Naia already uses `ConstBitLength` for kinds and `DiffMask` for properties — the payload wire format is already near the optimum for a bit-packed dynamic protocol. Further compression would need entity-specific knowledge that Naia can't reason about generically.

---

### Win 5 — Immutable replicated components (per-component cost floor)

**Motivation.** Today every replicated component — even ones whose fields never change after initial sync — pays full diff-tracking overhead:

For each `(GlobalEntity, ComponentKind)`, `GlobalWorldManager::insert_component_diff_handler` (`server/src/world/global_world_manager.rs:130-141`) allocates a `MutChannel` (Arc<RwLock<dyn MutChannelType>>) + `MutReceiverBuilder`, and registers it in `GlobalDiffHandler.mut_receiver_builders` (`shared/src/world/update/global_diff_handler.rs:7-49`). Every user that has the entity in scope then allocates a `MutReceiver` (Arc<RwLock<DiffMask>>) in `UserDiffHandler.receivers` (`shared/src/world/update/user_diff_handler.rs:16,29-54`) and an entry in that component's `MutChannelData.receiver_map`. Inside the component, *each* `Property<T>` field carries a cloned `PropertyMutator` (one Arc per field) so that `deref_mut` can push into the channel.

Each replicated `Property<T>` field also emits, via the derive macro, contributions to `diff_mask_size`, `set_mutator`, `write_update`, and the per-field dirty-bit branches (`shared/derive/src/replicate.rs:73-189`).

For 10K tiles × 1 component × N users, that's 10K Arc<RwLock<DiffMask>> instances server-wide plus 10K × N per-user receiver allocations. For a 3-field Tile component, that's another 30K per-field `PropertyMutator` Arc clones. None of it is exercised if the tile never mutates.

**Observation.** cyberlith tiles are written once (on level load or editor placement) and never mutated afterward — destruction is a despawn, not a mutation. The same pattern applies broadly: unit-class descriptors, monster-type tags, item kinds, static terrain species, any "set-once" metadata.

**Bevy already has exactly this concept.** Bevy 0.15+ added `Component::Mutability`:

```rust
// bevy_ecs/src/component/mod.rs:514-519
pub trait Component: ... {
    /// ... Mutable components will have Component<Mutability = Mutable>,
    /// while immutable components will instead have Component<Mutability = Immutable>.
    type Mutability: ComponentMutability;
}

#[derive(Component)]
#[component(immutable)]
struct TileKind(u8);  // Bevy refuses to hand out &mut via queries
```

Bevy guarantees "never have an exclusive reference `&mut ...` created while inserted onto an entity" (bevy_ecs mod.rs:670-672). The restriction is compile-time enforced.

Naia's `ReplicatedComponent` trait currently *requires* `Mutable`:

```rust
// shared/src/world/component/replicate.rs:124-128
pub trait ReplicatedComponent: Replicate + Component<Mutability = Mutable> {}
impl<T: Replicate + Component<Mutability = Mutable>> ReplicatedComponent for T {}
```

This is the single line standing between us and a trivially correct "immutable replicated component" path.

**Proposal.**

1. **Relax `ReplicatedComponent`** to accept either mutability:
   ```rust
   pub trait ReplicatedComponent: Replicate + Component {}
   impl<T: Replicate + Component> ReplicatedComponent for T {}
   ```

2. **Add a single const on `Replicate`** that the derive macro sets based on the Bevy `Mutability` (or a `#[replicate(immutable)]` attr for non-Bevy users):
   ```rust
   pub trait Replicate: Sync + Send + 'static + Named + Any {
       const IS_IMMUTABLE: bool = false;
       // ... (existing trait methods unchanged)
   }
   ```

3. **Derive macro** (`shared/derive/src/replicate.rs`):
   - If the struct is immutable (Bevy `Component<Mutability = Immutable>` or `#[replicate(immutable)]`), generate `const IS_IMMUTABLE: bool = true`.
   - Forbid `Property<T>` fields in immutable components at macro-expansion time (compile error: "immutable Replicate cannot hold Property<T> — use plain T"). Fields are plain `T: Serde`.
   - Generated `diff_mask_size() → 0`, `set_mutator(_) → { /* no-op */ }`, `write_update(...) → { panic!("immutable") }` (never called on the hot path — see point 4).
   - Generated `write(...)` still serializes all fields for initial-state sync. This is the only wire path the component participates in.

4. **Hot-path branches** (the actual savings):
   - `GlobalWorldManager::insert_component_diff_handler` (`global_world_manager.rs:130`) — early return if `T::IS_IMMUTABLE`. No `MutChannel`, no `MutReceiverBuilder`, no entry in `GlobalDiffHandler`. `set_mutator` is never called.
   - `HostWorldManager::init_entity_send_host_commands` (`host_world_manager.rs:115-137`) — don't call `entity_update_manager.register_component` for immutable kinds. `UserDiffHandler.receivers` never gets an entry for them.
   - Win 3's `drain_dirty_updates` — immutable components never enter the dirty set by construction (no `MutChannel::send` path exists for them).
   - `HostEntityChannel::component_kinds()` still includes immutable components so the initial `Spawn` + `InsertComponent` (or Win 4's `SpawnWithComponents`) carries them on the wire. `write()` serializes each field once.

5. **Client-side.** Unchanged. Immutable components arrive via the standard read path — `new_read` constructs them from `BitReader::de()` for each field. Because the macro forbade `Property<T>` in immutable components, there's no `RemoteOwnedProperty`/`DelegatedProperty` machinery to route through — the read path is straight `T::de()`.

6. **Forbidden combinations** (caught at derive time):
   - `immutable` + `Property<T>` field → compile error.
   - `immutable` + `EntityProperty` field → compile error (would require diff tracking for relation resolution).
   - `immutable` + `ReplicationConfig::Delegated` at runtime → runtime panic or error event. Delegation implies "transfer authority to mutate," which is meaningless for an immutable component. Configuration-validation step in `configure_entity_replication`.

**Quantified impact (10K tiles, 1 immutable component per tile, 8 users):**

| Allocation | Mutable (current) | Immutable |
|---|---|---|
| `GlobalDiffHandler.mut_receiver_builders` | 10K entries + 10K Arc<RwLock<dyn MutChannelType>> | 0 |
| `UserDiffHandler.receivers` (×8 users) | 80K entries + 80K Arc<RwLock<DiffMask>> | 0 |
| `MutChannelData.receiver_map` | 80K entries | 0 |
| Per-field `PropertyMutator` Arc clones (3-field tile) | 30K | 0 |
| `take_outgoing_events` `diff_mask_is_clear` reads per tick | 10K (pre-Win-3) / 0 (post-Win-3) | 0 (always) |

Win 5 is **orthogonal and compounding** with Win 3: Win 3 eliminates *scanning* the full entity world for dirty components; Win 5 eliminates the *allocation and registration* overhead for components that can never be dirty. A room with 10K immutable-tile entities and 50 mutable-unit entities:

- Post-Win-3 only: 10K × 8 users = 80K MutReceiver allocations (never used), but per-tick CPU is proportional to ~50 dirty units.
- Post-Win-3 + Win-5: ~50 MutReceiver allocations total (only on units), per-tick CPU unchanged.

**Ergonomics.** User opts in via standard Bevy derive syntax:

```rust
#[derive(Component, Replicate)]
#[component(immutable)]
pub struct NetworkedTile {
    pub x: i32,
    pub y: i32,
    pub fill: TileFill,  // plain T, not Property<T>
}

// cyberlith usage — no API change at the call site
commands.spawn((
    NetworkedTile { x: tx, y: ty, fill },
    Transform::from_xyz(...),
)).configure_replication(
    ReplicationConfig::public().persist_on_scope_exit()
);
```

Compile-time enforcement:
- Bevy refuses `&mut NetworkedTile` via queries (Bevy's guarantee).
- Naia derive refuses `Property<T>` fields in the component (our guarantee).
- To mutate a tile, the user removes + re-inserts a new `NetworkedTile` component (or despawns + respawns the entity). This matches dspr's actual tile lifecycle.

**Stays compatible.** Components that do need mutability (unit position, HP, animation state) keep `Property<T>` + `Mutable` and flow through the existing mutation path unchanged. The derive macro picks the right generation path per struct; the user never thinks about the two cases except via the one attribute.

**Not implementing:**
- A fallback "interior `Property<T>` but with a `freeze()` method that locks mutation" — less explicit, less Bevy-aligned, doesn't save the per-field Arc allocation. The immutable-at-the-type-level design is strictly better.
- Per-instance immutability (same component type is sometimes mutable, sometimes not). The per-type model is simpler, matches Bevy's, and covers every use case we can think of.

---

## 3. Implementation phases

Each phase is an independent PR, builds on the previous, and can ship on its own.

### Phase 1 — `ReplicationConfig` refactor + `ScopeExit::Persist` (Win 1)

**Scope:**
- `server/src/world/replication_config.rs` — struct definition, constants, builder API.
- Update ~20 call sites (enumerated in §2 above).
- Add `LocalWorldManager::pause_entity_for_user` and `resume_entity_for_user`.
- Extend `HostEntityChannel` with a per-channel paused flag.
- Branch the scope-exit path in `world_server.rs:2735`.
- Adapter surface (`adapters/bevy/server/src/commands.rs`, `client/src/commands.rs`) — add `.persist_on_scope_exit()` builder forwarding.
- Gherkin spec additions in `test/specs/features/` covering: (a) default despawn-on-exit unchanged, (b) persist-on-exit keeps entity on client, (c) re-entry with no mutations is zero-byte, (d) re-entry with mutations sends diff only, (e) server-despawn during paused propagates correctly on resume.

**Acceptance:**
- All existing tests pass (no semantic change for entities configured without `Persist`).
- New tests for Persist behavior pass.
- cyberlith can flag tile entities with `.persist_on_scope_exit()` and observe no despawn/respawn churn on scope changes.

### Phase 2 — Push-based scope-change tracking (Win 2)

**Scope:**
- Add `scope_change_queue` and `ScopeChange` enum to `WorldServer`.
- Populate at API entry points: `UserScope::include/exclude`, `room_add_user`, `room_remove_user`, `room_add_entity`, `room_remove_entity`, entity spawn/despawn paths, `configure_entity_replication` when it changes `publicity`.
- Rewrite `update_entity_scopes` to drain the queue.
- Preserve the `entity_removal_queue` behavior or absorb it into the new queue.

**Acceptance:**
- Existing scope-change specs pass unchanged.
- New benchmark in `test/harness/`: room with 10K entities, scope-static tick → per-tick wall-time in `update_entity_scopes` is within noise of zero.

### Phase 3 — Push-based update candidate set (Win 3)

**Scope:**
- Extend `MutChannelData::send` / `MutReceiver` to also push `(GlobalEntity, ComponentKind)` into a per-client `dirty_components: HashSet`. The simplest path: have each `MutReceiver` hold a reference (Arc/RwLock) to its owning user's dirty-set, analogous to the existing diff-mask reference topology.
- Add `EntityUpdateManager::drain_dirty_updates` returning the map currently produced by `get_updatable_world` + `take_outgoing_events`.
- Remove `HostWorldManager::get_updatable_world` full scan; callers use the drained set.
- Keep the existing membership/diff-clear sanity checks at drain time.

**Acceptance:**
- All update-path specs pass unchanged.
- Benchmark: 10K-entity room with no mutations → per-tick wall-time in update write path is near zero. One mutation → one entity's update flows, no others touched.

### Phase 4 — `SpawnWithComponents` coalesced command (Win 4)

**Scope:**
- Add `EntityCommand::SpawnWithComponents` and `EntityMessageType::SpawnWithComponents`.
- Update `WorldWriter::write_command` to serialize the combined form.
- Update `init_entity_send_host_commands` to emit the combined command when `component_kinds.len() > 0`.
- Update `WorldReader` / remote handlers to decode into the existing spawn + insert_component event stream.
- Preserve the existing `EntityCommand::Spawn` for the (rare) case of spawning without initial components.

**Acceptance:**
- All existing spec tests pass — client observes identical `EntityEvent::SpawnEntity` + `EntityEvent::InsertComponent` sequence.
- Benchmark: 10K-entity scope-entry burst — reliable message count roughly halves; wire bytes drop by the sum of per-message framing overhead (CommandId deltas + type tags + ack records).

### Phase 5 — Immutable replicated components (Win 5)

**Scope:**
- `shared/src/world/component/replicate.rs` — add `const IS_IMMUTABLE: bool = false` to the `Replicate` trait. Relax `ReplicatedComponent` to accept `Component<Mutability = _>`.
- `shared/derive/src/replicate.rs` — detect Bevy `Component<Mutability = Immutable>` (and/or `#[replicate(immutable)]` attr). Emit `const IS_IMMUTABLE: bool = true`. Forbid `Property<T>` / `EntityProperty` fields in immutable components. Generate trivial `diff_mask_size`, `set_mutator`, `write_update` impls. Keep plain-`T`-serializing `write` / `new_read`.
- `server/src/world/global_world_manager.rs:130-141` — early return in `insert_component_diff_handler` when `T::IS_IMMUTABLE`.
- `shared/src/world/host/host_world_manager.rs:115-137` — skip `entity_update_manager.register_component` for immutable kinds.
- `server/src/server/world_server.rs` — validate at `configure_entity_replication` time: reject `Delegated` + any-immutable-component combinations.

**Acceptance:**
- All existing specs pass (mutable components unchanged).
- New spec in `test/specs/features/`: immutable component spawns, round-trips initial values, verified to *never* appear in the per-tick update stream.
- Benchmark: 10K-tile room with one `#[component(immutable)]` tile component — `GlobalDiffHandler.mut_receiver_builders.len()` and `UserDiffHandler.receivers.len()` both stay at zero for tile kinds.
- cyberlith `NetworkedTile` compiles with `#[component(immutable)]` and plain fields; no call-site changes beyond removing the `Property<T>` wrapper inside the struct.

---

## 4. Expected impact for cyberlith

After Phases 1-3:

| Scenario | Current | Post-refactor |
|---|---|---|
| 10K-tile level loaded, steady state | ~10ms/tick CPU on scope+update scans | ~microseconds (proportional to mutations) |
| Tile edit in level editor | O(1) (unchanged, already push) | O(1) (unchanged) |
| Scope change (user moves camera, camera-culled scope) | O(entities in room × users) per tick | O(changes) |
| Scope re-entry for a tile that hasn't changed | Full Spawn + InsertComponent + value write | Zero bytes (Phase 1, `Persist`) |
| Future fog-of-war for tiles | Requires custom tile-streaming layer | Falls out of Phase 1 — just set `persist_on_scope_exit` |

After Phase 4:

| Scenario | Current | Post-refactor |
|---|---|---|
| Level load with 10K tiles | ~20K reliable messages | ~10K reliable messages, lower framing overhead |

After Phase 5:

| Scenario | Current (or post-Phase-3) | Post-refactor |
|---|---|---|
| Per-tile server-side allocation (1 component, 8 users) | 9 Arc<RwLock<DiffMask>> + 9 HashMap entries | 0 |
| 10K-tile level, per-tick DiffMask read-locks | 10K (pre-Win-3) / 0 (post-Win-3, but allocations remain) | 0 allocations, 0 read-locks |
| Adding a new immutable component to an entity | O(users) allocations + mutator wiring | O(1), no per-user work |

Phases 1–3 unlock the 10K-tile target. Phase 4 trims level-load framing. Phase 5 eliminates the per-component floor cost entirely for components that don't need mutation — which, for cyberlith tiles specifically, is all of them.

---

## 5. Notes on what this is NOT

- **Not a substrate rewrite.** Entities remain Bevy entities. Archetype tables remain the storage. `Query<&NetworkedTile>` continues to work exactly as today.
- **Not chunk-grained authority.** Authority stays per-entity via the existing `Delegated` state machine.
- **Not a new wire format.** Phase 4 adds one new `EntityCommand` variant; the bit-packing, varint, and DiffMask conventions are unchanged.
- **Not a CRDT or event-log replication model.** Ordering and authority semantics are identical to today.
- **Not tied to any cyberlith code change.** All four wins are internal Naia improvements. cyberlith benefits automatically once Phase 1 lands and it opts its tile entities into `Persist`.

---

## 6. Open questions to resolve during implementation

1. **Phase 3 plumbing.** `MutChannelData::send` runs on the mutation thread (can be Bevy's world thread); the per-client `dirty_components` set must be accessible from both sides. The existing `MutReceiver` already uses `Arc<RwLock<DiffMask>>` (see `user_diff_handler.rs:69-73`), so the pattern is established — piggyback.
2. **`ScopeExit::Persist` interaction with `Delegated`.** If a persisted-and-delegated entity loses scope while a user holds authority, does authority get released (current behavior in `user_scope_set_entity:1351-1366`)? Probably yes — scope-loss should still drop authority; the entity just isn't despawned. Confirm during Phase 1 implementation.
3. **Phase 4 backward compatibility.** `SpawnWithComponents` is a new `EntityMessageType` variant. Clients on an older protocol version wouldn't recognize it. Naia already has a protocol ID check at handshake (`shared/src/protocol_id.rs`); bumping that is the standard path. Confirm whether the spec-test harness enforces a protocol-version bump for new message types.
4. **Phase 5 + delegation.** Specified as "runtime error" above, but a softer alternative is: allow `Delegated` on immutable components and have it mean "authority over spawn/despawn only" (since there's nothing to mutate). Decide during Phase 5 implementation based on whether any real use case exists for "delegated immutable."
5. **Phase 5 + non-Bevy backends.** The primary signal is Bevy's `Mutability` type param. For the non-Bevy shared path (`#[cfg(not(feature = "bevy_support"))]`), the derive macro needs `#[replicate(immutable)]` as an explicit opt-in. Straightforward but must be covered by tests in both configurations.

---

## 7. Out-of-scope for this plan

- **`GenerativeSubstrate`** (seed + overlay deterministic regen). Interesting long-term; not justified until procedural content is an established cyberlith feature.
- **Spatial rooms as a primitive.** Connor clarified rooms are intended as coarse first-filter, not spatial. Build spatial helpers on top of `UserScope::include/exclude` via user code (e.g., a cyberlith-side quadtree that drives `include`/`exclude` calls). Phase 2 makes that strategy cheap.
- **`MostlyImmutable` as a per-entity config.** Phase 3 makes it universal (all entities are zero-cost-when-idle by construction), so no per-entity flag is needed.
