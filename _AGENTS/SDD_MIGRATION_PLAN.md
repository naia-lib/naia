# SDD Migration Plan — legacy_tests → namako

**Status:** Phase A in progress · Started 2026-05-06
**Owner:** twin-Claude (autonomous execution)
**Gate:** every phase must end with all existing tests still green + commit pushed to `main`

---

## North star

Naia's testing strategy converges on **namako SDD as the primary contract test surface**. The 14 `test/harness/legacy_tests/contract_NN_*.rs` files (16K LOC, 198 contract IDs, 220/8/13 currently passing) get migrated into ~8 grouped `.feature` files with a deduplicated step-binding catalog, then deleted. A small carve-out (~50-70 tests) for genuinely-imperative scenarios (tick loops, trace-subsequence assertions, instrumentation counters) stays as Rust integration tests under `contract_tests/integration_only/` with a documented rationale.

**Success = legacy_tests directory is gone, namako covers all 198 contract IDs, every test still passes, step catalog is searchable + parameter-disciplined + helper-thin.**

## Constraints (Connor's directives)

- **Few large `.feature` files at high abstraction.** Files of 1000+ LOC are fine. Avoid one-file-per-concern proliferation.
- **Step binding catalog is the elegance work.** Deduplication, organization, readability, clarity, and elegance are all non-negotiable. The catalog must be greppable and ambiguity-free.
- **Coverage stays high.** Contract-ID parity is the gate before deletion. No silent loss.
- **SDD is the primary strategy.** The handful of imperative-only tests are documented exceptions, not a parallel test culture.

## Architecture

### Feature files (24 → 8 grouped)

```
test/specs/features/
  00_foundations.feature       (was: 00_common, smoke, _orphan_stubs)
  01_lifecycle.feature         (was: 01_connection, 02_transport, 04_time_ticks, 05_observability)
  02_messaging.feature         (was: 03_messaging — kept dedicated; biggest)
  03_replication.feature       (was: 07_entity_replication, 18_spawn_with_components, 19_immutable_components)
  04_visibility.feature        (was: 06_entity_scopes, 15_scope_exit, 16_scope_propagation, 17_update_candidate)
  05_authority.feature         (was: 08_ownership, 09_publication, 10_delegation, 11_authority)
  06_events_api.feature        (was: 12_server_events, 13_client_events, 14_world_integration, 20_priority_accumulator)
  07_resources.feature         (was: 21_replicated_resources — kept dedicated)
```

Each ~600-1500 LOC, 40-80 Scenarios, organized by `Rule:` blocks per sub-concern. **Aggressive use of `Background:` and `Scenario Outline:`** — both currently at zero usage in the codebase.

### Step bindings catalog (current 22 contract-aligned files → purpose-aligned modules)

```
test/tests/src/steps/
  vocab.rs                # parameter parsing: {client}, {entity}, {component}, {channel}, {role}
  world_helpers.rs        # tick_until, expect_event, with_server, with_client, etc.
  given/
    mod.rs
    setup.rs              # "a server is running", "client {client} connects", "a room exists"
    state.rs              # entity/component/scope/auth state preconditions
  when/
    mod.rs
    server_actions.rs     # "the server spawns ...", "server gives authority to {client}"
    client_actions.rs     # "client {client} sends {message}", auth requests, etc.
    network_events.rs     # "the connection drops", "{n} ticks elapse"
  then/
    mod.rs
    state_assertions.rs   # observable state predicates
    event_assertions.rs   # event-history predicates
    ordering.rs           # subsequence/order assertions
```

