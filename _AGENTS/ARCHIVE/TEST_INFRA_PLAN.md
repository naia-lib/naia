# Naia — Test Infrastructure Plan

**Owner:** Connor + twin-Claude
**Branch:** `dev`
**Created:** 2026-05-08
**Gate:** `cargo run --manifest-path test/npa/Cargo.toml -- coverage --specs-root test/specs` → 309 active (100% pass)

---

## Current state snapshot

| Metric | Value |
|---|---|
| "Active" contracts (coverage tool) | **309** |
| Of which: real executable scenarios | **~232** |
| Of which: `@PolicyOnly` empty stubs | **~77** |
| `@Deferred` (excluded from gate) | **11** (all in `00_foundations.feature`) |
| Step binding files | 22 files, ~6k LOC |
| Harness core | 15 modules, ~6.2k LOC |
| `scenario.rs` | **1731 lines** (the single largest file) |
| Bevy NPA scenarios | **9** (smoke only) |

### The count is misleading

The "309 active" figure **includes ~77 `@PolicyOnly` empty stubs** that trivially pass with no steps. They are valid contracts but not executable tests. The real test count is ~232. Converting these stubs to real tests does **not** increase the count — they're already counted. To grow the count, we must write genuinely new scenarios.

---

## What exists — layer by layer

### Harness (`test/harness/`)

The simulation harness provides a deterministic tick-based server + N-client environment. The mutate/expect cycle cleanly separates state changes from assertions, and the TestClock gives full determinism.

**Core modules:**
| File | LOC | Responsibility |
|---|---|---|
| `scenario.rs` | 1731 | Main `Scenario` struct — owns everything |
| `server_events.rs` | 695 | Per-tick server event collection and replay |
| `client_events.rs` | 707 | Per-tick client event collection and replay |
| `server_mutate_ctx.rs` | 574 | Mutation API for server-side Given/When |
| `client_mutate_ctx.rs` | 326 | Mutation API for client-side Given/When |
| `server_expect_ctx.rs` | 257 | Read-only assertion API for server |
| `client_expect_ctx.rs` | 255 | Read-only assertion API for client |
| `entity_registry.rs` | 331 | Maps harness keys → ECS entity IDs |
| `server_entity.rs` | 208 | Server-side entity builder |

**Key existing capabilities:**
- Multi-client scenarios with named clients (`ClientName`)
- Room membership and user scope management
- Authority delegation — request/grant/deny/release
- Resource replication — insert/remove/configure
- Link conditioner — per-client latency, loss, reorder
- Tick-buffered message send and injection
- EntityProperty buffer cap testing
- Reconnect / disconnect sequences
- `bdd_store`/`bdd_get`/`bdd_take` for cross-step state passing
- Trace capture and golden-trace comparison (`spec_tool`)
- `e2e_debug` feature for timeout diagnostics

**What the harness CANNOT do today:**
- Insert or remove a component on an entity that already exists in scope (dynamic component ops after spawn)
- Track server-side `MessageEvent` firing (only client-side message receipt is tracked)
- Track server-side `AuthDenied` event (only `AuthGranted` and `AuthReset`)
- Test static entity replication (`as_static()` / `enable_static_replication()`) — no step or harness path
- Simulate two clients requesting authority simultaneously (single-threaded)
- Per-component packet inspection or drop
- Wallclock advancement (heartbeat timeout, TTL scenarios)

### Step Bindings (`test/tests/src/steps/`)

22 files, ~6k LOC. Three-layer structure: `given/` (state setup), `when/` (actions), `then/` (assertions). Vocabulary types (`ClientName`, `EntityRef`) enforce naming conventions at compile time.

**Storage keys** are defined centrally in `world_helpers.rs:23-49` as `&'static str` constants — well-organized.

