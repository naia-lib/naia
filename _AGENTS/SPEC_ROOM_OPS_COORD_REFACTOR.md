# SPEC — Room-Ops Coord Refactor (cyberlith MISSION_SIM_OWNS_WORLD D.5cd prereq)

**Status:** Design · **Created:** 2026-05-18 · **Audience:** anyone implementing the naia-side prereq for cyberlith D.5cd

## 0. Problem

cyberlith's `AssetScopeMachine` needs `room_*` ops (`create_room`, `room_mut.add_user / add_entity / remove_entity / destroy`) to execute from the Recv SubApp via `CoordHandle`. A.3's deviation note (`pipeline_handles.rs` ~L241) states the design intent: "`RoomStore` stays on `CoordinatorState`; cyberlith reaches room state via `CoordHandle` on the Recv SubApp."

The current implementation doesn't honor that intent. Every `WorldServer::room_*` body mutates **both** `CoordinatorState.{room_store, user_store}` AND `SendState.{entity_room_map, scope_checks_cache}` directly. There's no Coord-only path. Building one as a parallel API (`CoordRoomMut` alongside `RoomMut`) was considered and rejected — the elegant answer is to unify on one contract.

## 1. The single contract

> **Room mutations push to `scope_change_queue`. The drainer is the single canonical updater of `entity_room_map` + `scope_checks_cache`.**

Two callers, same contract:

