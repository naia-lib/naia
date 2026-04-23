# Test Strategy — Scaling Naia to 10K+ Entities

**Status:** Proposed — 2026-04-23
**Companion to:** `SCALE_10K_ENTITIES_PLAN.md`
**Purpose:** Define how to refactor Naia internals across Phases 1–5 with high confidence that no existing behavior regresses, and how to spec/verify the new behaviors (`ScopeExit::Persist`, push-based queues, `SpawnWithComponents`, immutable components) under Naia's existing spec-driven-development discipline.

---

## 1. Audit of current test infrastructure

Naia already has a serious, well-designed test apparatus. Summary of what I found walking `test/`, `shared/derive/tests`, and `shared/src/world/sync/tests`:

### 1.1 Spec-driven BDD is the law of the land

- **Contracts** live in `test/specs/contracts/*.spec.md` — 15 numbered contracts covering connection, transport, messaging, ticks, observability, scopes, replication, ownership, publication, delegation, authority, server events API, client events API, and world integration.
- **Feature files** (Gherkin) in `test/specs/features/*.feature` — each contract has a paired `@Feature(...)` file with `@Rule(NN)` and `@Scenario(NN)` structure.
- **Step bindings** in `test/tests/src/steps/*.rs` — each `Given`/`When`/`Then` line in a feature file is a labeled Rust function registered via `namako_engine` macros.
- **Policy B** (from `test/specs/README.md`): every contract MUST have at least one obligation labeled `t1`; tests reference contracts via `scenario.spec_expect("06_entity_scopes.t3: ...", ...)`; mechanical adequacy is machine-enforced.
- **Spec tool** (`naia_spec_tool`): `verify`, `lint`, `coverage`, `adequacy`, `traceability`, `packet` — CI-grade gating. Golden files + deterministic timestamps. `certification.json` and `run_report.json` track the certified state with blake3 hashes of (features, steps, resolved plan, run report).
- **Harness** (`test/harness/src/harness/`): `Scenario` driver + `MutateCtx` / `ExpectCtx` + per-tick stepping + event history tracking (`TrackedClientEvent` / `TrackedServerEvent`) + link conditioner + traffic pause + raw packet injection. Supports multi-client scenarios, per-client event history, "observed X before Y" assertions.

### 1.2 Unit tests where it matters

- `shared/src/world/sync/tests/` — `engine.rs`, `migration.rs`, `command_validation_tests.rs`, `bulletproof_migration.rs`, `perfect_migration_tests.rs`. Dense unit coverage for the command-pipeline and reliable-channel migration corner cases.
- `shared/src/messages/tests/fragment.rs` — message fragmentation.
- `shared/tests/derive_*.rs` — derive-macro tests for `Replicate`, `Message`, struct/enum/tuple-struct/unit-struct.

### 1.3 Where coverage is thin — THIS MATTERS FOR THE REFACTOR

Scenario counts per feature file (actual counts from `grep -c "Scenario:"`, `@Deferred` count in parens — anything deferred is a non-running placeholder):

| # | Feature | Scenarios | Deferred | Touched by Phase |
|---|---|---:|---:|---|
| 00 | Common definitions | 12 | 1 | — |
| 01 | Connection lifecycle | 14 | 0 | — |
| 02 | Transport | 17 | 0 | **Phase 4** (new EntityMessageType) |
| 03 | Messaging | 9 | 0 | — |
| 04 | Time, ticks, commands | 3 | 0 | (21 obligations — thin) |
| 05 | Observability metrics | 7 | 0 | **all phases** (metric surface) |
| 06 | Entity scopes | 15 | **6** | **Phase 1, 2** |
| 07 | Entity replication | 5 | **2** | **Phase 3, 4** |
| 08 | Entity ownership | 3 | 0 | Phase 1 (via ReplicationConfig) |
| 09 | Entity publication | 3 | 0 | Phase 1 |
| 10 | **Entity delegation** | **0** | **all** | **Phase 1** — *this is a red alert* |
| 11 | Entity authority | 0 | all | Phase 1 |
| 12 | Server events API | 0 | all | all phases |
| 13 | Client events API | 0 | all | all phases |
| 14 | World integration | 0 | all | Phase 4 |

**The delegation feature file (contract 10, 17 obligations) has zero scenarios implemented.** `ReplicationConfig::Delegated` is *exactly* what cyberlith tiles use and *exactly* what Phase 1 refactors. Touching it without coverage is the definition of the plane taking off mid-rebuild.