**Key problem files by LOC:**
| File | LOC | Issue |
|---|---|---|
| `then/state_assertions_entity.rs` | 597 | Largest then-file; handles entity, authority, delegation, messaging |
| `then/event_assertions.rs` | 534 | Event tracking assertions |
| `then/state_assertions_replication.rs` | 517 | May be splittable |
| `when/client_actions.rs` | 496 | All client-side mutations |
| `when/network_events_transport.rs` | 481 | At the 500-LOC limit |
| `when/server_actions_entity.rs` | 457 | All server entity mutations |

**Missing step categories:**
- Dynamic component insert/remove on existing in-scope entities
- Static entity creation (server-side `as_static()` path)
- "N ticks elapse" as a phrase-level binding (tests call `tick_n()` in code directly)
- Server-side `MessageEvent` observable assertion
- Resource late-join observable (client connecting after resource already inserted)
- Authority-cleared assertion (client-side state after server reconfigures entity away from `Delegated`)

### NPA adapter (`test/npa/`)

Clean four-command CLI: `manifest`, `run`, `coverage`, `help`. The `coverage` subcommand is functional. Scenario-key filtering with edit-distance suggestions is useful for debugging. Run report is machine-readable JSON. Solid, low-debt.

### Bevy NPA (`test/bevy_npa/`)

Mirrors the main NPA architecture. Has a 9-scenario smoke suite (`bevy_specs/features/smoke.feature`) covering only connection lifecycle and event ordering. The BDD step bindings in `steps.rs` cover only connect/disconnect/event-count assertions — no entity, replication, authority, or resource scenarios.

No README exists. The state is: working smoke layer, very thin coverage, clear path to expand.

### Bench / perf (`test/bench/`, `benches/`, `crucible.toml`)

Crucible integration: baseline named `perf_v0`, post-assert via `naia-bench-report --assert-wins`. The `wins.rs` module defines pass/fail criteria for the benchmark suite (29 wins, 0 losses confirmed at last audit).

**Gaps:** Wins criteria are defined but not commented (what threshold = a "win"?).

### Loom & compile_fail

Loom covers `DirtySet` concurrency properties. Compile_fail covers 2 derive-macro violations (`immutable_entity_property`, `immutable_property`). Both are minimal but correct.

---

## Quality issues (concrete)

### H-1 — `scenario.rs` dead alternation-enforcement code
**File:** `test/harness/src/harness/scenario.rs:112, 177, 384, 392, 546, 582, 602, 638, 692`

The `LastOperation` enum (line 112) and `last_operation` field (line 177) were designed to enforce strict mutate→expect alternation. Three enforcement checks are commented out (lines 384, 546, 602). The field is still assigned (lines 392, 582, 638) but never read, making it dead state.

**Fix (delete path):** Remove the `LastOperation` enum, the `last_operation` field, and all six assignment sites. The commented-out enforcement blocks go with them. This is the preferred option — the alternation contract is not enforced today and the dead code misleads readers.

**Fix (enable path — alternative):** Re-enable the three commented-out checks with clear panic messages and remove the `// if` wrapper. Only choose this if strict alternation enforcement is desirable for all current tests.

→ **Decision: delete** unless there's a known reason the enforcement was disabled.

### H-2 — Missing harness API for dynamic component operations
**Affected @PolicyOnly stubs:** `entity-replication-04`, `entity-replication-05`, `entity-replication-11`

`ServerMutateCtx` has no method to insert or remove a component on an entity that's already registered and in-scope. This blocks three live-test conversions.

**Fix:** Add `ServerMutateCtx::insert_component_on<C: ReplicatedComponent>(key, component)` and `remove_component_from<C>(key)` — each looks up the entity from the registry and calls `server.insert_component()` / `server.remove_component()`.

### H-3 — Missing server-side MessageEvent and AuthDenied tracking
**Affected @PolicyOnly stubs:** `server-events-05`, `server-events-10`

