# Landing Strategy — Scaling Naia to 10K+ Entities

**Status:** Proposed — 2026-04-23
**Companion to:** `SCALE_10K_ENTITIES_PLAN.md` (the *what*: architecture, Wins 1–5, Phases 1–5).
**Purpose:** The *how*. One doc covering every non-architectural obligation required to land the refactor with full behavior preservation, measurable perf gains, and the codebase in strictly better shape than where it started.

This is the "don't wave your hands" layer. Every recommendation is verifiable, gated, and specific to things Naia doesn't already have.

---

## Table of Contents

- [0. How to Use This Document](#0-how-to-use-this-document)
- [1. Audit of Current Test Infrastructure](#1-audit-of-current-test-infrastructure)
  - [1.1 Spec-driven BDD is the law of the land](#11-spec-driven-bdd-is-the-law-of-the-land)
  - [1.2 Unit tests where it matters](#12-unit-tests-where-it-matters)
  - [1.3 Where coverage is thin](#13-where-coverage-is-thin)
  - [1.4 Under-instrumented internal observability](#14-under-instrumented-internal-observability)
- [2. Coverage Strategy — Three Layers](#2-coverage-strategy--three-layers)
  - [2.1 Layer A — Close existing BDD gaps (Phase 0)](#21-layer-a--close-existing-bdd-gaps-phase-0)
  - [2.2 Layer B — Golden-trace regression fence](#22-layer-b--golden-trace-regression-fence)
  - [2.3 Layer C — New contracts for new behavior](#23-layer-c--new-contracts-for-new-behavior)
- [3. Pre-flight Code-Hygiene Audit](#3-pre-flight-code-hygiene-audit)
  - [3.1 Nine `todo!()` in host_world_manager.rs](#31-nine-todo-in-host_world_managerrs)
  - [3.2 Commented-out / dead code sweep](#32-commented-out--dead-code-sweep)
  - [3.3 process_delivered_commands dispatch shape](#33-process_delivered_commands-dispatch-shape)
  - [3.4 Rustdoc gaps on touched types](#34-rustdoc-gaps-on-touched-types)
- [4. CI Gates](#4-ci-gates)
  - [4.1 `naia_spec_tool verify` in CI](#41-naia_spec_tool-verify-in-ci)
  - [4.2 `traces check` in CI](#42-traces-check-in-ci)
  - [4.3 `cargo-audit` + `cargo-deny`](#43-cargo-audit--cargo-deny)
  - [4.4 Tighten clippy](#44-tighten-clippy)
  - [4.5 `cargo doc --no-deps`](#45-cargo-doc---no-deps)
- [5. Per-Phase PR Checklist](#5-per-phase-pr-checklist)
- [6. Phase-by-Phase Rollout](#6-phase-by-phase-rollout)
  - [6.0 Phase 0 — Close BDD gaps](#60-phase-0--close-bdd-gaps)
  - [6.0.5 Phase 0.5 — Trace-capture harness + allocation snapshots](#605-phase-05--trace-capture-harness--allocation-snapshots)
  - [6.1 Phase 1 — `ReplicationConfig` struct + `ScopeExit::Persist`](#61-phase-1--replicationconfig-struct--scopeexitpersist)
  - [6.2 Phase 2 — Push-based scope-change tracking](#62-phase-2--push-based-scope-change-tracking)
  - [6.3 Phase 3 — Push-based update candidate set](#63-phase-3--push-based-update-candidate-set)
  - [6.4 Phase 4 — `SpawnWithComponents` coalesced command](#64-phase-4--spawnwithcomponents-coalesced-command)
  - [6.5 Phase 5 — Immutable components](#65-phase-5--immutable-components)
- [7. New Harness Capabilities](#7-new-harness-capabilities)
  - [7.1 `AllocationSnapshot` API](#71-allocationsnapshot-api)
  - [7.2 Wire trace capture](#72-wire-trace-capture)
  - [7.3 `trybuild` compile-fail harness](#73-trybuild-compile-fail-harness)
- [8. Benchmarking Discipline](#8-benchmarking-discipline)
  - [8.1 Add a criterion benches crate](#81-add-a-criterion-benches-crate)
  - [8.2 Baseline-per-phase](#82-baseline-per-phase)
  - [8.3 Perf regression gate (deferred)](#83-perf-regression-gate-deferred)
- [9. Concurrency Correctness — loom for Phase 3](#9-concurrency-correctness--loom-for-phase-3)
  - [9.1 Add a loom test crate](#91-add-a-loom-test-crate)
  - [9.2 Required cases](#92-required-cases)
- [10. Observability — tracing + metrics](#10-observability--tracing--metrics)
  - [10.1 Migrate hot paths to `tracing`](#101-migrate-hot-paths-to-tracing)
  - [10.2 Expose new metrics via contract 05](#102-expose-new-metrics-via-contract-05)
- [11. Wire-format Safety (Phase 4)](#11-wire-format-safety-phase-4)
  - [11.1 Fuzz the new decoder](#111-fuzz-the-new-decoder)
  - [11.2 Fragmentation scenario for SpawnWithComponents](#112-fragmentation-scenario-for-spawnwithcomponents)
  - [11.3 Protocol version bump documentation](#113-protocol-version-bump-documentation)
- [12. API Documentation + Migration](#12-api-documentation--migration)
  - [12.1 Migration guide for downstream](#121-migration-guide-for-downstream)
  - [12.2 Canonical example: 10K-entity demo](#122-canonical-example-10k-entity-demo)
  - [12.3 `#[deny(missing_docs)]` on new modules](#123-denymissing_docs-on-new-modules)
- [13. Feature-flag Rollout](#13-feature-flag-rollout)
  - [13.1 Flag: `v2_push_pipeline`](#131-flag-v2_push_pipeline)
  - [13.2 Stabilization criteria](#132-stabilization-criteria)
  - [13.3 Anti-pattern to avoid](#133-anti-pattern-to-avoid)
- [14. Coverage Maintenance Policy](#14-coverage-maintenance-policy)
- [15. Estimated Effort](#15-estimated-effort)
- [16. Open Questions](#16-open-questions)
- [17. Deliberately Out of Scope](#17-deliberately-out-of-scope)
- [18. End-State Vision](#18-end-state-vision)

---

## 0. How to Use This Document

Three reading paths, depending on what you're doing:

**Starting a phase.** Read [§6.N](#6-phase-by-phase-rollout) for that phase's deliverables and acceptance gate, then [§5](#5-per-phase-pr-checklist) for the checklist you'll need to satisfy at PR time. Cross-reference `SCALE_10K_ENTITIES_PLAN.md` §3 for the architectural intent.

**Writing code mid-phase.** Consult the topical sections the checklist points to — [§3](#3-pre-flight-code-hygiene-audit) for hygiene, [§8](#8-benchmarking-discipline) for benches, [§9](#9-concurrency-correctness--loom-for-phase-3) for loom (Phase 3), [§10](#10-observability--tracing--metrics) for tracing, [§11](#11-wire-format-safety-phase-4) for fuzz (Phase 4), [§12](#12-api-documentation--migration) for docs.

**Opening a PR.** Copy [§5](#5-per-phase-pr-checklist) into the PR description and tick every box. Every unticked box is a hold on merge.

Treat [§18](#18-end-state-vision) as the north star — the concrete description of what "done" looks like.

---

## 1. Audit of Current Test Infrastructure

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

### 1.3 Where coverage is thin

This matters for the refactor. Scenario counts per feature file (actual counts from `grep -c "Scenario:"`, `@Deferred` count in parens — anything deferred is a non-running placeholder):

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
| 10 | **Entity delegation** | **0** | **all** | **Phase 1** — *red alert* |
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

None of these are observable from `ExpectCtx` today. We add a feature-gated telemetry surface in [§7.1](#71-allocationsnapshot-api).

---

## 2. Coverage Strategy — Three Layers

Three complementary layers of safety net, each closing a different failure mode.

### 2.1 Layer A — Close existing BDD gaps (Phase 0)

Required before any Phase 1 code lands. For contracts touched by the refactor, convert every `@Deferred` scenario to running, and add scenarios for obligations (`t1..tN`) that have no bound test.

Prioritized:

1. **Contract 10 (Delegation) — all 17 obligations.** Without this, Phase 1 cannot be safely refactored. Write scenarios for: publish-then-delegate migration, first-request-wins arbitration, denied status, release back to available, scope-loss ends authority, disconnect releases authority, write-without-granted panics.
2. **Contract 11 (Authority) — 16 obligations.** Parallel to 10; covers the client-side view of the same state machine.
3. **Contract 07 (Replication) — undefer scenarios 04 (GlobalEntity stability) and 05 (server wins conflict).**
4. **Contract 06 (Scopes) — undefer the 6 deferred scenarios.** These include re-entry new-lifetime semantics, owner exclude-is-noop, roomless-include-is-noop — all directly relevant to Phase 1/2.
5. **Contracts 08, 09** — flesh out the remaining obligations.
6. **Contracts 12, 13, 14** — at minimum smoke scenarios for the event-API surface, so a refactor that inadvertently changes event ordering or payloads gets caught.

**Acceptance gate:** `cargo run -p naia_spec_tool -- adequacy --strict` passes for contracts 06/07/08/09/10/11. No Phase 1 PR merges until this is green on `main`.

### 2.2 Layer B — Golden-trace regression fence

Phases 2, 3, 4 are *behaviorally transparent* refactors — the user-observable event sequence and wire bytes should be identical (Phase 4 changes wire framing in a specific, coalesced way but preserves the event stream). Gherkin-level scenarios catch "did the right events fire in the right order," but they won't catch "same events, different wire-level shape" or "same events, subtly reordered across a tick boundary."

The mechanism: **record a canonical trace of multi-client scenarios before the refactor; assert identical trace after.**

A trace is a deterministic, hash-stable sequence of:

- Wire packets (by `(direction, packet_index, bytes)`).
- Decoded events emitted to client and server event APIs (`TrackedClientEvent` / `TrackedServerEvent` already exist).
- Per-tick summaries: `(tick, packets_sent, packets_received, bytes_sent, bytes_received, events_emitted)`.

Harness extensions (details in [§7.2](#72-wire-trace-capture)):

```rust
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

### 2.3 Layer C — New contracts for new behavior

Each phase that changes observable behavior gets a new contract file and feature file. Structure mirrors existing contracts (normative statement + obligations `t1..tN` + feature scenarios + step bindings).

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
- **t6**: `SpawnWithComponents` with payload exceeding MTU is fragmented and reassembled with no observable difference from small-payload case. (See [§11.2](#112-fragmentation-scenario-for-spawnwithcomponents).)

#### Phase 5 — new contract `19_immutable_components.spec.md`

- **t1**: Component with `#[component(immutable)]` replicates initial state correctly on spawn.
- **t2**: Immutable component generates zero entries in `GlobalDiffHandler.mut_receiver_builders` and zero entries in any `UserDiffHandler.receivers`.
- **t3**: Immutable component with no later changes produces zero update-write work (weaker version of 17.t2 — always true, not conditional).
- **t4**: Removing and re-inserting an immutable component replicates the new value (the supported "mutation" path).
- **t5**: Immutable component + `ReplicationConfig::Delegated` is rejected at `configure_entity_replication` with a clear error.
- **t6** (compile-fail, via `trybuild`): `Property<T>` field inside `#[component(immutable)]` struct is a compile error.
- **t7** (compile-fail): `EntityProperty` field inside immutable component is a compile error.

---

## 3. Pre-flight Code-Hygiene Audit

Before any Phase 1 code lands, walk the files the refactor touches and resolve tech debt that would otherwise compound. These are not nice-to-haves; they're liabilities the refactor would carry forward.

### 3.1 Nine `todo!()` in host_world_manager.rs

`shared/src/world/host/host_world_manager.rs:268-292`:

```rust
EntityMessage::Spawn(_) => {
    todo!("Implement EntityMessage::<HostEntity>::Spawn handling");
}
// ... 8 more variants: Despawn, InsertComponent, RemoveComponent,
// Publish, Unpublish, EnableDelegation, DisableDelegation, SetAuthority
```

This is the host-side `process_incoming_messages` branch. Phase 1 touches delegation and Phase 4 adds a new `EntityMessage` variant — both of which increase the likelihood of this branch being hit.

**Required before Phase 1:**

- Determine for each variant: is the branch genuinely unreachable (delete), or is it a real hole (implement)?
- If unreachable, replace `todo!()` with a documented `unreachable!()` naming the invariant.
- Add a `#[cfg(debug_assertions)]` log at the `match` entry so if a message variant *does* reach here in prod, we see it in our telemetry before the `unreachable!()` panics.

**Acceptance:** zero `todo!()` in files the refactor touches. The 11 other `todo!()` occurrences across the repo are out-of-scope but noted.

### 3.2 Commented-out / dead code sweep

- `shared/src/world/host/host_world_manager.rs:374-420` — `fn on_delivered_migrate_response` has dead commented-out code paths. Either delete or implement.
- `shared/src/world/update/user_diff_handler.rs:76-78` — `pub fn has_diff_mask` commented out. Delete.
- `shared/src/world/update/global_diff_handler.rs:37-41` — commented-out `info!` call. Either rewire via the `log` crate properly or delete.

**Acceptance:** `grep -n "^\s*//.*pub fn\|^\s*//.*fn\|^\s*//.*info!\|^\s*//.*todo!" shared/src/` returns nothing in files touched by the refactor.

### 3.3 process_delivered_commands dispatch shape

`host_world_manager.rs:194-253` — this function fans delivered messages out to `on_delivered_spawn_entity` (a stub), `on_delivered_despawn_entity`, `on_delivered_insert_component` (a no-op with a comment), `on_delivered_remove_component`. The stubs are load-bearing — the system works today because the stubs happen to be correct — but the shape is confusing.

**Required:** either fold the stubs inline with a clarifying comment, or keep the dispatch but remove the stubs that do nothing. This file's complexity is about to grow with the pause/resume machinery; clean it now while we have the context.

### 3.4 Rustdoc gaps on touched types

`ReplicationConfig`, `PropertyMutator`, `MutChannel`, `HostEntityChannel`, `GlobalEntityRecord`, `UserDiffHandler` — the refactor adds to these and exposes some to a wider surface. Today most have a one-line `///` or none.

**Required:** for each type the refactor touches, the post-refactor state has at minimum:

- A paragraph `///` on the type itself explaining role + lifetime.
- `# Examples`, `# Panics`, `# Errors` sections on public methods where applicable.

---

## 4. CI Gates

### 4.1 `naia_spec_tool verify` in CI

Today's `.github/workflows/main.yml` runs fmt + clippy + tests per-package + wasm targets. It does NOT run `cargo run -p naia_spec_tool -- verify`. Policy B (every contract obligation has a labeled test) is enforced only locally by convention.

**Required:** add a job to `main.yml`:

```yaml
spec-verify:
  name: Spec Adequacy
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v3
    - uses: Swatinem/rust-cache@v2
    - name: Verify specs, tests, adequacy
      run: cargo run -p naia_spec_tool -- verify
    - name: Strict adequacy
      run: cargo run -p naia_spec_tool -- adequacy --strict
```

Land this *before* Phase 0 so gap-closure work has the gate active from the start.

### 4.2 `traces check` in CI

Per [§2.2](#22-layer-b--golden-trace-regression-fence), golden wire traces are the regression fence for Phases 2/3/4. CI must enforce:

```yaml
- name: Golden traces
  run: cargo run -p naia_spec_tool -- traces check
```

### 4.3 `cargo-audit` + `cargo-deny`

Every recent push triggered a GitHub banner about 2 unresolved vulnerabilities (1 high, 1 moderate) on the default branch. These are compounding risk. Add a workflow:

```yaml
supply-chain:
  name: Supply Chain
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v3
    - name: cargo-audit
      uses: rustsec/audit-check@v1
    - name: cargo-deny
      uses: EmbarkStudios/cargo-deny-action@v1
```

Add a `deny.toml` at workspace root excluding known-and-accepted advisories with a written justification inline.

### 4.4 Tighten clippy

Current `RUSTFLAGS: -Dwarnings -Aclippy::new_without_default -Aclippy::derive-partial-eq-without-eq`. Add:

```yaml
env:
  RUSTFLAGS: "-Dwarnings -Wclippy::pedantic -Aclippy::module_name_repetitions -Aclippy::missing_errors_doc"
```

Pedantic lints catch a lot that plain clippy doesn't. Opt out only the noisy ones; keep the signal.

### 4.5 `cargo doc --no-deps`

```yaml
- name: Docs build clean
  run: cargo doc --workspace --no-deps
  env:
    RUSTDOCFLAGS: "-D warnings"
```

Rustdoc warnings-as-errors catches broken intra-doc links and unwritten `# Panics` sections.

---

## 5. Per-Phase PR Checklist

Every phase PR must have all of these present at review time. Missing any = return for fixes. Copy this block into `.github/pull_request_template.md` and link from each Phase-N PR.

```markdown
## Phase N — [Title]

### Code
- [ ] Implementation matches the Phase N scope in `SCALE_10K_ENTITIES_PLAN.md` §3
- [ ] No `todo!()`, `unimplemented!()`, or dead commented code added or left behind
- [ ] New public items have rustdoc with `# Examples` / `# Panics` / `# Errors`
- [ ] `tracing` spans added on hot paths (§10.1)

### Tests
- [ ] All existing contracts still pass (attach `naia_spec_tool verify` output)
- [ ] New contract `NN_...spec.md` added with obligations t1..tN
- [ ] New feature scenarios match obligations
- [ ] New step bindings (if any) have their hashes stable across runs
- [ ] Compile-fail fixtures (Phase 5 only) all produce expected errors

### Benchmarks
- [ ] `cargo bench` baseline captured on parent commit
- [ ] `cargo bench` baseline captured on this commit
- [ ] Delta meets Phase N acceptance claim from §8.2
- [ ] Benchmark results included in PR description

### Golden traces (Phases 2/3/4)
- [ ] `traces check` passes (Phases 2/3) or new goldens committed with diff explanation (Phase 4)

### Concurrency (Phase 3)
- [ ] `loom` tests added under `test/loom/`
- [ ] Loom CI job passes

### Supply chain
- [ ] `cargo audit` clean
- [ ] `cargo deny check` clean

### Docs
- [ ] Migration guide updated if user-visible API changed
- [ ] Demo added or updated (Phase 1 or Phase 5 only)
```

---

## 6. Phase-by-Phase Rollout

Gates are strict: each phase gate must pass cleanly before the next begins.

### 6.0 Phase 0 — Close BDD gaps

**Deliverables:**

- Scenarios + step bindings for contracts 10 (Delegation) and 11 (Authority) — every obligation.
- Undefer all 6 scenarios in contract 06 and both deferred scenarios in contract 07.
- Smoke scenarios for contracts 12, 13, 14 (at least one scenario per major event type).

**Gate:** `naia_spec_tool verify` is green. `adequacy --strict` passes for 06, 07, 08, 09, 10, 11. No new obligation unbound.

**Effort:** This is the biggest block of work in the whole plan — the existing test harness is powerful, but 40+ scenarios need writing. Fortunately, the step bindings that exist for 06/07 give a template and many `Given`/`When`/`Then` phrases are reusable.

### 6.0.5 Phase 0.5 — Trace-capture harness + allocation snapshots

**Deliverables:** [§7.1](#71-allocationsnapshot-api) + [§7.2](#72-wire-trace-capture) infrastructure. One sample scenario using each. Golden-trace regeneration script (`naia_spec_tool traces record`).

**Gate:** Golden trace captured for representative scenarios from contracts 06, 07, 10.

### 6.1 Phase 1 — `ReplicationConfig` struct + `ScopeExit::Persist`

**Deliverables:** Code per `SCALE_10K_ENTITIES_PLAN.md` §3 Phase 1. New contract `15_scope_exit_policy.spec.md` + feature + steps.

**Gate:**

- All Phase 0 tests still pass (no regression in delegation/authority/scope/replication).
- Contract 15 passes all obligations.
- Golden traces for contracts 06, 07, 10 are byte-identical to Phase 0.5 baseline for scenarios that don't opt into `Persist`.
- `adequacy --strict` passes for contract 15.

### 6.2 Phase 2 — Push-based scope-change tracking

**Deliverables:** Code per plan §3 Phase 2. New contract `16_scope_propagation_model.spec.md`. Gated behind the `v2_push_pipeline` feature flag ([§13.1](#131-flag-v2_push_pipeline)).

**Gate:**

- Contract 06 feature scenarios pass unchanged.
- Contract 16 passes all obligations, including idle-tick allocation assertion.
- Golden traces for multi-client scope scenarios are byte-identical, verified under both flag states.
- Bench [§8.2](#82-baseline-per-phase): idle-room scope-update tick time ≤ 5% of pre-refactor and absolute < 100µs for 10K entities.

### 6.3 Phase 3 — Push-based update candidate set

**Deliverables:** Code per plan §3 Phase 3. New contract `17_update_dispatch_model.spec.md`. Loom tests under `test/loom/` per [§9](#9-concurrency-correctness--loom-for-phase-3). Gated behind `v2_push_pipeline`.

**Gate:**

- Contract 07 passes unchanged.
- Contract 17 passes, including idle-entity-allocation and per-user diff-independence.
- Golden traces unchanged under both flag states.
- Loom CI job passes.
- Bench [§8.2](#82-baseline-per-phase): idle-entity per-tick update-dispatch tick time ≤ 5% of pre-refactor.

### 6.4 Phase 4 — `SpawnWithComponents` coalesced command

**Deliverables:** Code per plan §3 Phase 4. New contract `18_spawn_with_components_wire.spec.md`. Updated goldens (commit documents the delta). `cargo-fuzz` target per [§11.1](#111-fuzz-the-new-decoder).

**Gate:**

- Contract 07 event-stream assertions pass unchanged.
- Contract 18 passes including the byte-count-lower assertion and fragmentation scenario (t6).
- Wire-byte golden is smaller by at least the expected framing overhead (10K tiles × per-InsertComponent framing saved).
- Fuzz target runs clean for its configured duration.

### 6.5 Phase 5 — Immutable components

**Deliverables:** Code per plan §3 Phase 5. New contract `19_immutable_components.spec.md`. Compile-fail fixtures under `test/compile_fail/` per [§7.3](#73-trybuild-compile-fail-harness).

**Gate:**

- All prior contracts unchanged.
- Contract 19 passes including allocation-count-zero obligations.
- `trybuild` tests all pass, rejecting the forbidden `Property<T>` / `EntityProperty` / `Delegated` combinations with clear error messages.
- Bench [§8.2](#82-baseline-per-phase): `DiffHandler.mut_receiver_builders.len()` after inserting 10K immutable components equals 0.

---

## 7. New Harness Capabilities

To support Layers B and C, the harness needs three extensions.

### 7.1 `AllocationSnapshot` API

Feature-gated behind `test_utils`:

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

### 7.2 Wire trace capture

```rust
#[cfg(feature = "test_utils")]
impl Scenario {
    pub fn enable_trace_capture(&mut self);
    pub fn take_trace(&mut self) -> Trace;
}
```

Hook into the local transport already used by the harness. Every send/recv call records `(direction, packet_index, timestamp_tick, bytes)`. `Trace` derives `Serialize`/`Deserialize` via serde so golden files are JSON.

Hash stability: pre-refactor baseline trace has a blake3 hash; CI asserts the post-refactor trace of the same scenario hashes identically (Phases 2 and 3) or differs only in the expected wire-framing region (Phase 4).

### 7.3 `trybuild` compile-fail harness

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

## 8. Benchmarking Discipline

Naia has **no `benches/` directory**. The perf claims in the plan ("10K tiles with O(mutations) work") are today unverifiable. This is unacceptable for a networking library and is the single biggest methodological gap.

### 8.1 Add a criterion benches crate

Create `benches/` with a dedicated `naia-benches` crate using `criterion`:

```
benches/
  Cargo.toml           # separate from workspace default-members to avoid build-time cost
  src/
    common.rs          # scenario fixtures (10K entities, N users, etc.)
  benches/
    scope_update.rs    # scope-update work per tick
    update_dispatch.rs # mutation → update pipeline
    level_load.rs      # 10K-entity burst (wire bytes + reliable-message count)
    per_component.rs   # DiffHandler allocations per component insert
```

### 8.2 Baseline-per-phase

For each phase that claims a perf improvement, the PR commits a baseline criterion report *before the code change* (generated on `main`) AND the post-change report. Include both in the PR description so review can diff them.

```bash
cargo bench --bench scope_update -- --save-baseline pre-phase-2
# <land Phase 2 code>
cargo bench --bench scope_update -- --save-baseline post-phase-2
cargo bench --bench scope_update -- --baseline pre-phase-2
```

Concrete claims to verify per phase:

- **Phase 2:** idle-room scope-update tick time ≤ 5% of pre-refactor (and absolute < 100µs for 10K entities).
- **Phase 3:** idle-entity per-tick update-dispatch tick time ≤ 5% of pre-refactor.
- **Phase 4:** level-load wire bytes for 10K entities drop by ≥ expected framing overhead (specific number TBD during implementation).
- **Phase 5:** `DiffHandler.mut_receiver_builders.len()` after inserting 10K immutable components = 0.

### 8.3 Perf regression gate (deferred)

Criterion can be configured to fail CI on regression. Not required for the refactor, but worth adopting as a follow-up once a baseline exists. Flagged for `_AGENTS/` backlog.

---

## 9. Concurrency Correctness — loom for Phase 3

`MutChannel::send` runs on whatever thread mutates a `Property<T>` — in Bevy that's the system-scheduler-chosen thread. Today the cross-thread communication is via `Arc<RwLock<DiffMask>>` (one per user per component). Phase 3 adds a per-user `dirty_components: HashSet<(GlobalEntity, ComponentKind)>` that is written from the mutation thread and drained from the send thread.

This is a new synchronization site on a hot path. Current tests don't exercise it under interleaving.

### 9.1 Add a loom test crate

`loom` is the standard Rust tool for model-checking concurrent code. It exhaustively permutes thread interleavings on unit-test-scale models.

```
test/loom/
  Cargo.toml       # depends on loom, gated behind "loom" feature
  src/
    dirty_set.rs   # model-check the per-user dirty set writer/drainer
    mut_channel.rs # model-check the existing MutChannel path (retroactive confidence)
```

Run as part of a dedicated CI job (loom runs slow — minutes, not seconds):

```yaml
loom:
  runs-on: ubuntu-latest
  steps:
    - run: cd test/loom && cargo test --features loom --release
```

### 9.2 Required cases

- Writer on thread A mutates property P1 and P2; drainer on thread B drains; final state contains both.
- Writer-drainer interleaving does not lose a mutation (lost-update invariant).
- Concurrent writers from multiple threads (Bevy systems) to different entities — no torn state.

---

## 10. Observability — tracing + metrics

### 10.1 Migrate hot paths to `tracing`

Naia uses `log` today with zero uses of `tracing`. For the refactored code, `tracing` spans let us measure tick-level cost breakdown in prod without rebuilds:

```rust
#[tracing::instrument(skip_all, fields(queue_len = self.scope_change_queue.len()))]
fn drain_scope_changes(&mut self) { ... }

#[tracing::instrument(skip_all, fields(dirty_entities = dirty.len()))]
fn drain_dirty_updates(&mut self) -> ... { ... }
```

Scope:

- Top-level server tick (`WorldServer::send_all_updates`).
- `drain_scope_changes` (new, Phase 2).
- `drain_dirty_updates` (new, Phase 3).
- `write_updates` / `write_commands` per-packet.
- `init_entity_send_host_commands` (Phase 4 boundary).

Keep `log::warn/info` for error cases; `tracing` is for structured perf data. Add `tracing-subscriber` as a dev-dep only; production users configure their own subscriber.

### 10.2 Expose new metrics via contract 05

Contract `05_observability_metrics.spec.md` has 11 obligations and 7 scenarios today. The refactor adds prod-useful metrics that should be contracted:

- Gauge: `scope_change_queue_depth`
- Gauge: `dirty_entities_per_user` (histogram across users)
- Gauge: `diff_handler_receiver_count_global`
- Gauge: `diff_handler_receiver_count_per_user`
- Counter: `spawn_with_components_total`

These go into the existing `ServerMetrics` / `ClientMetrics` surface. Add obligations to contract 05 as part of each phase's deliverable.

---

## 11. Wire-format Safety (Phase 4)

Phase 4 adds `EntityCommand::SpawnWithComponents` — a new variant in a reliable-channel message type. This expands the wire-decode attack surface.

### 11.1 Fuzz the new decoder

Add a `cargo-fuzz` target for `WorldReader::read_command`:

```
fuzz/
  Cargo.toml
  fuzz_targets/
    world_reader.rs
```

Run for N minutes in CI on a cadence (weekly, not per-PR). The fuzz target feeds random bytes into `read_command` and asserts no panic. Specifically covers:

- Truncated `SpawnWithComponents` payloads.
- Component-count field larger than remaining buffer.
- Unknown `ComponentKind` values.
- Mixed valid/invalid inside the same command.

### 11.2 Fragmentation scenario for SpawnWithComponents

Contract 03 (Messaging) covers fragmentation for long messages. A 10K-tile-ish `SpawnWithComponents` could theoretically be small or large depending on component sizes. Covered by contract 18 obligation t6 (see [§2.3](#23-layer-c--new-contracts-for-new-behavior) Phase 4).

### 11.3 Protocol version bump documentation

Naia's `ProtocolId` is a hash of the protocol definition. Adding the new variant naturally changes the hash, which naturally rejects old clients at handshake. Document this explicitly in:

- `SCALE_10K_ENTITIES_PLAN.md` §6 Open Questions — note resolved.
- Contract 18 obligation t5.
- Release notes on the version that ships Phase 4.

---

## 12. API Documentation + Migration

### 12.1 Migration guide for downstream

Add `docs/migration-v0.26.md` (or whatever version ships this) covering:

- Struct-ified `ReplicationConfig` — migration path from enum. `const fn` constructors preserve the call-site shape.
- New `.persist_on_scope_exit()` — what it does, when to use it, interaction with `Delegated`.
- New `#[component(immutable)]` — Bevy-side enablement, Naia-side requirements, forbidden field types.
- `SpawnWithComponents` — zero user-facing API change; mentioned for release-note transparency.
- `UserScope::include/exclude` — unchanged, but idle-tick cost is now O(1) (documentation update, not API).

### 12.2 Canonical example: 10K-entity demo

Add `demos/10k_entities_demo/` that spawns 10K `#[component(immutable)]` entities with `ReplicationConfig::public().persist_on_scope_exit()` across multiple users with scope changes. Doubles as a manual perf-verification scenario and a reference for downstream users.

### 12.3 `#[deny(missing_docs)]` on new modules

Each new file (`replication_config.rs` after the struct refactor, the new `scope_exit.rs` if we split it, the Phase 3 dirty-set module) gets `#![deny(missing_docs)]` at the top. Forces rustdoc on every public item. Existing code continues under `#[warn(missing_docs)]` at the crate root — new code raises the bar without forcing a retroactive doc sprint.

---

## 13. Feature-flag Rollout

For Phases 2 and 3 (behavior-transparent internal changes), shipping behind a cargo feature flag lets downstream A/B test and revert without a repo-level revert.

### 13.1 Flag: `v2_push_pipeline`

```toml
# server/Cargo.toml
[features]
v2_push_pipeline = []  # default off during stabilization window
```

Phase 2 gates its code paths behind `#[cfg(feature = "v2_push_pipeline")]`. Phase 3 same. Phases 1, 4, 5 are user-visible API changes and do NOT gate (no meaningful "fall back to the old shape" story).

### 13.2 Stabilization criteria

Flag becomes default-on and the old paths get deleted when:

- All Phase 0 tests pass on both flag states.
- Golden traces match on both flag states.
- One release cycle elapsed with the flag off-by-default.
- No critical bug reports filed against the flag-on path.

This is an additional ~2 PRs (one to flip default, one to delete the old path), but the optionality is cheap and the confidence boost is real.

### 13.3 Anti-pattern to avoid

Flags proliferate. We allow exactly one (`v2_push_pipeline`). Phases 1, 4, 5 ship as hard cuts. Phase 5 does NOT get a "soft immutable" flag — the type-system mechanism is the commitment.

---

## 14. Coverage Maintenance Policy

Once Phase 0 lands, enforce:

1. **No merge without green `verify`.** CI gate runs `cargo run -p naia_spec_tool -- verify` including `adequacy --strict` and `traces check`.
2. **Every new public API surface in `naia-server` / `naia-client` / `naia-shared` gets a contract with ≥1 `t1` obligation before merge.** The existing Policy B applies to new code too, not just initial specs.
3. **Any PR that regenerates a golden trace must include a paragraph in the commit message explaining why the wire shape changed and link to the contract obligation authorizing it.**
4. **`@Deferred` scenarios are a bug**, not a commitment. If a scenario can't be written, the contract obligation should be removed or restated.

---

## 15. Estimated Effort

Rough T-shirt sizes. Phase 0 dominates; everything else benefits from it compounding.

| Phase | Effort | Rationale |
|---|---|---|
| Phase 0 | **L** (2–3 weeks FTE) | ~40 new scenarios, many reused step phrases; 10/11/14 are greenfield. |
| Phase 0.5 | S (1 week) | Trace capture is mechanical; allocation accessors are thin. |
| Phase 1 | M (1–2 weeks) | Struct refactor has wide call-site surface but mechanical; ScopeExit::Persist is new but contained. |
| Phase 2 | S (1 week) | Localized to scope pipeline. |
| Phase 3 | M (1–2 weeks) | Per-client dirty-set threading needs care around the existing Arc<RwLock> topology; loom model takes time. |
| Phase 4 | S (1 week) | Single new EntityCommand variant + wire plumbing + fuzz target. |
| Phase 5 | M (1–2 weeks) | Derive macro branches + trybuild harness + allocation-zero proofs. |

Phase 0 is non-negotiable. Everything else is gated behind it.

---

## 16. Open Questions

1. **Should `traces record` live in CI or be a dev-local command?** Recording is human-gated (we need to approve that the new trace is correct). `traces check` is CI-enforced. Propose: `record` is manual, `check` is CI, goldens live in the repo with a CODEOWNERS gate on the `golden_traces/` directory.
2. **Do we need property-based fuzzing for Phase 2/3?** Current tests are scenario-driven. For "idle-tick zero work" and "mutation → update" invariants, a small property-based generator (using `proptest` against the scenario API) would catch patterns scenarios might miss. Propose: add to Phase 3 as a "stretch" deliverable.
3. **Protocol version bump mechanics for Phase 4.** Naia has `ProtocolId` via hash of the protocol definition. Adding a new `EntityMessageType` variant changes the hash naturally, so handshake rejects old clients — correct behavior. Documented in contract 18 so it's not a surprise.

---

## 17. Deliberately Out of Scope

To prevent scope creep during implementation:

- **Property-based testing (proptest)** beyond the §16 open question. Scenario coverage + loom + fuzz is sufficient for Phases 1–5. Add proptest as a follow-up if a bug pattern emerges that scenarios miss.
- **async-mutex migration.** Naia's `RwLock` is `std::sync`. No benefit to switching for this refactor.
- **No-alloc / no-std paths.** Server already depends on std.
- **Typestate-encoded authority state machine.** Considered for `EntityAuthStatus`. The pay-off doesn't justify the API churn; contract 11 scenario coverage is the right tool.
- **Replacing `log` with `tracing` wholesale.** Only the new hot paths migrate. The rest stays `log`.
- **Removing `todo!()` elsewhere in the codebase.** 11 total exist; we address only the 9 in `host_world_manager.rs` that the refactor touches. The other 2 are unrelated and get their own cleanup ticket.

---

## 18. End-State Vision

If every item in the plan + this doc is honored, the Naia repo at the end of Phase 5:

**Coverage.** Contracts 06, 07, 08, 09, 10, 11 have full obligation coverage with running BDD scenarios. Five new contracts (15–19) specify the new behavior. Zero `@Deferred` in any feature file. `adequacy --strict` is CI-enforced.

**Observability.** Every hot-path tick phase has a tracing span. Metrics for dirty-set depth, scope-change-queue depth, diff-handler receiver counts are surfaced. Contract 05 covers them.

**Performance.** Criterion baselines exist for every claim in the plan. 10K-entity idle-room tick is proportional to mutations, not entities. Per-component allocation for immutable tiles is zero.

**Safety.** Loom-verified push-based dirty set. Fuzz-tested wire decoder for `SpawnWithComponents`. Nine `todo!()` gone — nine `unreachable!()` in their place, each with an explicit invariant.

**Supply chain.** `cargo-audit` and `cargo-deny` gated. Vulnerability banner gone.

**Docs.** Every public item touched has rustdoc with examples. Migration guide ships with the release. Demo crate exercises 10K entities + immutable + persist.

**Architecture.** `ReplicationConfig` is a struct with orthogonal axes (`publicity`, `scope_exit`). Scope-change propagation is push-driven. Update dispatch is push-driven. Immutable components are type-system-enforced and allocation-free. The per-client metadata layer scales with *what changes*, not with *what exists*.

That's the bar. Hold it.