**Discipline rules:**
- Parameters use a typed vocabulary defined once in `vocab.rs`. `{client}` always means a registered client by name; `{entity}` always means a stored entity reference; etc.
- A binding lives where its phrase belongs by *purpose*, not by which feature originally needed it.
- No two bindings may match the same step text (cucumber-rs ambiguity check enforces this; it's a hard fail).
- Median binding LOC target: **6** (current median: 18). Helper layer absorbs the boilerplate.

### Carve-out: integration-only tests

Tests that legitimately resist gherkin (estimated 50-70):
- Tight tick loops with explicit `for _ in 0..N`
- Trace-subsequence ordering invariants
- Instrumentation-counter reads (`e2e_debug` only)
- Multi-stage state-machine tests with 8+ mutate→expect cycles

These move to `test/harness/contract_tests/integration_only/` with a per-file header comment explaining the carve-out and linking to the related namako Scenarios for context.

---

## Phase tracker

Each phase ends with: tests green, commit on `main`, this document updated, pushed to origin.

### Phase A — Catalog refactor (3-5 days · LOW risk)

Goal: existing 220 tests stay green; step bindings reorganized into the new vocab/given/when/then structure; 24 features collapse into the 8 grouped files; `vocab.rs` exists.

- [x] **A.0** Persist this plan document to `_AGENTS/SDD_MIGRATION_PLAN.md`. Commit + push.
- [ ] **A.1** Establish the new directory structure under `test/tests/src/steps/` (vocab.rs, world_helpers.rs, given/, when/, then/) with empty stubs. `cargo build` clean.
- [ ] **A.2** Define the `vocab.rs` parameter vocabulary: typed wrappers for `{client}`, `{entity}`, `{component}`, `{channel}`, `{role}`, `{room}`, plus a parser convention. Document each in module docs.
- [ ] **A.3** Move existing 259 step bindings from `steps/{contract_name}.rs` to the new purpose-aligned modules. Mechanical pass; preserve phrases verbatim. Watch for ambiguity errors (cucumber will fail compile if two bindings match same text).
- [ ] **A.4** Collapse the 24 `.feature` files into the 8 grouped files. Preserve every Scenario verbatim; just regroup by feature. Update file paths in any tooling references (manifest emit, NPA run, etc.).
- [ ] **A.5** Verify: `cargo test -p naia-tests` passes the same 174 namako scenarios as before. Verify `cargo run -p naia_npa -- manifest` still emits 259+ bindings. Verify `cargo test -p naia-test-harness` still 220/8/13.
- [ ] **A.6** Update this doc with completion notes; commit `phase A complete: catalog refactor`; push to main.

### Phase B — Helper layer (1-2 days · LOW risk)

Goal: 15-20 reusable helpers in `world_helpers.rs` that drop median binding LOC from 18 → ~6. New bindings written in this phase use the helpers; existing bindings refactored opportunistically.

- [ ] **B.1** Identify the 20 highest-recurrence imperative patterns across existing bindings (e.g. "tick once and assert", "wait for client to observe X", "with_server(|s| ...)"). Catalog them.
- [ ] **B.2** Implement `world_helpers.rs` with each helper carrying a doc-comment that includes a usage example.
- [ ] **B.3** Refactor 30-50 existing bindings to use helpers as proof-of-shape. Verify median LOC drops as predicted.
- [ ] **B.4** Update this doc; commit `phase B complete: helper layer`; push to main.

### Phase C — Migration plumbing (1 day · LOW risk)

Goal: tooling that lets us see migration progress per-contract-ID, generate Background templates, and verify coverage parity at the end.

- [ ] **C.1** Coverage-diff tool: parse contract IDs from `legacy_tests/`, parse contract IDs from `features/`, output per-ID status (legacy-only / both / namako-only). Living artifact of migration progress.
- [ ] **C.2** Per-feature `Background:` templates pre-filled (e.g. for `02_messaging.feature`, the standard "server + 2 clients connected" prelude).
- [ ] **C.3** Update this doc; commit `phase C complete: migration plumbing`; push to main.

### Phase D — Bulk migration (8-12 days · MEDIUM risk)

Goal: all 167 missing contract IDs covered as namako Scenarios. Estimated +285 new scenarios, +80-150 new step bindings (vs ~1000 if no dedup).

Per-feature checklist (each is an independently-verifiable subgoal):

- [ ] **D.1** `00_foundations.feature` — migrate from 00_common.rs (~30 tests). Includes the error-taxonomy + determinism scenarios.
- [ ] **D.2** `01_lifecycle.feature` — migrate from 01_connection_lifecycle, 02_transport, 04_time_ticks_commands, 05_observability_metrics (~130 tests). Use Scenario Outlines for the connection-state matrix.
- [ ] **D.3** `02_messaging.feature` — migrate from 03_messaging (~78 tests). **Heaviest file**; expect Scenario Outlines for the channel-type × behavior matrix.
- [ ] **D.4** `03_replication.feature` — migrate from 07_entity_replication, 18_spawn_with_components, 19_immutable_components (~38 tests).
- [ ] **D.5** `04_visibility.feature` — migrate from 06_entity_scopes, 15_scope_exit_policy, 16_scope_propagation_model, 17_update_candidate_set (~30+ tests already partially covered).
- [ ] **D.6** `05_authority.feature` — migrate from 08_entity_ownership, 09_entity_publication, 10_entity_delegation, 11_entity_authority (~120 tests). Carries the most subtle contracts.
- [ ] **D.7** `06_events_api.feature` — migrate from 12_server_events_api, 13_client_events_api, 14_world_integration, 20_priority_accumulator (~36 tests).
- [ ] **D.8** `07_resources.feature` — already migrated; verify parity with legacy (if a 21_replicated_resources legacy file exists; otherwise no-op).
- [ ] **D.9** Each migrated test gets the matching contract ID(s) preserved as Scenario tags or names (`@Scenario(messaging-04)` or `Scenario: [messaging-04] …`).
- [ ] **D.10** Update this doc as each Dx subgoal lands; commit per subgoal; push to main.

### Phase E — Integration-only carve-out (1 day · LOW risk)

Goal: the gherkin-resistant tests have a clean home that doesn't pollute the main test suite.

- [ ] **E.1** Survey: per-file list of tests that didn't migrate cleanly to gherkin. Each gets a one-line "why" tag.
- [ ] **E.2** Create `test/harness/contract_tests/integration_only/` with a `README.md` explaining the carve-out's purpose and the migration deletion criteria (i.e. when can a test in here move out).
- [ ] **E.3** Move the carve-out tests; update `Cargo.toml` `[[test]]` entries to point at the new path.
- [ ] **E.4** Verify all tests still pass.
- [ ] **E.5** Update this doc; commit `phase E complete: integration-only carve-out`; push to main.

### Phase F — Delete legacy_tests (½ day · LOW-IF-A-E-DONE risk)

Goal: zero coverage loss verified, then the directory is gone.

- [ ] **F.1** Run the C.1 coverage-diff tool. Target: 0 contract IDs in legacy-only column. Anything in legacy-only is a migration miss; resolve before deletion.
- [ ] **F.2** Delete `test/harness/legacy_tests/` directory entirely.
- [ ] **F.3** Remove the 14 `[[test]]` declarations for `contract_NN_*` from `test/harness/Cargo.toml`.
- [ ] **F.4** Update `_AGENTS/CODEBASE_AUDIT.md` (T2.1 entry) marking the legacy_tests cleanup as DONE 2026-MM-DD.
- [ ] **F.5** Final verification: `cargo test --workspace --all-targets` green; `RUSTFLAGS="-D warnings" cargo build --workspace --all-targets` clean; `crucible run --assert` passes.
- [ ] **F.6** Update this doc with mission close-out; commit `mission complete: SDD migration`; push to main.

---

## Success criteria (cumulative, all must hold at end)

1. `test/harness/legacy_tests/` directory does not exist.
2. `test/specs/features/` has exactly 8 grouped `.feature` files plus the existing tooling files (no orphan stubs).
3. `test/tests/src/steps/` is organized by purpose (vocab + world_helpers + given/when/then), not by contract.
4. **All 198 contract IDs** are covered by at least one namako Scenario.
5. `cargo test --workspace --all-targets` is green (excluding any documented failing tests in `integration_only/`).
6. `cargo run -p naia_npa -- manifest` shows no ambiguous step bindings.
7. Median step-binding LOC is ≤ 8 (down from 18 today).
8. `crucible run --assert` still passes (perf gate not regressed).
9. This document has its phase tracker fully checked off, with completion dates per phase.

---

## Open questions / decisions deferred

- **Tag convention for contract IDs.** Option A: `Scenario: [messaging-04] — ...`. Option B: `@Contract(messaging-04)` tag. Decide in Phase A; document choice here.
- **Background scope.** Per-feature `Background:` (one block per file) vs per-`Rule:` `Background:`. Per-Rule is more granular but requires gherkin parser support; verify in A.4.
- **Outline for messaging matrix.** Channel kinds × delivery semantics is ~25 tests today, all near-clones. Confirm `Scenario Outline:` is the right shape vs writing them as one Scenario each. Decide in D.3.
- **Failing test policy.** The current 8 failing legacy tests block Phase F's parity gate. Decide: fix during Phase D, or migrate as `@Skip`-tagged Scenarios (with the same note as today's `#[ignore = "..."]` attributes). Recommended: fix during D.

---

## Change log

- **2026-05-06** — Document created. Phase A.0 done. Twin-Claude beginning Phase A.1.