`server_events.rs` tracks `AuthGranted` and `AuthReset` but not `AuthDenied` or server-received `MessageEvent`. The server-side message receipt is an important observable.

**Fix:** Add `auth_denied_count() -> usize` to `ServerEvents`. Add `server_message_count(channel: ChannelKind) -> usize` that counts inbound messages per channel.

### H-4 — No static entity harness path
**Affected spec area:** `03_replication.feature` (no static-entity coverage at all)

`ServerMutateCtx::spawn_entity` always creates dynamic entities. There is no `spawn_static_entity` API in the harness. The production API (`enable_static_replication`) is untested via BDD.

**Fix:** Add `ServerMutateCtx::spawn_static_entity(key)` that calls `server.enable_static_replication(&entity)` before registering the entity in the registry.

### H-5 — Client does not reset authority state when entity is reconfigured away from `Delegated`
**Affected @PolicyOnly stubs:** `entity-delegation-05`, `entity-authority-13`

When the server calls `configure_replication(Public)` on a previously-`Delegated` entity, the `ReplicationConfig` change message already travels to the client via the existing config-update packet. However, the **client does not reset its local authority state** when it processes that packet. A client that previously received `Denied` (or had a pending `Requested`) retains that stale authority status indefinitely.

The fix is **purely client-side** — no new server message is required. The existing config-change packet is the signal.

**Fix:** In the client's `configure_entity_replication` handler, when the incoming config's `publicity` changes from `Delegated` to any non-`Delegated` value, clear the client's local `EntityAuthStatus` for that entity (reset to `None` / not-delegable).

### S-1 — Several step files approach or exceed 500-LOC limit
`state_assertions_entity.rs` (597), `event_assertions.rs` (534), `state_assertions_replication.rs` (517), `client_actions.rs` (496).

**Fix:** Split `state_assertions_entity.rs` into `state_assertions_auth.rs` (authority/delegation assertions) + `state_assertions_messaging.rs` (message/request assertions); entity/component assertions stay in the original. Evaluate others after this split.

### S-2 — Missing "N ticks elapse" phrase binding
Many step bodies call `tick_n(ctx, N)` directly in Rust code. Feature files cannot express time passing without a binding, forcing scenario logic into step Rust code.

**Fix:** Add `#[when("{int} ticks elapse")]` and `#[given("{int} ticks have elapsed")]` in `given/state_misc.rs` (or `given/setup.rs`).

### B-1 — Wins criteria undocumented
`test/bench/src/wins.rs` defines what constitutes a benchmark win but has no comment explaining the thresholds.

**Fix:** Add a comment header to `wins.rs` explaining what a "win" means (e.g., new baseline ≤ X% of old baseline for latency metrics).

---

## Coverage gaps — new scenarios needed

### Gap A — Static entity replication (no coverage at all)
`03_replication.feature` has zero scenarios for the `as_static()` / `enable_static_replication()` path.

**New live scenarios (4):**
- `static-entity-01` — Static entity spawns on client when entering scope
- `static-entity-02` — Static entity: component update on server is NOT replicated after initial spawn
- `static-entity-03` — Static entity: correct initial component values received on spawn
- `static-entity-04` — Static entity: full snapshot re-sent on scope re-entry (exclude then include)

Note: the "panic on insert after construction" contract (`static-entity-05` from earlier drafts) belongs in a unit test or integration_only test, not BDD — panics are awkward to assert in Gherkin. Add it to `test/harness/contract_tests/integration_only/` instead.

### Gap B — Dynamic component operations (3 stubs → live tests)
Requires H-2 (T1.1) first.

**Convert to live:**
- `entity-replication-04` — Component insert event fires for in-scope additions
- `entity-replication-05` — Component remove event fires for in-scope removals
- `entity-replication-11` — Component remove on out-of-scope entity is safe (no panic)

### Gap C — Resource delegation end-to-end (thin coverage)
`07_resources.feature` has 9 real tests covering insert, diff, authority request/grant/release. Missing end-to-end delegation flow and late-join.

