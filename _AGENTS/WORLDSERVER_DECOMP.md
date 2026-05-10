# WorldServer Decomposition — Architecture & Progress

**Target file:** `server/src/server/world_server.rs`  
**Baseline (2026-05-10):** 3,826 lines / 153 methods (pre-work)  
**After RoomStore:** 3,710 lines / 152 methods  
**Approach:** Manager-as-field pattern — each manager is a struct+impl in its own file, held as a field by `WorldServer`. Cross-domain mutation methods use struct destructuring at the `WorldServer` call site so the borrow checker sees disjoint field borrows. `WorldServer` becomes a thin orchestration layer.

---

## Field inventory (30 fields)

| Field | Type | Domain | Plan |
|---|---|---|---|
| `server_config` | `ServerConfig` | Config | stays |
| `channel_kinds` | `ChannelKinds` | Protocol | stays |
| `message_kinds` | `MessageKinds` | Protocol | stays |
| `component_kinds` | `ComponentKinds` | Protocol | stays |
| `client_authoritative_entities` | `bool` | Protocol | stays |
| `io` | `Io` | IO | stays (already encapsulated) |
| `heartbeat_timer` | `Timer` | Timers | stays (3 timers, not worth extracting) |
| `ping_timer` | `Timer` | Timers | stays |
| `timeout_timer` | `Timer` | Timers | stays |
| `users` | `HashMap<UserKey, WorldUser>` | Users | → **UserStore** |
| `disconnected_users` | `HashMap<SocketAddr, UserKey>` | Users | → **UserStore** |
| `user_connections` | `HashMap<SocketAddr, Connection>` | Connections | → **ConnectionStore** |
| `addrs_with_new_packets` | `HashSet<SocketAddr>` | Connections | → **ConnectionStore** |
| `outstanding_disconnects` | `Vec<UserKey>` | Connections | → **ConnectionStore** |
| `room_store` | `RoomStore` | Rooms | ✅ **DONE** |
| `entity_room_map` | `EntityRoomMap` | Entities/Rooms | stays (shared; already typed) |
| `entity_scope_map` | `EntityScopeMap` | Scope | stays (already typed) |
| `scope_change_queue` | `VecDeque<ScopeChange>` | Scope | stays (written by 4 domains) |
| `scope_checks_cache` | `ScopeChecksCache<E>` | Scope | stays (already typed) |
| `global_world_manager` | `GlobalWorldManager` | Entities | stays (already encapsulated) |
| `global_entity_map` | `GlobalEntityMap<E>` | Entities | stays (already encapsulated) |
| `incoming_world_events` | `WorldEvents<E>` | Events | stays (already typed) |
| `incoming_tick_events` | `TickEvents` | Events | stays (already typed) |
| `global_request_manager` | `GlobalRequestManager` | Req/Resp | stays (already encapsulated) |
| `global_response_manager` | `GlobalResponseManager` | Req/Resp | stays (already encapsulated) |
| `time_manager` | `TimeManager` | Ticks | stays (already encapsulated) |
| `pending_auth_grants` | `Vec<(UserKey, GlobalEntity, EntityAuthStatus)>` | Authority | stays (tiny; orchestration only) |
| `global_priority` | `GlobalPriorityState<E>` | Priority | stays (already typed) |
| `user_priorities` | `HashMap<UserKey, UserPriorityState<E>>` | Priority | stays (already typed per-user) |
| `resource_registry` | `ResourceRegistry` | Resources | stays (already encapsulated) |

---

## Manager extraction plan

### ✅ Phase 1 — `RoomStore` — DONE (commit `eae71471`, 2026-05-10)

**File:** `server/src/server/room_store.rs`  
**Owns:** `rooms: BigMap<RoomKey, Room>`  
**Also:** `ScopeChange` enum moved to `server/src/server/scope_change.rs`

Pure queries are self-contained (no external params). Mutation methods (`add_user`, `remove_user`, `destroy`, `add_entity`, `remove_entity`, `remove_all_entities`) accept the external state they need as params and return `ScopeChange` events for `WorldServer` to enqueue. `WorldServer` room methods are now thin delegation stubs.

Lines saved in `world_server.rs`: ~116 (3,826 → 3,710)

---

### Phase 2 — `UserStore`

**File:** `server/src/server/user_store.rs`  
**Owns:** `users: HashMap<UserKey, WorldUser>`, `disconnected_users: HashMap<SocketAddr, UserKey>`

