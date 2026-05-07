# DEBUGGING_PLAYBOOK.md — Naia BDD Failure Investigation

**Audience:** an agent (you) staring at a failing scenario.
**Purpose:** make the next bug hunt take 3 iterations, not 15. The tools you need almost certainly exist already; this doc tells you which ones.

---

## 1. First move when a scenario fails

```bash
# Run the single failing scenario with debug instrumentation enabled.
# (After Sidequest S.2 lands, --scenario-key replaces the jq dance.)
cargo run -p naia_npa --release --features e2e_debug -- run \
  --plan test/specs/resolved_plan.json \
  --scenario-key "authority:Rule(03):Scenario(08)"
```

Then, **before adding any `eprintln!`**, drop one line into the failing assertion:

```rust
// e.g. inside steps/then/state_assertions.rs
ctx.scenario_for_debug().debug_dump_identity_state(
    "denied check",
    &entity_key,
    &[client_a_key, client_b_key],
);
```

That single call dumps server presence, server's per-user `LocalEntity` view, registered client entity, world-side existence, and `LocalEntity` value for every client you pass. **Most stale-mapping / scope-timing / id-collision bugs show up directly in that output.** Only reach for custom `eprintln!`s after this dump fails to localize the cause.

After Sidequest S.3 lands, `Scenario::expect_with_ticks_internal` will auto-dump on timeout, so you may not even need step 2.

---

## 2. Existing debug APIs — quick reference

All gated on `cargo --features e2e_debug` (compiled out in release). Zero perf cost when off.

| API | What it returns | When to use |
|---|---|---|
| `Scenario::debug_dump_identity_state(label, &EntityKey, &[ClientKey])` | Multi-line eprintln: server registered/in-world, server's `local_entity` per user, each client's registered entity + world presence + `LocalEntity` | First move on any "entity not found" / "wrong entity" / "auth status drift" failure |
| `RemoteEntityChannel::debug_channel_snapshot()` → `(state, last_epoch_id, buffered_count, peek_front, last_processed_id)` | Per-channel state machine snapshot | Suspected message-ordering or epoch-filter bug |
| `RemoteEntityChannel::debug_auth_diagnostic()` → `(state, (next_subcommand_id, buffered, last_processed, …))` | AuthChannel state | Authority status machine failures (delegation, migration) |
| `LocalWorldManager::debug_channel_snapshot(remote_entity)` | Same as above but addressed by `RemoteEntity` | Server-side debugging without an `EntityKey` in hand |
| `SERVER_SCOPE_DIFF_ENQUEUED`, `SERVER_SEND_ALL_PACKETS_CALLS` (atomic counters in `world_server.rs`) | Cumulative event counts | Suspected "the server didn't fire X" — pre/post snapshot proves enqueue happened |

Find more by `rg 'cfg\(feature = "e2e_debug"\)' shared/src` — the cfg gates are systematic.

---

## 3. Entity id spaces — the table you wish you had

Naia distinguishes seven id types. They all eventually wrap a `u16` or a `u64`, and the same numeric value can mean different things on different sides. **Mismatches across these are the most common bug class.**

| Type | Defined in | Side | Purpose | Allocator | Recycled when |
|---|---|---|---|---|---|
| `GlobalEntity(u64)` | `shared/src/world/entity/global_entity.rs` | both | Process-unique id for an entity across its full lifecycle on a given side | Each side independently | Never (monotonic) |
| `HostEntity { id: u16, is_static: bool }` | `shared/src/world/local/local_entity.rs` | sender | Wire id assigned by the *host* (sender-of-spawn) for a (user, entity) pair | `HostEntityGenerator` per connection | On Despawn ACK (recycled into `KeyGenerator`) |
| `RemoteEntity { id: u16, is_static: bool }` | `shared/src/world/local/local_entity.rs` | receiver | Wire id as observed by the *remote* (receiver-of-spawn) | Server tells the client | When client processes Despawn |
| `OwnedLocalEntity::{Host,Remote} { id: u16, is_static: bool }` | `shared/src/world/local/local_entity.rs` | both | Tagged-union wire form: same `id`, but with a bit indicating which side originated the spawn | — | — |
| `LocalEntity` (newtype around `OwnedLocalEntity`) | `shared/src/world/local/interior_visibility.rs` | both | Public API surface for `OwnedLocalEntity` | — | — |
| `TestEntity` (`= naia_demo_world::Entity`) | `test/harness/src/lib.rs` (re-export) | test world only | Slot id in the test ECS world | `TestWorld::spawn_entity` | When `TestWorld::despawn_entity` runs |
| `EntityKey(u32)` | `test/harness/src/harness/keys.rs` | test only | BDD-side stable handle: one `EntityKey` represents "this entity" across server + all clients for the duration of a scenario | `EntityRegistry` | Not recycled within a scenario |

### The two pitfalls this table prevents