**New live scenarios (3):**
- `resource-delegation-01` — Server configures resource as Delegated; client requests authority; client mutations replicate to server
- `resource-delegation-02` — Server reclaims authority via `configure_replicated_resource(Server)` after client held it; server mutations resume replication
- `resource-latejoin-02` — Client connecting after a dynamic resource is already live receives the current value immediately

### Gap D — Authority cleared on entity reconfigure (client fix + 2 stubs → live tests)
Requires H-5 (T1.4) first.

**Convert to live:**
- `entity-delegation-05` — After server reconfigures entity from `Delegated` to `Public`, client observes authority status is cleared (None / not-delegable)
- `entity-authority-13` — Client that previously received `Denied` observes authority status cleared after entity is reconfigured to `Public`

### Gap E — Bevy adapter BDD expansion
9 scenarios covering connection only. All new scenarios go in a new `bevy_specs/features/replication.feature`.

**Scope constraint:** These tests cover what the Bevy adapter adds *on top of* the core Naia protocol — the ECS change-detection bridge, Bevy event routing, and `Commands`-based APIs. They do NOT re-verify state-machine correctness (authority transitions, diff correctness, etc.) which is already covered by the 327 non-Bevy scenarios.

**New live scenarios (12) — three rules:**

*Rule: Entity lifecycle via Bevy CommandsExt (5)*
- `enable_replication` + Position inserted → client Bevy world gets entity [ECS hook]
- `SpawnEntityEvent<T>` fires on client [Bevy event bridge]
- `disable_replication` removes entity from client world [CommandsExt API]
- `DespawnEntityEvent<T>` fires when entity leaves scope [Bevy event bridge]
- Position mutated via Bevy world mutation → client observes update [change-detection bridge]

*Rule: Authority via Bevy CommandsExt (4)*
- `give_authority` command → client observes `EntityAuthStatus::Granted` [CommandsExt API]
- `EntityAuthGrantedEvent<T>` fires [Bevy event bridge]
- `request_authority` via `ClientCommandsExt` → Granted [client Commands API]
- `EntityAuthDeniedEvent<T>` fires for second requester [Bevy event bridge]

*Rule: Resources via Bevy ServerCommandsExt (3)*
- `replicate_resource` → client sees resource as Bevy `Resource<TestScore>` [ServerCommandsExt + resource mirror]
- `ResMut<TestScore>` mutation → client `Resource<TestScore>` updates [ResMut change-detection bridge]
- `request_resource_authority` via `ClientCommandsExt` → server observes Granted [client Commands API]

### Gap F — Server-side event observability (2 stubs → live tests)
Requires H-3 (T1.3) first.

**Convert to live:**
- `server-events-05` — Server-side `MessageEvent` count increments when client sends a message
- `server-events-10` — Server-side `AuthDenied` count increments when a second client's authority request is rejected

---

## Plan

### T0 — Housekeeping

- [ ] **T0.1** — Delete the dead `LastOperation` alternation code in `scenario.rs`: remove the enum (line 112), the `last_operation` field (line 177), all six assignment sites (lines 392, 582, 638, 692), and the three commented-out enforcement blocks (lines 384, 546, 602).
- [ ] **T0.2** — Add a comment header to `test/bench/src/wins.rs` explaining the win/loss threshold criteria (H-1 fix for bench).
- [ ] **T0.3** — Write `test/bevy_npa/README.md`: purpose (Bevy adapter BDD verification), relationship to naia_npa (same architecture, Bevy ECS world instead of raw harness), current state (smoke only, T4 expands it), and how to run.
- [ ] **T0.4** — Add `static-entity-05` panic test to `test/harness/contract_tests/integration_only/` as a `#[should_panic]` unit test.

### T1 — Harness: new APIs