Contracts 11, 12, 13, 14 are similarly bare. Contract 04 (ticks/commands) has 21 obligations but only 3 scenarios — a lot of normative behavior is claimed in the spec that has no BDD enforcement today.

### 1.4 Under-instrumented internal observability

For the refactor specifically, we need to observe things that today's test harness doesn't surface:

- `GlobalDiffHandler.mut_receiver_builders.len()` (per-entity per-kind registrations).
- `UserDiffHandler.receivers.len()` per connection.
- `MutChannelData.receiver_map.len()` per component.
- Dirty-set size (post-Win-3).
- `scope_change_queue.len()` (post-Win-2).
- Wire bytes emitted per packet, per command, per entity.
- Allocation counts per-tick for zero-allocation-on-idle assertions.

None of these are observable from `ExpectCtx` today. We'll add a feature-gated telemetry surface.

---

## 2. Coverage strategy — three layers

Three complementary layers of safety net, each closing a different failure mode:

### Layer A — Close existing BDD gaps BEFORE refactoring (Phase 0)

Required before any Phase 1 code lands. For contracts touched by the refactor, convert every `@Deferred` scenario to running, and add scenarios for obligations (`t1..tN`) that have no bound test.

Prioritized:
1. **Contract 10 (Delegation) — all 17 obligations.** Without this, Phase 1 cannot be safely refactored. Write scenarios for: publish-then-delegate migration, first-request-wins arbitration, denied status, release back to available, scope-loss ends authority, disconnect releases authority, write-without-granted panics.
2. **Contract 11 (Authority) — 16 obligations.** Parallel to 10; covers the client-side view of the same state machine.
3. **Contract 07 (Replication) — undefer scenarios 04 (GlobalEntity stability) and 05 (server wins conflict).**
4. **Contract 06 (Scopes) — undefer the 6 deferred scenarios.** These include re-entry new-lifetime semantics, owner exclude-is-noop, roomless-include-is-noop — all directly relevant to Phase 1/2.
5. **Contracts 08, 09** — flesh out the remaining obligations.
6. **Contracts 12, 13, 14** — at minimum smoke scenarios for the event-API surface, so a refactor that inadvertently changes event ordering or payloads gets caught.

**Acceptance gate:** `cargo run -p naia_spec_tool -- adequacy --strict` passes for contracts 06/07/08/09/10/11. No Phase 1 PR merges until this is green on `main`.

### Layer B — Golden-trace regression fence during refactors

Phases 2, 3, 4 are *behaviorally transparent* refactors — the user-observable event sequence and wire bytes should be identical (Phase 4 changes wire framing in a specific, coalesced way but preserves the event stream). Gherkin-level scenarios catch "did the right events fire in the right order," but they won't catch "same events, different wire-level shape" or "same events, subtly reordered across a tick boundary."

The mechanism: **record a canonical trace of multi-client scenarios before the refactor; assert identical trace after.**

A trace is a deterministic, hash-stable sequence of:
- Wire packets (by `(direction, packet_index, bytes)`).
- Decoded events emitted to client and server event APIs (`TrackedClientEvent` / `TrackedServerEvent` already exist).
- Per-tick summaries: `(tick, packets_sent, packets_received, bytes_sent, bytes_received, events_emitted)`.

Add to harness:

```rust
// test/harness/src/harness/scenario.rs
impl Scenario {
    pub fn enable_trace_capture(&mut self) { ... }
    pub fn finish_trace(&mut self) -> Trace { ... }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Trace {
    pub packets: Vec<(Direction, PacketIndex, Vec<u8>)>,
    pub server_events: Vec<TrackedServerEvent>,
    pub client_events: HashMap<ClientKey, Vec<TrackedClientEvent>>,
    pub tick_summary: Vec<TickSummary>,
}
```

Golden traces get written once on a sanctioned baseline commit, stored in `test/specs/golden_traces/*.json`, and re-checked on every PR. The spec tool extends with a `traces` subcommand:

```bash
cargo run -p naia_spec_tool -- traces record <scenario>  # regenerate golden
cargo run -p naia_spec_tool -- traces check              # compare against golden
```

