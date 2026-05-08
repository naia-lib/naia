# Naia API Cleanup Plan

**Created:** 2026-05-08  
**Scope:** Naming consistency and method ergonomics only. Excludes D-P7 (Replicate decomp) and D-P12 (architectural refactors).  
**Status:** Awaiting implementation scheduling.

---

## Approved changes

### P-API-1 — `ReplicationConfig` is two different types  
**Priority: High**

The name `ReplicationConfig` exists in both crates but refers to fundamentally different things:

- **Server** (`server/src/world/replication_config.rs`): a struct with builder methods, two-axis configuration (`publicity: Publicity`, `scope_exit: ScopeExit`)
- **Client** (`client/src/world/replication_config.rs`): a plain enum (`Private | Public | Delegated`)

Any user reading both APIs will expect these to be the same type — they aren't. The client version is legacy-shaped and should be aligned with the server's richer form or renamed to make the distinction explicit.

---

### P-API-2 — Authority API: resource verbs must match entity verbs  
**Priority: High**

The entity authority verb set on `EntityMut` is:
- Server: `give_authority(user_key)`, `take_authority()`, `release_authority()`
- Client: `request_authority()`, `release_authority()`

Resources have `resource_take_authority` / `resource_release_authority` but **no** `resource_give_authority` — because resources use client-initiated delegation. This asymmetry is semantically correct but needs to be consistent with entity authority naming in all other respects (verb form, casing pattern).

**Note:** The top-level server methods (`entity_give_authority`, `entity_take_authority`, `entity_release_authority`) and the replication-state verbs (`enable_entity_replication`, `disable_entity_replication`, etc.) need verification — Connor believes these may be marked adapter-internal only, in which case they are N/A for public API cleanup.

---

### P-API-3 — Rename `make_room` → `create_room`  
**Priority: Medium**

`server.make_room()` is the only verb in the API that uses `make`. Everything else uses `spawn` (for entities), `insert` (for resources), or `create`/`configure`. `make_room` → `create_room` aligns with the surrounding verb set. (`spawn` was rejected for rooms — `spawn` is reserved for entities.)

---

### P-API-4 — Standardize count method noun form to plural  
**Priority: Medium**

Current inconsistency:

| Method | Noun form |
|---|---|
| `server.users_count()` | plural ✓ |
| `server.rooms_count()` | plural ✓ |
| `server.resource_count()` | **singular** ✗ |
| `user.room_count()` | **singular** ✗ |
| `room.users_count()` | plural ✓ |
| `room.entities_count()` | plural ✓ |

Fix: rename `resource_count` → `resources_count` and `room_count` → `rooms_count`. Consistent rule: noun before `_count` is always plural.

---

### P-API-7 — `take_world_events` return type naming  
**Priority: Medium**

```rust
server.take_world_events() -> Events<E>
client.take_world_events() -> WorldEvents<E>
```

Both drain accumulated world events since last call. The return types are named differently (`Events` vs `WorldEvents`), making them look like they carry different things when they represent the same concept.

---

### P-API-8 — Static-entity method proliferation  
**Priority: HIGH (Connor's top priority)**

The "static" concept creates three parallel method pairs:

```rust
server.spawn_entity(world) -> EntityMut
server.spawn_static_entity(world) -> EntityMut

server.insert_resource(world, value) -> Result<E, ...>
server.insert_static_resource(world, value) -> Result<E, ...>

server.enable_entity_replication(entity)          // if adapter-public
server.enable_static_entity_replication(entity)   // if adapter-public
```

This should be collapsed into a single constructor/method with a `.static()` builder flag or a parameter, rather than duplicating every entry point for one binary distinction. Design the consolidation before implementing.

---

### P-API-10 — `send_message` should return `Result` on the server  
**Priority: Medium**

```rust
server.send_message<C, M>(&mut self, user_key, message)         // returns ()
client.send_message<C, M>(&mut self, message) -> Result<(), E>  // returns Result
```

The server silently drops the message if the channel is full or the user is gone. It should return `Result` (or at minimum `bool`) to give callers visibility into failure. Connor confirmed: server side should return `Result`.

---

### P-API-12 — Add `entity_is_delegated` predicate  
**Priority: Low**

```rust
server.entity_is_static(entity) -> bool        // exists
// to check delegation today:
server.entity_replication_config(entity)       // returns Option<ReplicationConfig>
    .map(|c| c.publicity == Publicity::Delegated)
```

`entity_is_static` is a convenience predicate; the equivalent `entity_is_delegated` is missing. Add it for symmetry with the existing predicate set.

---

### P-API-13 — Remove `insert_components` (batch variant) from server `EntityMut`  
**Priority: Low**

```rust
entity_mut.insert_component(component)    // exists on server + client
entity_mut.insert_components(vec![...])   // server-only batch variant
```

The batch method exists only on the server side. Connor's verdict: remove it — callers can just call `insert_component` in a loop. Asymmetric convenience methods add surface area without proportionate value.

---

## Rejected / N/A

| Issue | Verdict | Reason |
|---|---|---|
| Issue 5: `enter/leave` vs `add/remove` room membership | **Rejected** | Connor likes the dual-perspective API as-is |
| Issue 9: `rtt()` Option vs bare return | **Rejected** | Not agreed |
| Issue 11: `_worldless` suffix | **N/A** | These methods are explicitly marked as adapter-internal only; not user-facing |

---

## Needs verification before implementation

- **Issue 2 top-level methods**: Verify whether `entity_give_authority`, `entity_take_authority`, `entity_release_authority` on the raw server are public API or adapter-internal (Connor believes adapter-internal). If adapter-internal, the duplication concern is N/A.
- **Issue 6 replication-state verbs**: Verify whether `enable/disable/pause/resume_entity_replication` are public API or adapter-internal. If adapter-internal, the verb-family confusion is N/A for external users; still worth reviewing `pause`/`resume` naming if they ARE public.

---

## Implementation order (when scheduled)

1. P-API-8 (static proliferation) — highest priority per Connor
2. P-API-1 (ReplicationConfig two types) — high; semantic confusion
3. P-API-2 (resource authority verb alignment) — high; after verifying which methods are public
4. P-API-10 (send_message Result on server) — medium; mechanical
5. P-API-3 (make_room → create_room) — medium; mechanical rename
6. P-API-4 (plural count methods) — medium; mechanical rename
7. P-API-7 (Events vs WorldEvents naming) — medium; requires checking all call sites
8. P-API-12 (entity_is_delegated predicate) — low; additive
9. P-API-13 (remove insert_components) — low; deletion