1. **Same `id` u16, different `OwnedLocalEntity` variant.** On the client, an entity is stored as `Remote { id: 0 }`. The server's view of "entity for user B" is `Host { id: 0 }`. They have the same `id`, but `client.local_entity(world_ref, &Host(0))` returns `None` — variant is part of the lookup key. Always use `extract_local_entity_value` when comparing across sides.

2. **Recycled `HostEntity` ids.** `HostEntityGenerator` recycles ids from a `KeyGenerator` pool when `on_delivered_despawn_entity` fires (i.e., on Despawn ACK). Between Despawn-send and Despawn-ACK, an `apply_scope_for_user` re-include can race with the recycle. See pattern (a) below.

---

## 4. Five common failure patterns

### (a) Stale mapping after in-flight despawn — `[entity-delegation-15]` shape

**Symptom:** A re-included entity is "missing" on the client perpetually after exclude→include. `debug_dump_identity_state` shows: server registered ✓, server's per-user `local_entity` = `None`, client registered ✓, client world has new TestEntity, but `c.entity(&entity_key)` returns `None`.

**Cause:** `host_init_entity` saw the still-mapped HostEntity (from the in-flight Despawn that B hasn't ACK'd) and skipped fresh allocation. The eventual Despawn ACK then wiped the mapping via id-collision.

**Fix shape:** Detect stale by cross-checking with `HostEngine` channel existence. See commit `9aa47e80`.

### (b) Recycled HostEntity id collision

**Symptom:** A new entity inherits authority/component state from a previously-despawned entity with the same `HostEntity.id`.

**Cause:** Despawn ACK recycled the id into the generator pool, but a stale reference somewhere (`EntityRegistry`, a buffered message, a delivered_engine record) still maps the id to the old `GlobalEntity`.

**Where to look:** All `HashMap<HostEntity, _>` and `HashMap<RemoteEntity, _>` collections. Run `rg 'HashMap<(?:HostEntity|RemoteEntity)' shared/src`.

### (c) Scope-vs-despawn distinction — `[entity-scopes-08]` / `[entity-scopes-15]` shape

**Symptom:** Test passes "client A doesn't see entity" both when entity is out-of-scope AND when it's despawned, even though the test should distinguish them.

**Cause:** A check that only consults `EntityRegistry` (which is registry-stable for the scenario duration) without also consulting `world_ref.has_entity(...)`. Registry-stable ≠ actually-present-in-world.

**Fix:** Always combine `entity_registry.contains(...)` with `world_ref.has_entity(...)`. See `ServerExpectCtx::has_entity` after the 2026-05-06 fix.

### (d) Auth status drift after migration

**Symptom:** Entity is migrated (host↔remote authority change), but `entity.authority()` reads stale on one side.

**Cause:** `AuthChannel` state-machine vs `EntityAuthStatus` mismatch — usually the receiver's `next_subcommand_id` wasn't synced.

**First check:** `debug_auth_diagnostic()` on both sides — compare `state` and the `(next_subcommand_id, buffered, last_processed)` tuple. Asymmetry localizes the bug.

### (e) `SetAuthority` dropped due to `LocalEntityMap` miss

**Symptom:** Client never observes a `SetAuthority(Denied)` event the server sent.

**Cause:** `local_entity_map.global_entity_from_remote(remote_entity)` returned `None` — usually because the spawn wasn't yet processed when the SetAuthority arrived. See `RemoteWorldManager::process_ready_messages` SetAuthority arm — it currently early-`continue`s when this happens.

**Investigate:** Add a snapshot of the `local_entity_map` at the SetAuthority handling site. If the entry is missing, check the message-ordering / spawn-barrier for that channel.

---

## 5. Iterative dev recipe — `cargo watch`

```bash
# Recompile + re-run the failing scenario on every save.
cargo watch -x "run -p naia_npa --release --features e2e_debug -- \
  run --plan test/specs/resolved_plan.json \
  --scenario-key 'authority:Rule(03):Scenario(08)'"
```

For source edits in `naia-shared` / `naia-server` / `naia-test-harness`, expect ~10s recompile per save. Edits inside `test/tests/src/steps/` (the BDD step bindings) are faster.

Don't `cargo test --workspace` until you've localized the bug — it's slow and noisy.

---

## 6. What this playbook is NOT

- **Not** Naia internals docs — see `_AGENTS/SYSTEM.md` and module-level rustdoc.
- **Not** SDD / namako / Tesaki tutorial — see `_AGENTS/SYSTEM.md` §2.
- **Not** a list of every `e2e_debug` cfg point — `rg 'cfg\(feature = "e2e_debug"\)' shared/src` is authoritative.
- **Not** a substitute for reading the failing test — always read the scenario's `Then` step and the assertion's source first.

When this playbook is wrong (an API renamed, a pattern shifts), fix it in the same commit as the rename. It rots fast otherwise.