Per-phase expectations:
- **Phase 2** (push-based scope queue): traces must be byte-identical. The refactor is pure internal plumbing.
- **Phase 3** (push-based update set): traces must be byte-identical. Same work, same output, different internal bookkeeping.
- **Phase 4** (`SpawnWithComponents`): **event-stream** identical; **wire-byte** trace differs and gets a new golden that is *smaller*. Commit the new golden in the same PR that lands Phase 4; code-review checks byte count drops as expected.

Phase 1 and Phase 5 introduce new *observable* behavior (ScopeExit::Persist changes scope-exit outcomes; immutable components forbid mutation API). They get new scenarios (Layer C) rather than golden-trace preservation.

### Layer C — New contracts + scenarios for new behavior

Each Phase that changes observable behavior gets a new contract file and feature file. Structure mirrors existing contracts (normative statement + obligations `t1..tN` + feature scenarios + step bindings).

#### Phase 1 — new contract `15_scope_exit_policy.spec.md`

**Obligations (draft):**
- **t1**: Default `ScopeExit` is `Despawn`. Unchanged semantics from pre-refactor contract 06.
- **t2**: `ScopeExit::Persist` entity leaving scope MUST NOT emit Despawn on the client.
- **t3**: `ScopeExit::Persist` entity MUST stop receiving updates while out of scope.
- **t4**: Re-entering scope with no mutations during absence MUST produce zero update bytes.
- **t5**: Re-entering scope with mutations during absence MUST emit exactly the accumulated DiffMask.
- **t6**: `ScopeExit::Persist` + server-side despawn during absence MUST emit Despawn on resume.
- **t7**: `ScopeExit::Persist` + component-insert during absence MUST emit InsertComponent on resume.
- **t8**: `ScopeExit::Persist` + component-remove during absence MUST emit RemoveComponent on resume.
- **t9**: `ScopeExit::Persist` + disconnect cleans up per-user state exactly like `Despawn`.

Feature scenarios are 1:1 with obligations, plus one scenario per non-trivial interaction with delegation (persist+delegated: authority is dropped on scope loss but entity persists on client).

#### Phase 2 — new contract `16_scope_propagation_model.spec.md`

Non-behavior-changing, but it's worth encoding the invariant we're enforcing:

- **t1**: Scope-state after N scope-API calls is identical whether evaluated eagerly (old) or lazily (new).
- **t2**: Idle room with N entities and no scope changes produces zero scope-evaluation work per tick (observed via allocation counter).
- **t3**: `scope_change_queue` drains to empty within the tick the change was enqueued.
- **t4**: Enqueuing for an unknown user or entity is a no-op (existing silent-ignore semantics preserved).

#### Phase 3 — new contract `17_update_dispatch_model.spec.md`

- **t1**: Property mutation → corresponding update packet within ≤1 tick (unchanged).
- **t2**: Idle entity (no mutations) produces zero update-write work per tick (observed via dirty-set emptiness and allocation counter).
- **t3**: Multiple mutations to same property within a tick collapse to single update (existing contract, now assertable).
- **t4**: Mutation on Property A while Property B idle MUST NOT touch B's diff bits on the wire.
- **t5**: Dropped-update replay behavior (`dropped_update_cleanup`) is unchanged.
- **t6**: Per-user diff independence: user A's received updates MUST NOT be affected by user B's packet loss.

#### Phase 4 — new contract `18_spawn_with_components_wire.spec.md`

- **t1**: Spawning an entity with K initial components emits exactly one `EntityCommand::SpawnWithComponents` (not 1 Spawn + K InsertComponent).
- **t2**: Client observes the same `EntityEvent::SpawnEntity` followed by K `EntityEvent::InsertComponent` events as before. Ordering preserved.
- **t3**: Spawning with zero initial components emits `EntityCommand::Spawn` (the legacy form) — no change.
- **t4**: Wire byte count for a 10K-tile scope-entry burst is lower than the legacy form. (Benchmark + lower-bound assertion, not byte-exact.)
- **t5**: Protocol-ID mismatch between old and new clients is rejected at handshake (existing mechanism).

#### Phase 5 — new contract `19_immutable_components.spec.md`

- **t1**: Component with `#[component(immutable)]` replicates initial state correctly on spawn.
- **t2**: Immutable component generates zero entries in `GlobalDiffHandler.mut_receiver_builders` and zero entries in any `UserDiffHandler.receivers`.
- **t3**: Immutable component with no later changes produces zero update-write work (weaker version of 17.t2 — always true, not conditional).
- **t4**: Removing and re-inserting an immutable component replicates the new value (the supported "mutation" path).
- **t5**: Immutable component + `ReplicationConfig::Delegated` is rejected at `configure_entity_replication` with a clear error.
- **t6** (compile-fail, via `trybuild`): `Property<T>` field inside `#[component(immutable)]` struct is a compile error.
- **t7** (compile-fail): `EntityProperty` field inside immutable component is a compile error.