- **`WorldServer::room_*`** (in-process; holds both halves). Pushes the change variant, then drains the queue immediately against `self.send.{entity_room_map, scope_checks_cache}`. Same-call readers (e.g. a follow-on `user_scope_has_entity` in the same closure) see consistent state — behavior unchanged from today.
- **`CoordHandle::room_*`** (Coord-only; doesn't see `SendState`). Pushes the change variant, doesn't drain. Send's `send_all_packets` drains the queue at its top before any read of `entity_room_map` / `scope_checks_cache`.

No double-write: the only writer to `SendState` room-projection fields is the drainer. No parallel API: `room_store::*` methods themselves become "Coord-only" — they touch only `rooms`, `user_store`, and return / push the `ScopeChange` events. The drainer is one new function reachable from both call sites.

## 2. Code shape

### 2.1 Extended `ScopeChange` enum

```rust
// naia/server/src/server/scope_change.rs
pub(crate) enum ScopeChange<E: Copy + Eq + Hash + Send + Sync> {
    // ── Existing variants (unchanged shape) ─────────────────────────
    UserEnteredRoom(UserKey, RoomKey),
    UserLeftRoom(UserKey, RoomKey),
    EntityEnteredRoom(GlobalEntity, RoomKey),
    ScopeToggled(UserKey, GlobalEntity, bool),

    // ── NEW variants carrying the data Send needs to update
    // entity_room_map + scope_checks_cache ──────────────────────────
    //
    // `entities_in_room` / `users_in_room` are snapshots taken on the
    // Coord side at push time so the drainer doesn't need to re-query
    // room_store (which is on the Coord half and may have moved on).
    RoomChange(RoomChange<E>),
}

pub(crate) enum RoomChange<E: Copy + Eq + Hash + Send + Sync> {
    /// User added to a room — Send must add (room, user, entity) tuples
    /// to scope_checks_cache for every entity already in the room.
    UserAdded { room_key: RoomKey, user_key: UserKey, entities_in_room: Vec<E> },

    /// User removed from a room — Send drops all (room, user, *) tuples.
    UserRemoved { room_key: RoomKey, user_key: UserKey },

    /// Entity added to a room — Send must:
    ///   1. entity_room_map.entity_add_room(&global_entity, &room_key)
    ///   2. scope_checks_cache.on_entity_added_to_room(room_key,
    ///      world_entity, users_in_room)
    EntityAdded {
        room_key: RoomKey,
        world_entity: E,
        global_entity: GlobalEntity,
        users_in_room: Vec<UserKey>,
    },

    /// Entity removed from a room — Send must:
    ///   1. entity_room_map.remove_from_room(&global_entity, &room_key)
    ///   2. scope_checks_cache.on_entity_removed_from_room(room_key,
    ///      world_entity)
    EntityRemoved {
        room_key: RoomKey,
        world_entity: E,
        global_entity: GlobalEntity,
    },

    /// Room destroyed — Send must:
    ///   1. for each (world_entity, global_entity) in removed_entities:
    ///        entity_room_map.remove_from_room(&global_entity, &room_key)
    ///   2. scope_checks_cache.on_room_destroyed(room_key)
    RoomDestroyed {
        room_key: RoomKey,
        removed_entities: Vec<(E, GlobalEntity)>,
    },
}
```

The existing `ScopeChange::UserEnteredRoom` / `UserLeftRoom` / `EntityEnteredRoom` variants are kept (other naia internals already drain them in `drain_scope_change_queue` for per-user scope evaluation). The new `RoomChange` payloads are **additional** — they carry the SendState-projection data the queue didn't carry before.

### 2.2 `room_store::*` methods become Coord-only

The room_store methods stop taking `&mut EntityRoomMap` / `&mut ScopeChecksCache` parameters. They mutate only `self.rooms` + `user_store`, and return `(legacy ScopeChange, RoomChange<E>)` so the caller (`WorldServer` or `CoordHandle`) can push both onto `scope_change_queue`.

```rust
// naia/server/src/server/room_store.rs (after refactor)
impl RoomStore {
    pub(super) fn add_user<E>(
        &mut self,
        room_key: &RoomKey,
        user_key: &UserKey,
        user_store: &mut UserStore,
        entity_map: &GlobalEntityMap<E>,
    ) -> (ScopeChange<E>, RoomChange<E>) {
        // ... existing room/user mutation logic ...
        let entities_in_room: Vec<E> = self.rooms.get(room_key)
            .map(|r| r.entities()
                .filter_map(|ge| entity_map.global_entity_to_entity(ge).ok())
                .collect())
            .unwrap_or_default();
        (
            ScopeChange::UserEnteredRoom(*user_key, *room_key),
            RoomChange::UserAdded { room_key: *room_key, user_key: *user_key, entities_in_room },
        )
    }
    // ... same shape for remove_user / add_entity / remove_entity / destroy ...
}
```

`room_store` no longer depends on `EntityRoomMap` or `ScopeChecksCache` at the import level — pure Coord-state mutator.

### 2.3 The single drainer

```rust
// naia/server/src/server/send_state.rs (or new module
//                                       naia/server/src/server/room_drainer.rs)
impl<E: Copy + Eq + Hash + Send + Sync> SendState<E> {
    /// Apply pending `RoomChange` events from `scope_change_queue` to
    /// `entity_room_map` + `scope_checks_cache`.
    ///
    /// Must be called before any read of those two structures (i.e. at
    /// the top of `send_all_packets`, before `drain_scope_change_queue`
    /// fires per-user scope re-evaluation).
    ///
    /// Idempotent: re-call drains an empty queue.
    ///
    /// **Note:** `WorldServer::room_*` callers invoke this immediately
    /// after pushing, so same-call readers see consistent state. The
    /// Send-thread `send_all_packets` invokes it once per send pass to
    /// catch any pushes from `CoordHandle::room_*` (Recv-side) that
    /// landed between ticks.
    pub(crate) fn apply_pending_room_changes(
        &mut self,
        scope_change_queue: &Mutex<VecDeque<ScopeChange<E>>>,
    ) {
        let drained: Vec<RoomChange<E>> = {
            let mut q = scope_change_queue.lock();
            let mut out = Vec::with_capacity(q.len());
            // Iterate in order; keep non-RoomChange variants in the queue
            // for drain_scope_change_queue to consume later.
            let mut keep = VecDeque::with_capacity(q.len());
            for change in q.drain(..) {
                if let ScopeChange::RoomChange(rc) = change {
                    out.push(rc);
                } else {
                    keep.push_back(change);
                }
            }
            *q = keep;
            out
        };

        for change in drained {
            match change {
                RoomChange::UserAdded { room_key, user_key, entities_in_room } => {
                    self.scope_checks_cache.on_user_added_to_room(
                        room_key, user_key, entities_in_room);
                }
                RoomChange::UserRemoved { room_key, user_key } => {
                    self.scope_checks_cache.on_user_removed_from_room(
                        room_key, user_key);
                }
                RoomChange::EntityAdded { room_key, world_entity, global_entity, users_in_room } => {
                    self.entity_room_map.entity_add_room(&global_entity, &room_key);
                    self.scope_checks_cache.on_entity_added_to_room(
                        room_key, world_entity, users_in_room);
                }
                RoomChange::EntityRemoved { room_key, world_entity, global_entity } => {
                    self.entity_room_map.remove_from_room(&global_entity, &room_key);
                    self.scope_checks_cache.on_entity_removed_from_room(
                        room_key, world_entity);
                }
                RoomChange::RoomDestroyed { room_key, removed_entities } => {
                    for (_world_entity, global_entity) in &removed_entities {
                        self.entity_room_map.remove_from_room(global_entity, &room_key);
                    }
                    self.scope_checks_cache.on_room_destroyed(room_key);
                }
            }
        }
    }
}
```

### 2.4 `WorldServer::room_*` (in-process callers — drain immediately)

```rust
// naia/server/src/server/world_server.rs (after refactor)
pub(crate) fn room_add_user(&mut self, room_key: &RoomKey, user_key: &UserKey) {
    let (legacy_change, room_change) = {
        let entity_map = self.shared.global_entity_map.read();
        self.coord.room_store.add_user(
            room_key, user_key, &mut self.coord.user_store, &*entity_map)
    };
    {
        let mut q = self.shared.scope_change_queue.lock();
        q.push_back(legacy_change);
        q.push_back(ScopeChange::RoomChange(room_change));
    }
    // Drain immediately so same-call readers (e.g. `user_scope_has_entity`
    // chained off this method) see consistent state.
    self.send.apply_pending_room_changes(&self.shared.scope_change_queue);
}
// ... same shape for room_remove_user / room_add_entity / room_remove_entity
//     / room_destroy ...
```

### 2.5 `CoordHandle::room_*` (Coord-only callers — push, no drain)

```rust
// naia/server/src/pipeline_actors/handles.rs (additions)
impl<E: Copy + Eq + Hash + Send + Sync> CoordHandle<E> {
    pub fn create_room(&mut self) -> RoomKey {
        let room_key = self.state.room_store.insert(Room::new());
        room_key
    }

    pub fn room_destroy(&mut self, room_key: &RoomKey) -> bool {
        let (legacy_change_opt, room_change) = {
            let entity_map = self.shared.global_entity_map.read();
            self.state.room_store.destroy_coord(
                room_key, &mut self.state.user_store, &*entity_map)
        };
        if let Some(rc) = room_change {
            let mut q = self.shared.scope_change_queue.lock();
            if let Some(legacy) = legacy_change_opt {
                q.push_back(legacy);
            }
            q.push_back(ScopeChange::RoomChange(rc));
        }
        // No drain — Send drains in send_all_packets.
        room_change.is_some()
    }

    pub fn room_add_user(&mut self, room_key: &RoomKey, user_key: &UserKey) { /* push, no drain */ }
    pub fn room_remove_user(&mut self, room_key: &RoomKey, user_key: &UserKey) { /* push, no drain */ }
    pub fn room_add_entity(&mut self, room_key: &RoomKey, world_entity: &E) { /* push, no drain */ }
    pub fn room_remove_entity(&mut self, room_key: &RoomKey, world_entity: &E) { /* push, no drain */ }
}
```

### 2.6 `send_all_packets` calls the drainer once at the top

```rust
// naia/server/src/server/send_state.rs::send_all_packets (or wherever the
// canonical send entry point is)
pub fn send_all_packets<W: WorldRefType<E> + Sync>(&mut self, world: W) {
    // NEW: drain any CoordHandle::room_* pushes that landed between ticks.
    self.apply_pending_room_changes(&self.shared.scope_change_queue);
    // ... existing send pipeline (drain_scope_change_queue for per-user
    //     scope evaluation, then per-user serialization) ...
}
```

### 2.7 Convenience: no separate `CoordRoomMut` struct

The user's elegance constraint: no `CoordRoomMut` alongside `RoomMut`. Two options:

- **Option α (chosen for this spec):** `CoordHandle` exposes flat methods (`room_add_user(rk, uk)`, etc.) instead of a chained-builder. cyberlith's `AssetScopeMachine` rewrite to flat-call form is mechanical (~30 LOC of grep-replace). The chained-builder ergonomics that `RoomMut` provides aren't load-bearing for the cyberlith call sites.

- **Option β (alternative; not chosen):** Move `RoomMut` itself to be generic over a `RoomMutBackend` trait, with `WorldServer` and `CoordHandle` both implementing it. More code, more abstract, gives back the chained-builder. Not worth it for the small number of call sites.

If a future need surfaces, option β can be retrofitted without breaking option α's flat methods (they delegate to the same `room_store::*_coord` paths).

## 3. Method-by-method refactor checklist

| Existing method | After refactor | Notes |
|---|---|---|
| `room_store::add_user(rk, uk, user_store, entity_map, cache)` | `add_user(rk, uk, user_store, entity_map) -> (ScopeChange, RoomChange)` | Drops `cache` param; returns the deferred `RoomChange::UserAdded`. |
| `room_store::remove_user(rk, uk, user_store, cache)` | `remove_user(rk, uk, user_store) -> (ScopeChange, RoomChange)` | Drops `cache` param. |
| `room_store::add_entity(rk, e, entity_map, entity_room_map, cache)` | `add_entity(rk, e, entity_map) -> Option<(ScopeChange, RoomChange)>` | Drops `entity_room_map` + `cache` params. Returns `None` if room missing. |
| `room_store::remove_entity(rk, e, entity_map, entity_room_map, cache)` | `remove_entity(rk, e, entity_map) -> Option<(ScopeChange, RoomChange)>` | Drops `entity_room_map` + `cache` params. |
| `room_store::destroy(rk, user_store, entity_room_map, cache) -> bool` | `destroy(rk, user_store, entity_map) -> (bool, Option<RoomChange>)` | Drops `entity_room_map` + `cache` params. Collects `(E, GlobalEntity)` snapshot for the drainer's `RoomDestroyed`. |
| `room_store::remove_all_entities(rk, entity_room_map)` | `remove_all_entities(rk) -> Vec<GlobalEntity>` | Internal helper for `destroy`; just collects + drops from rooms. Drainer updates entity_room_map via `RoomDestroyed`. |
| `WorldServer::room_*` (5 methods) | All push to queue + drain immediately | Behavior identical to today from any caller's perspective. |
| `CoordHandle::create_room / room_destroy / room_add_user / room_remove_user / room_add_entity / room_remove_entity` | NEW: push to queue, no drain | The flat-method API. |
| `SendState::apply_pending_room_changes(queue)` | NEW | Single drainer. Idempotent. |
| `SendState::send_all_packets` | Adds one line: `self.apply_pending_room_changes(...)` at top | Catches `CoordHandle` pushes between ticks. |

## 4. Backward-compat surface

### 4.1 No public API removal

- `WorldServer::room_mut(&RoomKey)` keeps returning `RoomMut<'_, E>` (the chained-builder API). `RoomMut::add_user`/`add_entity`/etc. keep delegating into `WorldServer::room_*_*`, which keep working — push + drain immediately.
- Init-time cyberlith paths (`send_startup` etc. that use `with_world_server_subapps` to spawn tile entities and put them in rooms) require zero changes.
- All existing naia user-facing examples in `_AGENTS/NAIA_BOOK_PLAN.md` keep compiling and behaving identically.

### 4.2 No semantic change for existing callers

The drainer is idempotent and order-preserving. `WorldServer::room_add_entity` callers see the same observable state after the call as before the refactor — just routed through a queue→drain instead of direct mutation. Same-call reads (`user_scope_has_entity` after `room_add_entity` in one closure) work as before.

The one semantic shift: callers of `CoordHandle::room_*` see DEFERRED state — Send's drainer applies on next `send_all_packets`. No existing caller has this concern (the new `CoordHandle::room_*` methods are added in this refactor; no pre-existing callers).

### 4.3 No wire-format change

The refactor is internal to the server's `WorldServer` + `CoordinatorState` + `SendState` separation. No bit changes on the network wire. Honors cyberlith memory `feedback_no_wire_format_change`.

## 5. Test surface

### 5.1 Workspace tests (must remain clean)

`cargo test --workspace` — covers:
- `naia-server` unit tests (room_store, scope_checks_cache, scope_change drainer).
- `naia-test-harness` integration tests.
- `naia-bevy-server` adapter tests.

### 5.2 namako_integration_test (must remain 10/10)

`cargo test -p naia-npa --test namako_integration_test` — coverage/baseline verification. Adding to ScopeChange enum + new methods on CoordHandle WILL trigger namako's baseline-drift detection. Update the namako baseline AS PART OF this PR (one expected manifest change). Don't try to land separately.

### 5.3 New unit tests (added in this PR)

In `naia/server/src/server/scope_checks_cache.rs` test module, add:

- `apply_pending_room_changes_replays_user_added` — push a UserAdded RoomChange, drain, verify cache state equals direct on_user_added_to_room call.
- `apply_pending_room_changes_replays_entity_added` — same for EntityAdded against both entity_room_map + cache.
- `apply_pending_room_changes_handles_room_destroyed` — RoomDestroyed cleans up entity_room_map for every entity + cache for the room.
- `apply_pending_room_changes_is_idempotent_on_empty_queue` — second call no-ops.
- `apply_pending_room_changes_preserves_non_RoomChange_variants` — drain leaves UserEnteredRoom / EntityEnteredRoom / ScopeToggled in the queue.
- `world_server_room_ops_observe_same_state_post_refactor` — for each WorldServer::room_* method, mutate via the new path, then read entity_room_map + scope_checks_cache; assert state equals the pre-refactor direct-mutation expected value (regression catch).

### 5.4 New integration test

In `naia/test/tests/`, add a single test that exercises the CoordHandle path:
- Construct a 3-handle pipeline via `spawn_server_handles`.
- Call `coord_handle.create_room()`, `coord_handle.room_add_user(rk, uk)`, `coord_handle.room_add_entity(rk, entity)`.
- Trigger a send pass.
- Assert send state's `entity_room_map.entity_get_rooms(entity)` contains `rk`, and `scope_checks_cache.pending_slice()` contains `(rk, uk, entity)`.
- Verifies the deferred-then-drained path works end-to-end.

### 5.5 cyberlith e2e regression (post-naia-land verification)

After naia lands and cyberlith bumps to the new SHA: `cargo test -p cyberlith_test_harness --test e2e --release` should remain 93/93 across default + desync_detection variants. No cyberlith code changes; verifies backward compat holds for the existing `WorldServer::room_*` callers (`send_startup`, init-time room creation, etc.).

## 6. Commit decomposition

One naia commit, atomic. The refactor isn't safely splittable — changing room_store's signature breaks every caller until they're updated together.

```
naia: room-ops coord-refactor — room_store mutates only Coord state;
RoomChange variants extend ScopeChange; SendState drainer is the
single canonical updater of entity_room_map + scope_checks_cache;
CoordHandle gains create_room / room_destroy / room_add_user /
room_remove_user / room_add_entity / room_remove_entity flat methods

* room_store::{add_user, remove_user, add_entity, remove_entity,
  destroy, remove_all_entities} signatures: drop entity_room_map +
  scope_checks_cache params; return (ScopeChange, RoomChange) or
  Option thereof.
* ScopeChange gains a RoomChange(RoomChange<E>) variant.
* RoomChange enum: UserAdded/Removed, EntityAdded/Removed,
  RoomDestroyed — payloads carry the snapshots Send needs.
* SendState::apply_pending_room_changes — single drainer; idempotent;
  preserves non-RoomChange queue entries.
* SendState::send_all_packets — calls drainer at top.
* WorldServer::room_add_user/remove_user/add_entity/remove_entity/
  destroy — push (legacy, RoomChange) onto queue + drain immediately
  (same-call consistency preserved).
* CoordHandle gains the six flat methods — push without drain.

cyberlith MISSION_SIM_OWNS_WORLD D.5cd consumes the CoordHandle path;
no cyberlith change in this commit.

Gates:
  cargo test --workspace                            clean
  cargo test -p naia-npa --test namako_integration_test   10/10
  (baseline manifest updated for new public surface — same commit)
  cyberlith e2e default + desync_detection         93/93 (post-bump)
```

## 7. Risk catalog

| Risk | Mitigation |
|---|---|
| `apply_pending_room_changes` ordering: a `UserAdded { entities_in_room: [...] }` queued at coord time references entities that may have been added/removed by later queue entries. | Snapshot at push time is intentional — Coord's view at push time is the source of truth. Send's drainer replays in order. The `entities_in_room` field captures the room's state at the moment the user joined, which matches the pre-refactor semantics (`cache.on_user_added_to_room` was called inside the same `WorldServer` reassembly that snapshotted the room). |
| Mutex contention on `scope_change_queue.lock()` for the partition-into-RoomChange step. | Drainer's lock-hold is `Vec::drain` + iterate + rebuild — bounded by queue size. For cyberlith's traffic (`scope_change_queue` averages 0-50 entries per send pass), no measurable contention. |
| namako baseline drift detection breaks CI for the PR. | Update the baseline in the same commit. Standard naia pattern (B.7b did this; see SHA 5fc8a09d). |
| `RoomChange` variant generic over `E` makes `ScopeChange<E>` generic too (currently un-genericized). | Tolerable — `ScopeChange` is `pub(crate)`, no external API surface. The queue type changes from `Mutex<VecDeque<ScopeChange>>` to `Mutex<VecDeque<ScopeChange<E>>>` — a one-line change at `ServerShared`. |
| Future regression: a contributor adds a new `WorldServer::room_*` method that mutates `entity_room_map` directly. | Add a `#[deprecated(note="use SendState::apply_pending_room_changes via scope_change_queue")]` on `EntityRoomMap`'s direct mutation methods (`entity_add_room`, `remove_from_room`, `remove_from_all_rooms`) for non-`apply_pending_room_changes` callers. Actually: make them `pub(super)` if not already, and verify the only call site is `apply_pending_room_changes`. |

## 8. Cyberlith follow-up (out of scope for this PR — sequenced in MISSION_SIM_OWNS_WORLD D.5cd)

After this naia commit lands and cyberlith bumps to the new SHA, cyberlith adds:
- `services/game/cell/src/send_systems/asset_room_ops.rs` (Recv-side; consumes a cross-SubApp wire driven by `AssetScopeManager` on Send; calls `coord_handle.room_*` directly).
- Deletes cyberlith's reliance on `WorldServer::room_mut` for per-user asset rooms.
- (Init-time `send_startup` keeps using `WorldServer::room_mut` via `with_world_server_on_send_world` — that's the init-only path the spec keeps.)

The naia refactor is necessary for cyberlith's D.5cd; it's also useful in isolation as the proper realization of A.3's "RoomStore stays on CoordinatorState" intent.

---

**End of spec.** Implementation order: read this doc → implement § 2 in order (enum → room_store → drainer → WorldServer wrappers → CoordHandle additions → send_all_packets hook) → land § 5 tests → run § 5.1-5.4 gates → update namako baseline → commit per § 6 template → push → bump cyberlith to the new SHA → run § 5.5.