- [ ] **T1.1** — Add `ServerMutateCtx::insert_component_on<C: ReplicatedComponent>(key: EntityKey, value: C)` and `remove_component_from<C: ReplicatedComponent>(key: EntityKey)` to `server_mutate_ctx.rs`. Both look up the entity via `EntityRegistry` then call through to `server.insert_component` / `server.remove_component`. Unlocks Gap B.
- [ ] **T1.2** — Add `ServerMutateCtx::spawn_static_entity(key: EntityKey)` to `server_mutate_ctx.rs`. Calls `server.enable_static_replication(&entity)` immediately after spawn (before any components are inserted). Unlocks Gap A.
- [ ] **T1.3** — Add to `server_events.rs`: `auth_denied_count() -> usize` (counts `EntityAuthDeniedEvent` firings), and `server_inbound_message_count(channel: ChannelKind) -> usize` (counts inbound `MessageEvent` per channel). Expose both via `ServerExpectCtx`. Unlocks Gap F.
- [ ] **T1.4** — Fix H-5 client-side authority state: in the client's `configure_entity_replication` handler, when the incoming `ReplicationConfig` transitions the entity's publicity away from `Delegated`, reset the client's local `EntityAuthStatus` for that entity to `None`. No new server message — the existing config-update packet is the signal. Unlocks Gap D.

### T2 — Step bindings: new phrases

All new bindings must follow the vocab.rs discipline rules (parameter names are part of the contract; phrase text must be unique and unambiguous).

- [ ] **T2.1** — Add `#[when("{int} ticks elapse")]` and `#[given("{int} ticks have elapsed")]` in `given/state_misc.rs`. Both call `tick_n(scenario, n)`.
- [ ] **T2.2** — Add `"the server inserts {component} on {entity}"` (When) and `"the server removes {component} from {entity}"` (When) in `when/server_actions_entity.rs`. Requires T1.1.
- [ ] **T2.3** — Add `"a server-owned static entity exists"` and `"a static entity exists with {component}"` (Given) in `given/state_entity.rs`. Requires T1.2.
- [ ] **T2.4** — Add `"the server has received {int} message(s)"` (Then) in `then/event_assertions.rs`, and `"the server has observed AuthDeniedEvent"` (Then) in `then/event_assertions.rs`. Requires T1.3.
- [ ] **T2.5** — Add `"the authority status for {entity} is not set"` / `"the entity is not delegable"` (Then) in `then/state_assertions_delegation.rs`. Used to assert the client sees no authority state after reconfigure. Requires T1.4.

### T3 — New and converted BDD scenarios

Run `cargo run --manifest-path test/npa/Cargo.toml -- coverage --specs-root test/specs` to verify count after each sub-task.

- [ ] **T3.1** — Add a new `Rule: Static entity replication` to `03_replication.feature` with 4 live scenarios (Gap A). Requires T1.2 + T2.3.
- [ ] **T3.2** — Convert `entity-replication-04`, `entity-replication-05`, `entity-replication-11` stubs to live tests with real steps (Gap B). Requires T1.1 + T2.2.
- [ ] **T3.3** — Add 3 new scenarios to `07_resources.feature` under a new `Rule: Resource delegation` (Gap C). No new harness work required — uses existing authority + resource APIs.
- [ ] **T3.4** — Convert `entity-delegation-05` and `entity-authority-13` stubs to live tests (Gap D). Requires T1.4 + T2.5.
- [ ] **T3.5** — Convert `server-events-05` and `server-events-10` stubs to live tests (Gap F). Requires T1.3 + T2.4.

### T4 — Bevy BDD expansion

Scope: test what the Bevy adapter adds on top of Naia (ECS bridge, event routing, Commands API). Do not re-verify core Naia protocol behavior.

