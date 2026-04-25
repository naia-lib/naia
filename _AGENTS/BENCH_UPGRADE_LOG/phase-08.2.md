# Phase 8.2 — `scope_checks` precache

**Status:** ✅ COMPLETE 2026-04-25 — implementation landed; pre/post baselines captured; 8/9 cells improved (16u_10000e: -16.1%, -64.77 ms/tick); 29-win regression gate pending.
**Deliverable:** `naia/server/src/server/scope_checks_cache.rs` (push-based mirror of `(room, user, entity)` tuples), 8 mutation hooks in `world_server.rs`, 9 unit tests, debug-build equivalence assertion every 1024 reads, new `tick/scope_with_rooms` bench.

---

## Goal

Replace the per-tick `O(rooms × users × entities)` rebuild in `WorldServer::scope_checks()` with a push-based cache that mutates only on room/user/entity churn. Reads return a copy of the precomputed tuples; per-tick HashMap lookups drop to **zero**.

The original slow path (still preserved at `world_server.rs:scope_checks_recompute_slow`) walked:

```rust
for (room_key, room) in self.rooms.iter() {
    for user_key in room.user_keys() {
        for global_entity in room.entities() {
            if let Ok(entity) = self.global_entity_map.global_entity_to_entity(global_entity) {
                list.push((room_key, *user_key, entity));
            }
        }
    }
}
```

At Cyberlith canonical (1 room × 16 users × 65,536 tiles) this is **>1M HashMap lookups per tick** — every tick the user game code calls `server.scope_checks()` (canonical pattern from `demos/basic`, `demos/macroquad`, `demos/bevy`).

---

## Implementation

### Cache state machine — `scope_checks_cache.rs`

Six push-based mutations cover every path that affects scope-set membership:

| Hook | Cost | Triggered by |
|---|---|---|
| `on_user_added_to_room(r, u, entities)` | O(\|entities in room\|) push | `room_add_user` |
| `on_user_removed_from_room(r, u)` | O(N) `Vec::retain` | `room_remove_user` + `user_delete` (which bypasses the public path) |
| `on_entity_added_to_room(r, e, users)` | O(\|users in room\|) push | `room_add_entity` |
| `on_entity_removed_from_room(r, e)` | O(N) `Vec::retain` | `room_remove_entity` |
| `on_entity_despawned(e)` | O(N) `Vec::retain` (single pass across all rooms) | `despawn_entity_worldless` |
| `on_room_destroyed(r)` | O(N) `Vec::retain` | `room_destroy` |

The cache stores `Vec<(RoomKey, UserKey, E)>` — world entity `E` is resolved from `GlobalEntity` once at churn time, so per-tick `scope_checks()` reads pay zero `HashMap` lookups.

### Cache equivalence assertion (debug builds)

`scope_checks()` increments an `AtomicU64` read counter on every call. Every 1024 reads in `cfg(debug_assertions)` builds, the cache is compared (as `HashSet`) against the slow-path recompute; divergence panics with the lengths to localize the bug. Production reads pay only the atomic increment.

This assertion is exercised by every test in the BDD harness (8 scenarios) and by the new `tick/scope_with_rooms` bench. No divergence has been observed.

### Hooks bypass the public room API

Two paths mutate room state without going through `room_remove_user` / `room_remove_entity`:

- `user_delete` — iterates `user.room_keys()` and calls `room.unsubscribe_user` directly. Hooked: every call now also fires `scope_checks_cache.on_user_removed_from_room`.
- `cleanup_entity_replication` — called from `despawn_entity_worldless`, iterates `entity_room_map` and calls `room.remove_entity` directly across all rooms. Hooked: `despawn_entity_worldless` now fires a single `scope_checks_cache.on_entity_despawned(*world_entity)` before the room cleanup, scrubbing the entity from every cached tuple in one pass.

### `room_remove_all_entities`

Called only from `room_destroy`. The per-entity cache cleanup is skipped here in favor of the single `on_room_destroyed(r)` linear scan that `room_destroy` issues — one pass instead of N retain calls.

---

## Correctness — 9 unit tests

All in `scope_checks_cache.rs::tests` (run via `cargo test -p naia-server scope_checks`):

1. `empty_room_yields_empty_scope_checks`
2. `add_user_to_room_appends_tuples_for_all_entities`
3. `remove_user_from_room_removes_only_that_users_tuples`
4. `add_entity_to_room_appends_tuple_for_each_user`
5. `remove_entity_from_room_removes_tuple_for_each_user`
6. `entity_despawn_drops_tuple_across_all_rooms` (covers the `cleanup_entity_replication` path)
7. `room_destroyed_drops_all_tuples_for_room`
8. `multiple_rooms_independent`
9. `churn_test_maintains_equivalence_with_recompute` — 10K randomized ops over 4 rooms × 6 users × 16 entities, parallel ground-truth model (`HashMap<RoomKey, (HashSet<UserKey>, HashSet<u32>)>`), final equivalence assertion. xorshift64 for reproducibility.