---

## 3. New harness capabilities required

To support Layers B and C, the harness needs three extensions:

### 3.1 `AllocationSnapshot` API (feature-gated behind `test_utils`)

```rust
// test/harness/src/harness/scenario.rs
#[cfg(feature = "test_utils")]
impl Scenario {
    pub fn diff_handler_snapshot(&self) -> DiffHandlerSnapshot {
        // Reads GlobalDiffHandler.mut_receiver_builders.len() and
        // per-user UserDiffHandler.receivers.len() via internal accessor.
    }
    pub fn dirty_set_snapshot(&self, client: ClientKey) -> DirtySetSnapshot {
        // Post-Win-3: reads EntityUpdateManager dirty set size.
    }
    pub fn scope_queue_len(&self) -> usize { ... }
}

pub struct DiffHandlerSnapshot {
    pub global_receivers: usize,
    pub user_receivers: HashMap<ClientKey, usize>,
    pub per_component_kind: HashMap<ComponentKind, usize>,
}
```

Internal accessors behind `#[cfg(feature = "test_utils")]` in `naia-server` and `naia-shared`. These crates already have the `test_utils` feature (see harness `Cargo.toml`), so we piggyback.

### 3.2 Wire trace capture

```rust
#[cfg(feature = "test_utils")]
impl Scenario {
    pub fn enable_trace_capture(&mut self);
    pub fn take_trace(&mut self) -> Trace;
}
```

Hook into the local transport already used by the harness. Every send/recv call records `(direction, packet_index, timestamp_tick, bytes)`. `Trace` derives `Serialize`/`Deserialize` via serde so golden files are JSON.

Hash stability: pre-refactor baseline trace has a blake3 hash; CI asserts the post-refactor trace of the same scenario hashes identically (for Phases 2 and 3) or differs only in the expected wire-framing region (Phase 4).

### 3.3 `trybuild` compile-fail harness

Not in Naia today. Add `test/compile_fail/` with:

```
test/compile_fail/
  Cargo.toml
  tests/
    compile_fail.rs           # trybuild entry
  fixtures/
    immutable_property.rs
    immutable_property.stderr
    immutable_entity_property.rs
    immutable_entity_property.stderr
    delegated_immutable.rs    # if we go with compile-time rejection
    delegated_immutable.stderr
```

`trybuild` is a well-established crate for this pattern; adds ~30s to the test run.

---

## 4. Phase-by-phase rollout sequence

Gates are strict: each phase gate must pass cleanly before the next begins.

### Phase 0 — Close BDD gaps (no production code change)

**Deliverables:**
- Scenarios + step bindings for contracts 10 (Delegation) and 11 (Authority) — every obligation.
- Undefer all 6 scenarios in contract 06 and both deferred scenarios in contract 07.
- Smoke scenarios for contracts 12, 13, 14 (at least one scenario per major event type).

**Gate:** `naia_spec_tool verify` is green. `adequacy --strict` passes for 06, 07, 08, 09, 10, 11. No new obligation unbound.

**Estimated effort:** This is the biggest block of work in the whole plan — the existing test harness is powerful, but 40+ scenarios need writing. Fortunately, the step bindings that exist for 06/07 give a template and many `Given`/`When`/`Then` phrases are reusable.

### Phase 0.5 — Trace-capture harness + allocation snapshots

**Deliverables:** §3.1, §3.2 infrastructure. One sample scenario using each. Golden-trace regeneration script (`naia_spec_tool traces record`).

**Gate:** Golden trace captured for representative scenarios from contracts 06, 07, 10.

### Phase 1 — `ReplicationConfig` struct + `ScopeExit::Persist`

**Deliverables:** Code per `SCALE_10K_ENTITIES_PLAN.md` §3 Phase 1. New contract `15_scope_exit_policy.spec.md` + feature + steps.

**Gate:**
- All Phase 0 tests still pass (no regression in delegation/authority/scope/replication).
- Contract 15 passes all obligations.
- Golden traces for contracts 06, 07, 10 are byte-identical to Phase 0.5 baseline for scenarios that don't opt into `Persist`.
- `adequacy --strict` passes for contract 15.