- [ ] **T4.1** — Create `test/bevy_specs/features/replication.feature` with @Feature(bevy_replication). Add 5 entity-lifecycle scenarios (Rule 01): enable_replication→spawn, SpawnEntityEvent, disable_replication→despawn, DespawnEntityEvent, Bevy mutation→propagates. Add matching step bindings and harness support (world.rs: expanded protocol with Position/TestScore/TestPlayerSelection, ServerState tracking room_key+last_entity, ClientState tracking spawn/despawn counts + authority status + score, new imperative harness methods).
- [ ] **T4.2** — Add 4 authority scenarios (Rule 02): give_authority→Granted, EntityAuthGrantedEvent, ClientCommandsExt::request_authority→Granted, EntityAuthDeniedEvent for second requester. Add matching step bindings.
- [ ] **T4.3** — Add 3 resource scenarios (Rule 03): replicate_resource→Bevy Resource on client, ResMut mutation→client Resource updates, ClientCommandsExt::request_resource_authority→server Granted. Add matching step bindings.

### T5 — Step binding cleanup

Run `cargo check --workspace` and the gate after each split to confirm nothing broke.

- [ ] **T5.1** — Split `then/state_assertions_entity.rs` (597 LOC): extract authority/delegation assertions into `then/state_assertions_auth.rs` and message/request assertions into `then/state_assertions_messaging.rs`. Entity/component assertions stay in the original file. Update `then/mod.rs` imports.
- [ ] **T5.2** — After T5.1, re-measure all then/when files. If any still exceed 500 LOC, split further. Target files: `event_assertions.rs` (534), `state_assertions_replication.rs` (517), `client_actions.rs` (496), `network_events_transport.rs` (481).

---

## Dependency graph

```
T0  (no deps)
T1.1 → T2.2 → T3.2
T1.2 → T2.3 → T3.1
T1.3 → T2.4 → T3.5
T1.4 → T2.5 → T3.4
T2.1 (no harness dep)
T3.3 (no new harness dep — uses existing resource + authority APIs)
T4.1 → T4.2 → T4.3
T5.1 → T5.2
```

T0, T2.1, T3.3, T4.x, and T5.x are all parallelisable with the T1→T2→T3 chain.

---

## Acceptance criteria for "test infrastructure done"

1. `cargo run --manifest-path test/npa/Cargo.toml -- coverage --specs-root test/specs` reports **≥ 325 active**, with zero new `@PolicyOnly` stubs added.
2. Static entity replication is covered by at least 4 live scenarios in `03_replication.feature`.
3. All of the following are live tests (not stubs): `entity-replication-04/05/11`, `entity-delegation-05`, `entity-authority-13`, `server-events-05`, `server-events-10`.
4. Bevy NPA has ≥ 20 scenarios (up from 9), covering Bevy-adapter-specific behavior: ECS change-detection bridge, Bevy event routing (SpawnEntityEvent, DespawnEntityEvent, EntityAuthGrantedEvent, EntityAuthDeniedEvent), CommandsExt/ServerCommandsExt/ClientCommandsExt APIs, and the client Bevy Resource mirror for replicated resources.
5. No step binding file exceeds 500 LOC.
6. `LastOperation` dead code is fully removed — no commented-out enforcement, no dead field/enum.
7. `cargo check --workspace` clean, 0 build warnings.
8. `static-entity-05` panic contract is covered by a `#[should_panic]` unit test in `contract_tests/integration_only/`.

---

## Revised scenario count target

The old target of **350** was aspirational. Given that ~77 of the 309 are already `@PolicyOnly` stubs (already counted), and many policy-only contracts are genuinely untestable (wallclock, packet inspection, concurrent ops, transport layer), the realistic ceiling from conversion alone is ~319.

The new target of **≥ 325 active** is achievable through T3 (new scenarios in Gaps A and C) and is a more honest metric. Bevy NPA expansion (T4) adds on top independently.

The old 350 target is retired from `NAIA_PLAN.md` in favour of the criteria above.

---

## Archives

*No archived phases yet — this plan was created 2026-05-08.*