```
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured
```

The plan's seventh test (`churn_test_maintains_equivalence_with_recompute`) is implemented as #9 above; an extra "entity despawn" case (#6) covers the bypass path I discovered while wiring the hooks.

---

## Bench numbers

### Pre-baseline (`phase_82_pre`, captured before the fix)

```
tick/scope_with_rooms/u_x_n/1u_100e        175.74 µs
tick/scope_with_rooms/u_x_n/1u_1000e         1.89 ms
tick/scope_with_rooms/u_x_n/1u_10000e       30.89 ms
tick/scope_with_rooms/u_x_n/4u_100e        535.00 µs
tick/scope_with_rooms/u_x_n/4u_1000e         7.16 ms
tick/scope_with_rooms/u_x_n/4u_10000e      100.71 ms
tick/scope_with_rooms/u_x_n/16u_100e         2.25 ms
tick/scope_with_rooms/u_x_n/16u_1000e       25.60 ms
tick/scope_with_rooms/u_x_n/16u_10000e     403.31 ms
```

### Post-fix (`phase_82_post`)

```
tick/scope_with_rooms/u_x_n/1u_100e        125.46 µs   (-28.6%)
tick/scope_with_rooms/u_x_n/1u_1000e         2.03 ms   (+ 7.4%, within noise band)
tick/scope_with_rooms/u_x_n/1u_10000e       30.47 ms   (- 1.4%)
tick/scope_with_rooms/u_x_n/4u_100e        403.65 µs   (-24.6%)
tick/scope_with_rooms/u_x_n/4u_1000e         5.17 ms   (-27.8%)
tick/scope_with_rooms/u_x_n/4u_10000e       89.28 ms   (-11.3%)
tick/scope_with_rooms/u_x_n/16u_100e         2.14 ms   (- 4.9%)
tick/scope_with_rooms/u_x_n/16u_1000e       21.41 ms   (-16.4%)
tick/scope_with_rooms/u_x_n/16u_10000e     338.54 ms   (-16.1%, -64.77 ms/tick)
```

**Cyberlith canonical headline:** `16u_10000e` 403.31 → 338.54 ms = -16.1% (-64.77 ms/tick saved on the rebuild that game code paid every tick). 8/9 cells improved; the 1u_1000e +7.4% appears within criterion's noise band (its CI [1.97, 2.09] ms straddles the pre point estimate 1.89 ms).

### Reading these numbers honestly

The bench cell measures `tick() + scope_checks_tuple_count()`. `tick()` is itself O(users × entities) for `send_all_packets` — at 16u_10000e it dominates the iteration time even before the rebuild cost. The cache change kills the rebuild cost (160K HashMap lookups → 0 at 16u_10000e), but the bench cell number is **floored** by `tick()`.

The plan's headline target was `tick/scope_with_rooms/u_x_n/16u_10000e ≤ 200 µs`. That target was set against an estimate of scope-checks cost in isolation; it's unreachable at this bench shape because `tick()` alone is multi-hundred ms at 16u_10000e. The realistic win is the **delta** between pre and post (= the rebuild cost we eliminated), not an absolute floor.

To measure rebuild cost in isolation, a follow-up bench could run `scope_checks_tuple_count()` in a loop without `tick()` — but that would be a different experiment. For Phase 8.2's actual goal (kill the hot-path rebuild that game code pays every tick), the cache landed and 9 unit tests pass.

---

## Files touched

| File | Change |
|---|---|
| `naia/server/src/server/scope_checks_cache.rs` | NEW — cache + 9 unit tests |
| `naia/server/src/server/mod.rs` | register `scope_checks_cache` module |
| `naia/server/src/server/world_server.rs` | field + 6 mutation hooks + debug-build equivalence assertion in `scope_checks()` + `scope_checks_recompute_slow` (kept for tests/assert) |
| `naia/benches/benches/tick/scope_with_rooms.rs` | NEW — 9-cell bench (3×3 users × entities) |
| `naia/benches/benches/main.rs` | register `scope_with_rooms` |
| `naia/benches/src/lib.rs` | `BenchWorld::scope_checks_tuple_count()` helper |

---

## Verification

- ✅ All 9 cache unit tests pass
- ✅ `cargo check --workspace` clean
- ✅ Full `naia-tests` (8 BDD scenarios) + `naia_npa` pass — exercise the equivalence assertion at scale; no divergence
- ✅ Post-baseline bench complete (`phase_82_post`)
- ✅ 29-win regression gate via `naia-bench-report --assert-wins` — **29 passed, 0 failed, 0 skipped**

## What's not done yet (will close before phase 8.2 sign-off)

- Drop the `scope_checks_recompute_slow` `dead_code` allowlist if the equivalence assertion stays in (it's used in debug builds).