### Phase 2 — Push-based scope-change tracking

**Deliverables:** Code per plan §3 Phase 2. New contract `16_scope_propagation_model.spec.md`.

**Gate:**
- Contract 06 feature scenarios pass unchanged.
- Contract 16 passes all obligations, including idle-tick allocation assertion.
- Golden traces for multi-client scope scenarios are byte-identical.

### Phase 3 — Push-based update candidate set

**Deliverables:** Code per plan §3 Phase 3. New contract `17_update_dispatch_model.spec.md`.

**Gate:**
- Contract 07 passes unchanged.
- Contract 17 passes, including idle-entity-allocation and per-user diff-independence.
- Golden traces unchanged.

### Phase 4 — `SpawnWithComponents` coalesced command

**Deliverables:** Code per plan §3 Phase 4. New contract `18_spawn_with_components_wire.spec.md`. Updated goldens (commit documents the delta).

**Gate:**
- Contract 07 event-stream assertions pass unchanged.
- Contract 18 passes including the byte-count-lower assertion.
- Wire-byte golden is smaller by at least the expected framing overhead (10K tiles × per-InsertComponent framing saved).

### Phase 5 — Immutable components

**Deliverables:** Code per plan §3 Phase 5. New contract `19_immutable_components.spec.md`. Compile-fail fixtures under `test/compile_fail/`.

**Gate:**
- All prior contracts unchanged.
- Contract 19 passes including allocation-count-zero obligations.
- `trybuild` tests all pass, rejecting the forbidden `Property<T>` / `EntityProperty` / `Delegated` combinations with clear error messages.

---

## 5. Coverage maintenance policy

Once Phase 0 lands, enforce:

1. **No merge without green `verify`.** CI gate runs `cargo run -p naia_spec_tool -- verify` including `adequacy --strict` and `traces check`.
2. **Every new public API surface in `naia-server` / `naia-client` / `naia-shared` gets a contract with ≥1 `t1` obligation before merge.** The existing Policy B applies to new code too, not just initial specs.
3. **Any PR that regenerates a golden trace must include a paragraph in the commit message explaining why the wire shape changed and link to the contract obligation authorizing it.**
4. **`@Deferred` scenarios are a bug**, not a commitment. If a scenario can't be written, the contract obligation should be removed or restated.

---

## 6. Estimated effort

Rough T-shirt sizes. Phase 0 dominates; everything else benefits from it compounding.

| Phase | Effort | Rationale |
|---|---|---|
| Phase 0 | **L** (2–3 weeks full-time equivalent) | ~40 new scenarios, many reused step phrases; 10/11/14 are greenfield. |
| Phase 0.5 | S (1 week) | Trace capture is mechanical; allocation accessors are thin. |
| Phase 1 | M (1–2 weeks) | Struct refactor has wide call-site surface but mechanical; ScopeExit::Persist is new but contained. |
| Phase 2 | S (1 week) | Localized to scope pipeline. |
| Phase 3 | M (1–2 weeks) | Per-client dirty-set threading needs care around the existing Arc<RwLock> topology. |
| Phase 4 | S (1 week) | Single new EntityCommand variant + wire plumbing. |
| Phase 5 | M (1–2 weeks) | Derive macro branches + trybuild harness + allocation-zero proofs. |

Phase 0 is non-negotiable. Everything else is gated behind it.

---

## 7. Open questions

1. **Should `traces record` live in CI or be a dev-local command?** Recording is human-gated (we need to approve that the new trace is correct). `traces check` is CI-enforced. Propose: `record` is manual, `check` is CI, goldens live in the repo with a CODEOWNERS gate on the `golden_traces/` directory.
2. **Do we need property-based fuzzing for Phase 2/3?** Current tests are scenario-driven. For "idle-tick zero work" and "mutation → update" invariants, a small property-based generator (using `proptest` against the scenario API) would catch patterns scenarios might miss. Proposed: add to Phase 3 as a "stretch" deliverable.
3. **Protocol version bump mechanics for Phase 4.** Naia has `ProtocolId` (`shared/src/protocol_id.rs`) via hash of the protocol definition. Adding a new `EntityMessageType` variant will change the hash naturally, so handshake will reject old clients — correct behavior. Document this explicitly in contract 18 so it's not a surprise.
