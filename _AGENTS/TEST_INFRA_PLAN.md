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

The simulation harness provides a deterministic tick-based server + N-client environment. It is well-designed at the conceptual level: the mutate/expect cycle cleanly separates state changes from assertions, and the TestClock gives full determinism.

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

**Storage keys** are defined centrally in `world_helpers.rs:23-49` as `&'static str` constants — well-organized, not scattered.

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
- Authority-cleared assertion (after server calls `configure_replication(Public)`)

### NPA adapter (`test/npa/`)

Clean four-command CLI: `manifest`, `run`, `coverage`, `help`. The `coverage` subcommand is functional (confirmed — it's what we ran above). Scenario-key filtering with edit-distance suggestions is useful for debugging. Run report is machine-readable JSON. Solid, low-debt.

### Bevy NPA (`test/bevy_npa/`)

Mirrors the main NPA architecture. Has a 9-scenario smoke suite (`bevy_specs/features/smoke.feature`) covering only connection lifecycle and event ordering. The BDD step bindings in `steps.rs` cover only connect/disconnect/event-count assertions — no entity, replication, authority, or resource scenarios.

No README exists. The state is: working smoke layer, very thin coverage, clear path to expand.

### Bench / perf (`test/bench/`, `benches/`, `crucible.toml`)

Crucible integration: baseline named `perf_v0`, post-assert via `naia-bench-report --assert-wins`. The `wins.rs` module defines pass/fail criteria for the benchmark suite (29 wins, 0 losses confirmed at last audit).

**Gaps:** No per-subsystem micro-benchmarks (spawn throughput, event dispatch overhead). Wins criteria are defined but not documented (what threshold = a "win"?). No historical baseline tracking per release.

### Loom & compile_fail

Loom covers `DirtySet` concurrency properties. Compile_fail covers 2 derive-macro violations (`immutable_entity_property`, `immutable_property`). Both are minimal but correct.

---

## Quality issues (concrete)

### H-1 — `scenario.rs` alternation enforcement is commented out
**File:** `test/harness/src/harness/scenario.rs:384, 546, 602`

The `LastOperation` enum (line 112) and `last_operation` field (line 177) were designed to enforce strict mutate→expect alternation. Three enforcement blocks are commented out. The field is set but never checked, making it dead state.

**Fix:** Either re-enable enforcement with clear panic messages, or remove the enum, field, and dead assignments entirely. The current state confuses readers about whether alternation is required.

### H-2 — Missing harness API for dynamic component operations
**Affected @PolicyOnly stubs:** `entity-replication-04`, `entity-replication-05`, `entity-replication-11`

`ServerMutateCtx` has no method to insert or remove a component on an entity that's already registered and in-scope. This blocks three live-test conversions.

**Fix:** Add `ServerMutateCtx::insert_component_on<C: ReplicatedComponent>(key, component)` and `remove_component_from<C>(key)`.

### H-3 — Missing server-side MessageEvent and AuthDenied tracking
**Affected @PolicyOnly stubs:** `server-events-05`, `server-events-10`

`server_events.rs` tracks `AuthGranted` and `AuthReset` but not `AuthDenied` or `MessageEvent`. The server-side message receipt is an important observable.

**Fix:** Add `auth_denied_count()` to `ServerEvents`. For `MessageEvent`, add `server_message_count(channel)`.

### H-4 — No static entity harness path
**Affected spec area:** `03_replication.feature` (no static-entity coverage)

`ServerMutateCtx::spawn_entity` always creates dynamic entities. There is no `spawn_static_entity` or post-spawn `mark_as_static` API in the harness. The production API (`enable_static_replication`) is untested via BDD.

**Fix:** Add `ServerMutateCtx::spawn_static_entity(key)` that calls `server.enable_static_replication(&entity)`.

### H-5 — Protocol gap: authority not cleared on `configure_replication(Public)`
**Affected @PolicyOnly stubs:** `entity-delegation-05`, `entity-authority-13`

When the server calls `configure_replication(Public)` on a previously-`Delegated` entity, **no message is sent to clear the client's authority status**. A client that held `Denied` retains that status indefinitely. This is a real missing protocol path, not a harness limitation.

**Fix:** In `configure_entity_replication` (server), when transitioning from `Delegated` → `Public`, send an authority-reset message to any client that has non-`Available` authority state for that entity.

### S-1 — Several step files approach or exceed 500-LOC limit
`state_assertions_entity.rs` (597), `event_assertions.rs` (534), `state_assertions_replication.rs` (517), `client_actions.rs` (496).

**Fix:** Split `state_assertions_entity.rs` into `state_assertions_auth.rs` + `state_assertions_messaging.rs`; the entity/component pieces stay. Evaluate others after this.

### S-2 — Missing "N ticks elapse" phrase binding
Many step bodies call `tick_n(ctx, N)` directly in Rust. Feature files can't express time passing without a binding.

**Fix:** Add `#[when("{int} ticks elapse")]` and `#[given("{int} ticks have elapsed")]` in `setup.rs` or `state_misc.rs`.

### B-1 — Wins criteria undocumented
`test/bench/src/wins.rs` defines what constitutes a benchmark win but doesn't explain the thresholds.

**Fix:** Add a comment header to `wins.rs` explaining the decision criteria.

---

## Coverage gaps — new scenarios needed

These represent behaviors currently not specced at all (not even as stubs) or stubs that are convertible with harness work:

### Gap A — Static entity replication (no coverage at all)
`03_replication.feature` has zero scenarios for the `as_static()` / `enable_static_replication()` path. This is a meaningful user-facing feature.

**New scenarios (5-6):**
- `static-entity-01` — Static entity spawns on client when entering scope
- `static-entity-02` — Static entity: no diff tracking after spawn (component update not replicated)
- `static-entity-03` — Static entity: insert-component-on-spawn → correct initial value received
- `static-entity-04` — Static entity: full snapshot sent on each new scope entry
- `static-entity-05` — Static entity: `panic!` on `insert_component` after construction

### Gap B — Dynamic component operations (3 stubs → live tests)
Requires H-2 first.

**Convert to live:**
- `entity-replication-04` — Component insert events fire for in-scope additions
- `entity-replication-05` — Component remove events fire for in-scope removals
- `entity-replication-11` — Component remove on out-of-scope is safe

### Gap C — Resource delegation end-to-end (thin coverage)
`07_resources.feature` has 9 real tests covering insert, diff, authority request/grant/release. Missing:

**New scenarios (3-4):**
- `resource-delegation-01` — Server configures resource as Delegated; client requests authority; client mutations replicate
- `resource-delegation-02` — Server reclaims authority via `configure_replicated_resource(Server)` after client held it
- `resource-latejoin-02` — Client connecting after dynamic resource is live receives current value

### Gap D — Authority cleared on reconfigure (protocol fix + 2 stubs → live tests)
Requires H-5 first.

**Convert to live:**
- `entity-delegation-05` — Disable delegation clears all authority status
- `entity-authority-13` — Disable delegation clears authority

### Gap E — Bevy adapter BDD expansion
9 scenarios covering connection only. Meaningful expansion targets:

**New scenarios in `bevy_specs/` (6-8):**
- Entity spawn with `CommandsExt::enable_replication` + verify client sees it
- Component insert replication via Bevy `Commands`
- Authority grant/deny via `CommandsExt::give_authority`
- Resource insert via `ServerCommandsExt::replicate_resource`
- Resource authority request via `ClientCommandsExt::request_resource_authority`

### Gap F — Server-side event observability (2 stubs → live tests)
Requires H-3 first.

**Convert to live:**
- `server-events-05` — MessageEvent fires per inbound message
- `server-events-10` — Authority denied event observable on server

---

## Plan

### T0 — Housekeeping (small, low-risk)

- [ ] **T0.1** — Re-enable or delete the `LastOperation` alternation enforcement (`scenario.rs:384, 546, 602`). If deleting: remove the enum and field too.
- [ ] **T0.2** — Document the wins criteria in `test/bench/src/wins.rs` (comment header).
- [ ] **T0.3** — Add `bevy_npa/README.md` explaining purpose, current state, and relationship to naia_npa.
- [ ] **T0.4** — Update `NAIA_PLAN.md` snapshot after each phase completes.

### T1 — Harness: new APIs

- [ ] **T1.1** — `ServerMutateCtx::insert_component_on<C>(key, value)` and `remove_component_from<C>(key)`. Unlocks Gap B.
- [ ] **T1.2** — `ServerMutateCtx::spawn_static_entity(key)`. Unlocks Gap A.
- [ ] **T1.3** — `ServerEvents::auth_denied_count()` and `server_message_count(channel)`. Unlocks Gap F.
- [ ] **T1.4** — Fix H-5 protocol gap: send authority-reset message when server transitions entity from `Delegated` → non-Delegated. Unlocks Gap D.

### T2 — Step bindings: new phrases

- [ ] **T2.1** — `#[given|when("{int} ticks elapse")]` phrase binding.
- [ ] **T2.2** — `"the server inserts {component} on {entity}"` and `"the server removes {component} from {entity}"` phrases. (Requires T1.1.)
- [ ] **T2.3** — `"a server-owned static entity exists"` / `"a static entity with {component}"` Given phrases. (Requires T1.2.)
- [ ] **T2.4** — Server-side event assertion phrases for MessageEvent and AuthDenied. (Requires T1.3.)
- [ ] **T2.5** — `"the authority status for {entity} is Available"` / `"is cleared"` Then phrase. (Supports Gap D.)

### T3 — New BDD scenarios

- [ ] **T3.1** — Write static entity Rule in `03_replication.feature` (Gap A, 5-6 new scenarios). Requires T1.2 + T2.3.
- [ ] **T3.2** — Convert `entity-replication-04/05/11` stubs to live tests (Gap B). Requires T1.1 + T2.2.
- [ ] **T3.3** — Write resource delegation scenarios in `07_resources.feature` (Gap C, 3-4 new scenarios).
- [ ] **T3.4** — Convert `entity-delegation-05` and `entity-authority-13` stubs to live tests (Gap D). Requires T1.4 + T2.5.
- [ ] **T3.5** — Convert `server-events-05` and `server-events-10` stubs to live tests (Gap F). Requires T1.3 + T2.4.

### T4 — Bevy BDD expansion

- [ ] **T4.1** — Add entity replication scenarios to `bevy_specs/` (spawn, component insert, scope). 3-4 scenarios.
- [ ] **T4.2** — Add authority scenarios to `bevy_specs/` (give_authority, deny). 2-3 scenarios.
- [ ] **T4.3** — Add resource scenarios to `bevy_specs/` (replicate_resource, request_resource_authority). 2-3 scenarios.

### T5 — Step binding cleanup

- [ ] **T5.1** — Split `state_assertions_entity.rs` (597 LOC) into `state_assertions_auth.rs` + `state_assertions_messaging.rs`.
- [ ] **T5.2** — Audit `when/client_actions.rs` (496 LOC) and `when/network_events_transport.rs` (481 LOC) for split opportunities.

---

## Acceptance criteria for "test infrastructure done"

1. `cargo run --manifest-path test/npa/Cargo.toml -- coverage --specs-root test/specs` reports **≥ 325 active** (up from 309), with zero new `@PolicyOnly` stubs added.
2. Static entity replication is covered by at least 4 live scenarios.
3. `entity-replication-04/05/11`, `entity-delegation-05`, `entity-authority-13`, `server-events-05/10` are all live tests (not stubs).
4. Bevy NPA has ≥ 20 scenarios (up from 9).
5. No step binding file exceeds 500 LOC.
6. `LastOperation` alternation: either enforced with panic, or fully removed — no commented-out enforcement.
7. `cargo check --workspace` clean, 0 build warnings.

---

## Revised scenario count target

The old target of **350** was aspirational. Given that ~77 of the 309 are already `@PolicyOnly` stubs (already counted), and many policy-only contracts are genuinely untestable (wallclock, packet inspection, concurrent ops, transport layer), the realistic ceiling from conversion alone is ~319.

The new target of **≥ 325 active with real steps** is achievable through T3 (new scenarios in Gaps A, C) and is a more honest metric than raw count. If T4 (Bevy expansion) reaches ≥ 20 scenarios, that's an additional quality story there.

The old 350 target is retired from `NAIA_PLAN.md` in favour of the criteria above.

---

## Archives

*No archived phases yet — this plan was created 2026-05-08.*