**Pure queries (self-contained):**
- `get(&UserKey) -> Option<&WorldUser>`
- `get_mut(&UserKey) -> Option<&mut WorldUser>`
- `contains(&UserKey) -> bool`
- `keys() -> Vec<UserKey>`
- `len() -> usize`
- `by_addr(&SocketAddr) -> Option<UserKey>` (from `disconnected_users`)

**Mutation methods (accept external params):**
- `insert(user_key, addr)` — inserts into both maps
- `remove(&UserKey) -> Option<WorldUser>` — removes from both maps
- `register_disconnected(&UserKey, &SocketAddr)` — adds to `disconnected_users`
- `clear_disconnected(&SocketAddr)` — removes from `disconnected_users`

**Stays on `WorldServer`:**
- `user_delete` — also removes from `user_priorities`, `entity_scope_map`, `scope_checks_cache`, `io`, rooms; orchestration only
- All `user_disconnect`, `user_queue_disconnect` — cross-domain orchestration

**WorldServer delegation methods:**
- `user_exists`, `user`, `user_mut`, `user_keys`, `users_count` — become 1-line stubs
- `user_address`, `user_room_keys`, `user_rooms_count` — become 1-line stubs via `users.get(...)`

Estimated lines to save: ~80–100

---

### Phase 3 — `ConnectionStore`

**File:** `server/src/server/connection_store.rs`  
**Owns:** `user_connections: HashMap<SocketAddr, Connection>`, `addrs_with_new_packets: HashSet<SocketAddr>`, `outstanding_disconnects: Vec<UserKey>`

**Pure queries:**
- `get(&SocketAddr) -> Option<&Connection>`
- `get_mut(&SocketAddr) -> Option<&mut Connection>`
- `iter_mut() -> impl Iterator<Item=(&SocketAddr, &mut Connection)>`
- `contains(&SocketAddr) -> bool`
- `len() -> usize`

**Pending-packet tracking:**
- `mark_has_packets(&SocketAddr)`
- `drain_addrs_with_packets() -> Vec<SocketAddr>`

**Disconnect tracking:**
- `queue_disconnect(&UserKey)`
- `drain_disconnects() -> Vec<UserKey>`

**Connection lifecycle:**
- `insert(&SocketAddr, Connection)`
- `remove(&SocketAddr) -> Option<Connection>`

**Note on cross-cutting:** `user_connections` is the most-touched field in `WorldServer` — packet processing, scope, messages, auth, heartbeat, and bandwidth all access it. The extraction gives `WorldServer` a cleaner accessor API but the orchestration methods stay.

Estimated lines to save: ~60–80

---

## What stays on `WorldServer` and why

| Concern | Why it stays |
|---|---|
| Authority methods (~600 lines) | Orchestration over `global_world_manager` + fan-out to `user_connections`; no owned state to extract |
| Scope update methods (`update_entity_scopes`, `drain_scope_change_queue`, `apply_scope_for_user`) | Deep cross-field: rooms, users, connections, entity_scope_map, entity_room_map all simultaneously |
| Packet I/O (`send_all_packets`, `process_all_packets`, `receive_all_packets`) | Orchestration over IO + every connection |
| Entity methods (~1000 lines) | Orchestration over `global_world_manager` + scope fan-out |
| `scope_change_queue` | Written by rooms (via RoomStore return values), user scope toggles, entity publish — genuinely cross-domain |

---

## Metrics tracker

| After | Lines | Delta |
|---|---|---|
| Baseline (pre-work) | 3,826 | — |
| Phase 1 (RoomStore) | 3,710 | −116 |
| Phase 2 (UserStore) | ~3,610 | ~−100 |
| Phase 3 (ConnectionStore) | ~3,530 | ~−80 |

Target: ~3,500 lines (−8% from baseline). The irreducible core is orchestration methods that legitimately touch 4–6 domains simultaneously — those belong on the coordinator struct by design.

---

## Pattern reference

```rust
// Pure query — no external params needed
pub fn room_exists(&self, room_key: &RoomKey) -> bool {
    self.room_store.contains(room_key)
}

// Cross-domain mutation — struct destructuring satisfies borrow checker
pub(crate) fn room_add_user(&mut self, room_key: &RoomKey, user_key: &UserKey) {
    let Self { room_store, users, global_entity_map, scope_checks_cache, scope_change_queue, .. } = self;
    let change = room_store.add_user(room_key, user_key, users, global_entity_map, scope_checks_cache);
    scope_change_queue.push_back(change);
}
```
