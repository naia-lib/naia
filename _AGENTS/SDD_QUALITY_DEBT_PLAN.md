# SDD Quality Debt — Audit + Implementation Plan

**Started:** 2026-05-07 · **Owner:** twin-Claude (autonomous execution)
**Status:** Audit complete. Implementation in progress.
**Gate per phase:** workspace builds clean, NPA run shows ≥172/172 pass with zero failures, plan doc updated, commit + push to `dev` (see `RELEASE_PROCESS.md`).

> **Branching policy (re-established 2026-05-07):** all in-flight work lives on `dev`.
> `main` is touched only at release time. See `BRANCH_REWIND_2026-05-07.md` for context.

---

## Why this document exists

The SDD migration plan (`SDD_MIGRATION_PLAN.md`) closed 2026-05-06 with all checkboxes marked. A subsequent audit (2026-05-07, originally `SDD_AUDIT_2026-05-07.md`, now subsumed into this doc) found that the mechanical deliverables shipped but the spirit did not. The plan declared "complete" while leaving:

- 152 `@Deferred` stub scenarios with no assertion logic (130 contract IDs covered by stub-only)
- Median step-binding LOC of 14 — original plan target was ≤8 (unmet by a wide margin)
- Zero `Scenario Outline:` usage despite the plan calling for "aggressive use"
- A 1206-line `given/state.rs` monolith
- A Python script (`_AGENTS/scripts/coverage_diff.py`) doing a job that belongs in the project's Rust CLI
- Stale committed run artifacts (171/169 in repo while live runs were 172/172)
- "Open questions" in the plan never answered

This document is the audit findings + a step-by-step remediation plan that closes that debt before any new test-coverage work begins.

---

## Connor's directives (2026-05-07)

These are hard constraints for the rest of this work. They override the original plan where they conflict.

1. **Step-binding ceiling: ≤25 LOC max.** Anything bigger gets a helper extracted. Enforce immediately, no grandfathering.
2. **Category B first, before any Category C work.** Stubs that already have integration-test coverage get upgraded to real BDD scenarios before we add coverage for genuinely missing behaviors.
3. **Use `Scenario Outline:` where appropriate.** Required, not optional.
4. **Split the `given/state.rs` monolith.** No single file should be 1200+ lines.
5. **Delete `_AGENTS/scripts/coverage_diff.py`.** No Python in this repo. Coverage tooling lives in the existing Rust CLI (`naia_npa coverage`, since contract-ID logic is Naia-specific, not generic to namako).
6. **Refresh committed run artifacts after any feature/binding change.** Treat `test/specs/resolved_plan.json` and `test/specs/run_report.json` as part of the change set.
7. **No fake test bodies in `@Deferred @PolicyOnly` scenarios.** A genuine policy-only Scenario gets a header and tag; it does not get a `Then the system intentionally fails` placeholder. Reserve `@Deferred @PolicyOnly` for items that truly cannot be asserted (Category A — meta-policy about test authorship); everything else gets upgraded or removed.
8. **`iai` workspace member stays disabled** for now.
9. **Category C work is out of scope for this plan.** Deferred to a follow-up plan once Categories A/B are clean.

---

## Section 1: Current state (audit baseline as of 2026-05-07)

### 1.1 What passes

```
NPA run:                   172/172  (live, fresh refresh committed today)
Workspace build:           CLEAN     (RUSTFLAGS=-D warnings)
Integration tests:         96/96     (carve-out, all passing, zero ignored)
Step-binding manifest:     258 bindings, no ambiguity errors
Feature files:             8         (00_foundations → 07_resources)
```

### 1.2 What's broken or wrong

| Area | Current | Target |
|---|---|---|
| Active scenarios in NPA plan | 172 | 172 + Category B uplift (~50 more) |
| Deferred stubs (junk `Then the system intentionally fails`) | 152 | ≤20 (Category A only) |
| Contract IDs with real BDD coverage | 125 | ≥175 (75% real, not stub-only) |
| Median binding LOC | 14 | ≤25 max, ≤14 median (current is fine) |
| Bindings > 25 LOC | 23 | 0 |
| `Scenario Outline:` blocks | 0 | ≥3 |
| `given/state.rs` size | 1206 lines | ≤500 lines per file (split into ≥4) |
| Python in repo | `_AGENTS/scripts/coverage_diff.py` | gone |
| Coverage tool | Python script (deceptive output) | `naia_npa coverage` Rust subcommand |
| Committed `run_report.json` | refreshed today | kept fresh per-commit |

### 1.3 Plan success criteria scoreboard

The original plan's nine success criteria, evaluated against current state with the directive overrides:

| # | Criterion | State |
|---|---|---|
| 1 | `legacy_tests/` does not exist | ✅ |
| 2 | 8 grouped feature files | ✅ |
| 3 | `steps/` organized by purpose | ⚠️ structure right, `given/state.rs` too big |
| 4 | All 198 contract IDs covered | ⚠️ technically yes, 130 stub-only |
| 5 | `cargo test --workspace` green | ✅ |
| 6 | No ambiguous bindings | ✅ |
| 7 | Median binding LOC ≤8 | ❌ now overridden: target is **≤25 max LOC** (per directive 1) |
| 8 | `crucible run --assert` passes | NOT VERIFIED IN AUDIT — verify in Q0 |
| 9 | Phase tracker fully checked | ⚠️ open questions never answered |

---

## Section 2: Inventory of work

### 2.1 The 152 deferred stubs broken down by category

**Category A — Genuinely policy-only (~20 stubs, KEEP marked, REMOVE fake step body):**

| Contract ID | Domain | Why policy-only |
|---|---|---|
| common-03 | framework | "Framework invariant violations MUST panic" — meta about Naia's panic policy |
| common-04 | framework | "Warnings are debug-only and non-normative" — about logging behavior |
| common-07 | testing | "Tests MUST NOT assert on logs" — meta about test authorship |
| common-08 | testing | "Test obligation template" — meta about contract format |
| common-09 | testing | "Observable signals subsection" — meta about doc format |
| common-10 | framework | "Fixed invariants are locked" — meta about constants |
| common-11 | framework | "Configurable defaults" — meta about config |
| common-11a | framework | "New constants start as invariants" — meta about evolution |
| common-12 | framework | "Internal measurements vs exposed metrics" — meta |
| common-12a | framework | "Test tolerance constants" — meta |
| common-13 | framework | "Metrics are non-normative for gameplay" — meta |
| observability-08 | metrics | "Time source monotonic consistency" — testable but requires wall-clock primitives |
| observability-10 | metrics | "Metrics are testable without feature flags" — meta about test infrastructure |
| world-integration-01/02/03 | api | API surface meta-rules |

These stay `@Deferred @PolicyOnly` but lose the placeholder `Then the system intentionally fails` step body. The Scenario header + `@Deferred @PolicyOnly` tag is sufficient for parity tracking.

**Category B — Integration-tested today, upgrade to real BDD (~51 stubs):**

The mapping below was generated from `test/harness/contract_tests/integration_only/*.rs` by grepping for contract-ID brackets. Each entry names the source-of-truth integration test the BDD scenario can use as a behavioral oracle.

#### B.1 — `02_messaging.feature` (22 stubs, all backed by `03_messaging.rs`)

| Contract ID | Integration test |
|---|---|
| messaging-01 | `messaging_01_user_errors_return_result` |
| messaging-02 | `messaging_02_remote_input_no_panic` |
| messaging-03 | (covered by `misusing_channel_types_yields_defined_failure`) |
| messaging-07 | `ordered_reliable_channel_keeps_order_under_latency_and_reordering` |
| messaging-08 | `ordered_reliable_channel_ignores_duplicated_packets` |
| messaging-09 | `unordered_reliable_channel_delivers_all_messages_but_in_arbitrary_order` |
| messaging-10 | `unordered_unreliable_channel_shows_best_effort_semantics` |
| messaging-11 | `sequenced_unreliable_channel_discards_late_outdated_updates` |
| messaging-12 | (covered by sequenced_unreliable + ordered_reliable combos) |
| messaging-13 | (TickBuffered groups by tick — needs spec match) |
| messaging-14 | (TickBuffered discards too-old) |
| messaging-15 | (TickBuffered discards too-far-ahead — see `protocol_type_order_mismatch`) |
| messaging-16 | `messaging_16_reliable_fragmentation_allowed` |
| messaging-17 | `messaging_15_unreliable_fragmentation_limit` |
| messaging-18 | `messaging_18_entity_property_message_buffering` |
| messaging-19 | `messaging_19_entity_property_ttl` |
| messaging-20 | `messaging_20_entity_property_buffer_caps` |
| messaging-23 | `client_to_server_request_yields_exactly_one_response` |
| messaging-24 | `concurrent_requests_from_multiple_clients_stay_isolated_per_client` |
| messaging-25 | `many_concurrent_requests_from_a_single_client_remain_distinct` |
| messaging-26 | `request_timeouts_are_surfaced_and_cleaned_up` |
| messaging-27 | `requests_fail_cleanly_on_disconnect_mid_flight` |

**Scenario Outline candidates here:** messaging-07/08/09/10/11/12 are "channel kind × delivery semantics" — perfect for an Outline. messaging-23/24/25 are "request/response cardinality" — another Outline.

#### B.2 — `01_lifecycle.feature` (21 stubs, all backed by `01_connection_lifecycle.rs`)

| Contract ID | Integration test |
|---|---|
| connection-04 | `server_reject_connection_produces_reject_event` |
| connection-06 | `connect_event_ordering_stable` |
| connection-12 | `auth_disabled_connects_without_auth_event` |
| connection-14 | `protocol_handshake_mismatch_fails` |
| connection-16/17 | (capacity reject — covered by `successful_auth_with_require_auth` + `server_reject_connection_produces_reject_event`) |
| connection-18/19 | `client_disconnects_due_to_heartbeat_timeout` |
| connection-20 | `reconnect_is_fresh_session` |
| connection-22 | (disconnect propagates — check `disconnect_idempotent_and_clean`) |
| connection-23 | `malformed_identity_token_rejected` |
| connection-24 | (token format — see token tests) |
| connection-25/26 | `expired_or_reused_token_obeys_semantics` |
| connection-27 | `valid_identity_token_roundtrips` |
| connection-28 | `reconnect_is_fresh_session` |
| connection-29 | `same_protocol_produces_same_id` |
| connection-30 | `protocol_id_wire_encoding_allows_connection` |
| connection-31 | `matched_protocol_id_allows_connection` |
| connection-32 | `protocol_id_determined_by_wire_relevant_aspects` |
| connection-33 | `protocol_id_verified_before_connect_event` |

**Scenario Outline candidates:** connection-29/30/31/32/33 are "protocol_id matching variants" — perfect for an Outline.

#### B.3 — `05_authority.feature` — entity-delegation (8 stubs, backed by `10_entity_delegation.rs`)

| Contract ID | Integration test |
|---|---|
| entity-delegation-01 | `enable_delegation_makes_entity_available_for_all_in_scope_clients` |
| entity-delegation-02 | `cannot_delegate_client_owned_unpublished_err_not_published` |
| entity-delegation-03 | (default available status — covered by enable_delegation test) |
| entity-delegation-04 | `disable_delegation_clears_authority_semantics` (precondition: no holder) |
| entity-delegation-05 | `disable_delegation_clears_authority_semantics` |
| entity-delegation-07 | (denied requests don't auto-promote — needs explicit assertion) |
| entity-delegation-08 | `migration_assigns_initial_authority_to_owner_if_owner_in_scope` |
| entity-delegation-10 | `delegating_client_owned_published_migrates_identity_without_despawn_spawn` |
| entity-delegation-12 | `client_request_authority_on_non_delegated_returns_err_not_delegated` |

**Total Category B: ~51 stubs across 3 feature files.** All have a verified integration test as oracle.

**Category C — Genuinely missing coverage (~80 stubs — OUT OF SCOPE for this plan, deferred):**

These are deferred to a follow-up plan. Categories include `entity-authority-*` state machine, `server-events-*` and `client-events-*` event API, `observability-*` metrics, and a handful of `entity-ownership/publication-*` items.

### 2.2 Bindings exceeding 25 LOC (23 bindings)

| LOC | File | Binding name |
|---|---|---|
| 92 | given/state.rs | `given_multiple_scope_operations_same_tick` |
| 73 | when/network_events.rs | `when_same_application_logic_runs` |
| 62 | when/network_events.rs | `when_client_authenticates_and_connects` |
| 60 | when/network_events.rs | `when_client_reconnects` |
| 49 | when/client_actions.rs | `when_client_attempts_write_to_server_owned_entity` |
| 47 | given/state.rs | `given_server_spawns_delegated_entity_in_scope_for_both_clients` |
| 45 | given/state.rs | `given_two_entities_a_b_in_scope` |
| 40 | given/state.rs | `given_server_spawns_non_delegated_entity_in_scope_for_client_a` |
| 39 | when/network_events.rs | `when_client_attempts_connection_rejected` |
| 39 | given/state.rs | `given_server_spawns_delegated_entity_in_scope_for_client_a` |
| 35 | then/state_assertions.rs | `then_entity_spawns_with_correct_values` |
| 35 | given/state.rs | `given_client_spawns_client_owned_entity_with_replicated_component` |
| 32 | when/client_actions.rs | `when_client_sends_on_server_to_client_channel` |
| 30 | when/client_actions.rs | `when_alice_requests_authority` |
| 29 | then/state_assertions.rs | `then_client_receives_message_a_exactly_once` |
| 29 | then/state_assertions.rs | `then_client_observes_server_value` |
| 29 | then/state_assertions.rs | `then_client_observes_component_update` |
| 28 | then/state_assertions.rs | `then_server_observes_component_update` |
| 28 | given/state.rs | `given_server_has_observed_spawn_event_for_client_a` |
| 28 | given/state.rs | `given_entity_in_scope_for_client_b` |
| 26 | then/state_assertions.rs | `then_client_entity_position_still_zero` |
| 26 | given/state.rs | `given_server_owned_entity_enters_scope_for_client_a` |
| 26 | given/state.rs | `given_connected_client_with_replicated_entities` |

Common patterns to extract into `world_helpers.rs`:
- "spawn delegated entity, configure scope for N clients" (5+ occurrences)
- "client connects with auth + room join + scope inclusion" (3+ occurrences)
- "observe component update with poll" (4+ occurrences)
- "scope queue interaction over N ticks" (1 large occurrence — 92 LOC)

### 2.3 `given/state.rs` proposed split

The file's 1206 lines already use horizontal-rule comment blocks to group sections. Map directly:

| New file | Lines from current state.rs | Approx LOC |
|---|---|---|
| `given/state_entity.rs` | "Entity / component preconditions" + `Score`/`PlayerSelection` resource setup | ~250 |
| `given/state_scope.rs` | "Scope / room preconditions" + ScopeExit::Persist setup | ~280 |
| `given/state_authority.rs` | "Authority / delegation preconditions" + delegated-entity spawns | ~320 |
| `given/state_publication.rs` | "Client-owned / Public/Private replication" preconditions | ~120 |
| `given/state_network.rs` | "Network conditions" — RTT, jitter, latency givens | ~80 |
| `given/state_misc.rs` | Disconnect / queued-tick / out-of-order command preconditions | ~150 |

**Note:** "given/state_entity.rs" should remain ≤500 LOC after extraction; if any single file ends up over that, split further.

---

## Section 3: Implementation phases

Each phase ends with: build clean, NPA 172+ /172+ pass, plan doc updated, **artifacts refreshed**, commit + push.

### Phase Q0 — Verify perf gate (BLOCKER, ~5 min)

The original plan's success criterion 8 was never verified.

- [ ] **Q0.1** Run `cargo run -p crucible -- run --assert` (or whatever the current bench command is — see `_AGENTS/BENCHMARKS.md`). Confirm 0 regressions.
- [ ] **Q0.2** If anything regressed, stop and triage before continuing — perf debt is its own track.

### Phase Q1 — Tooling cleanup (LOW risk, ~1 day)

Goal: get the Python out, replace with Rust subcommand, refresh artifacts as a habit.

- [ ] **Q1.1** Add `naia_npa coverage` subcommand at `test/npa/src/coverage.rs`. Behavior:
  - Walks `test/specs/features/*.feature` for `[contract-id]` brackets and `@Deferred` tags
  - Reports per-feature: `total / active / deferred / deferred-only-contract-IDs`
  - Optional `--json` output for downstream tooling
  - Optional `--fail-on-deferred-non-policy` flag: exits non-zero if any deferred scenario lacks `@PolicyOnly`
  - Replace the regex-based contract-area list with a richer enum (or accept any `[a-z][a-z0-9-]*-[0-9a-z]+` pattern)
- [ ] **Q1.2** Wire the subcommand into `test/npa/src/main.rs` `Commands` enum.
- [ ] **Q1.3** Update `_AGENTS/DEBUGGING_PLAYBOOK.md` reference (and any other doc) from the Python script to `cargo run -p naia_npa -- coverage`.
- [ ] **Q1.4** **Delete** `_AGENTS/scripts/coverage_diff.py` and `_AGENTS/scripts/` if it's now empty. Delete `_AGENTS/SDD_COVERAGE_DIFF.md` too — its content is regenerable on demand.
- [ ] **Q1.5** Add a one-line refresh recipe to this doc and to `_AGENTS/SYSTEM.md`:
  ```bash
  cd /home/connor/Work/specops/namako && cargo run -q -p namako_cli -- lint \
    --adapter-cmd /home/connor/Work/specops/naia/target/debug/naia_npa \
    -s /home/connor/Work/specops/naia/test/specs \
    -o /home/connor/Work/specops/naia/test/specs/resolved_plan.json
  cd /home/connor/Work/specops/naia && cargo run -q -p naia_npa -- run \
    -p test/specs/resolved_plan.json \
    -o test/specs/run_report.json
  ```
- [ ] **Q1.6** Gate: `naia_npa coverage` runs cleanly; coverage diff Python file is gone; lint + run produce identical artifacts to what's currently committed; build clean. Commit + push.

### Phase Q2 — Helper extraction (foundation for Q5, ~2 days)

Goal: every binding ≤25 LOC. Extract helpers preemptively before Q5 starts adding new bindings.

- [ ] **Q2.1** Add 4 helpers to `test/tests/src/steps/world_helpers.rs`:
  - `spawn_delegated_entity_in_scope(ctx, &[client_keys])` — replaces the 39–47 LOC delegated-entity setups
  - `connect_client_full(ctx, label, &Auth, &room_key)` — connect + auth + room join + scope-include in one call (used by the 60–73 LOC reconnect/connect bindings)
  - `observe_component_update(ctx, client_key, entity_key, predicate, n_ticks)` — replaces the 28–29 LOC observation polls in `then/state_assertions.rs`
  - `enqueue_scope_ops_same_tick(ctx, ops: &[ScopeOp])` — splits the 92-LOC `given_multiple_scope_operations_same_tick` into a slim binding + a helper
- [ ] **Q2.2** Refactor each of the 23 over-25-LOC bindings to consume the new helpers. Track per binding:
  - [ ] given/state.rs:92 → `given_multiple_scope_operations_same_tick`
  - [ ] when/network_events.rs:73 → `when_same_application_logic_runs`
  - [ ] when/network_events.rs:62 → `when_client_authenticates_and_connects`
  - [ ] when/network_events.rs:60 → `when_client_reconnects`
  - [ ] when/client_actions.rs:49 → `when_client_attempts_write_to_server_owned_entity`
  - [ ] given/state.rs:47 → `given_server_spawns_delegated_entity_in_scope_for_both_clients`
  - [ ] given/state.rs:45 → `given_two_entities_a_b_in_scope`
  - [ ] given/state.rs:40 → `given_server_spawns_non_delegated_entity_in_scope_for_client_a`
  - [ ] when/network_events.rs:39 → `when_client_attempts_connection_rejected`
  - [ ] given/state.rs:39 → `given_server_spawns_delegated_entity_in_scope_for_client_a`
  - [ ] then/state_assertions.rs:35 → `then_entity_spawns_with_correct_values`
  - [ ] given/state.rs:35 → `given_client_spawns_client_owned_entity_with_replicated_component`
  - [ ] when/client_actions.rs:32 → `when_client_sends_on_server_to_client_channel`
  - [ ] when/client_actions.rs:30 → `when_alice_requests_authority`
  - [ ] then/state_assertions.rs:29 → `then_client_receives_message_a_exactly_once`
  - [ ] then/state_assertions.rs:29 → `then_client_observes_server_value`
  - [ ] then/state_assertions.rs:29 → `then_client_observes_component_update`
  - [ ] then/state_assertions.rs:28 → `then_server_observes_component_update`
  - [ ] given/state.rs:28 → `given_server_has_observed_spawn_event_for_client_a`
  - [ ] given/state.rs:28 → `given_entity_in_scope_for_client_b`
  - [ ] then/state_assertions.rs:26 → `then_client_entity_position_still_zero`
  - [ ] given/state.rs:26 → `given_server_owned_entity_enters_scope_for_client_a`
  - [ ] given/state.rs:26 → `given_connected_client_with_replicated_entities`
- [ ] **Q2.3** Verify with `naia_npa coverage` (or a quick LOC count): zero bindings >25 LOC.
- [ ] **Q2.4** Gate: NPA 172/172 pass, build clean, refresh artifacts. Commit + push.

### Phase Q3 — Split `given/state.rs` (LOW risk, ~½ day)

Goal: no file >500 LOC in `steps/`.

- [ ] **Q3.1** Create the 6 new files under `given/`:
  - `state_entity.rs`, `state_scope.rs`, `state_authority.rs`, `state_publication.rs`, `state_network.rs`, `state_misc.rs`
- [ ] **Q3.2** Move bindings from `state.rs` to the appropriate file (mechanical move, follow the existing `// ──` section dividers as the boundary).
- [ ] **Q3.3** Update `given/mod.rs` to re-export each split file. Delete `state.rs` once empty.
- [ ] **Q3.4** Verify cucumber-rs ambiguity check still passes (run the manifest; it'll panic-fail compile if a binding's text is ambiguous after the move).
- [ ] **Q3.5** Gate: NPA 172/172 pass, build clean, every file ≤500 LOC, manifest still 258 bindings (or whatever it ended at after Q2). Refresh artifacts. Commit + push.

### Phase Q4 — Clean up Category A stubs (LOW risk, ~½ day)

Goal: stop pretending `Then the system intentionally fails` is a test.

- [ ] **Q4.1** Identify the ~20 Category A scenarios (list in §2.1). For each:
  - Keep the `Scenario:` header with its `[contract-id]` tag.
  - Keep the `@Deferred @PolicyOnly` tag.
  - Remove the `Then the system intentionally fails` step body. Add a one-line `# Policy-only: <one-sentence justification>` comment in its place.
- [ ] **Q4.2** Verify namako lint accepts a Scenario with no steps. (If it doesn't: replace with a single `Given a server is running` step that's already idempotent — but the comment-only form is preferable.)
- [ ] **Q4.3** Remove the `the system intentionally fails` step binding from `then/state_assertions.rs` if no scenarios still use it after Q5.
- [ ] **Q4.4** Gate: NPA 172/172 pass, lint passes, build clean. Refresh artifacts. Commit + push.

### Phase Q5 — Category B stubs → real BDD scenarios (MEDIUM risk, ~5–7 days)

Goal: 51 stubs become real `@Scenario` blocks with real step bodies. **This is the bulk of the value.**

For every Category B item: read the integration-test oracle, write the BDD scenario body in gherkin matching the same observable assertions, add or extend step bindings as needed (≤25 LOC each), remove `@Deferred` tag.

Per-feature execution order (small-to-large, lower-risk-first):

#### Q5.A — `02_messaging.feature` Scenario Outline pass (~1 day)

The channel-kind matrix is the highest-leverage Outline candidate.

- [ ] **Q5.A.1** Write a `Scenario Outline:` for messaging-07/08/09/10/11/12 (channel reliability × ordering matrix). Backing integration tests:
  ```
  ordered_reliable_channel_keeps_order_under_latency_and_reordering   → ordered + reliable
  unordered_reliable_channel_delivers_all_messages_but_in_arbitrary_order → unordered + reliable
  unordered_unreliable_channel_shows_best_effort_semantics             → unordered + unreliable
  sequenced_unreliable_channel_discards_late_outdated_updates          → sequenced + unreliable
  ```
  Use an `Examples:` table parameterizing `<channel_kind>`, `<allows_dupes>`, `<preserves_order>`, `<is_reliable>`.
- [ ] **Q5.A.2** Write a second Outline for messaging-23/24/25 (request/response cardinality: 1c→1s, Nc→1s, 1c→Ns).
- [ ] **Q5.A.3** Convert remaining standalone messaging stubs (01, 02, 03, 13, 14, 15, 16, 17, 18, 19, 20, 26, 27) to non-Outline real scenarios. Remove `@Deferred` tags as scenarios go green.
- [ ] **Q5.A.4** Gate: 22 messaging stubs gone, ~10 new real scenarios + 2 Outlines, NPA passes incrementally per scenario. Refresh artifacts. Commit + push.

#### Q5.B — `01_lifecycle.feature` Scenario Outline pass (~1 day)

- [ ] **Q5.B.1** Write a `Scenario Outline:` for connection-29/30/31/32/33 (protocol-id determinism matrix). Examples table: `<scenario_name>` / `<lhs_protocol>` / `<rhs_protocol>` / `<expect_match>`.
- [ ] **Q5.B.2** Convert connection-04/06/12/14/16/17/18/19/20/22/23/24/25/26/27/28 to real scenarios. Each maps directly to one integration test from §B.2.
- [ ] **Q5.B.3** Gate: 21 connection stubs gone, NPA passes. Refresh artifacts. Commit + push.

#### Q5.C — `05_authority.feature` delegation pass (~1 day)

- [ ] **Q5.C.1** Convert entity-delegation-01/02/03/04/05/07/08/10/12 to real scenarios using `10_entity_delegation.rs` as oracle.
- [ ] **Q5.C.2** Consider an Outline for delegation state-transition matrix (Available → Requested → Granted → Releasing → Available) if 3+ scenarios share shape.
- [ ] **Q5.C.3** Gate: 8 delegation stubs gone, NPA passes. Refresh artifacts. Commit + push.

#### Q5.D — Mop-up: any remaining trivially-upgradeable stubs (~½ day)

- [ ] **Q5.D.1** Audit remaining `@Deferred` scenarios. Anything that has integration coverage we missed in §2.1's Category B list gets upgraded.
- [ ] **Q5.D.2** Confirm via `naia_npa coverage`: zero non-Category-A `@Deferred` scenarios remain.
- [ ] **Q5.D.3** Gate: deferred count drops to ~20 (Category A only). Refresh artifacts. Commit + push.

### Phase Q6 — Plan close-out + answer the open questions (~1 hr)

- [ ] **Q6.1** Update `SDD_MIGRATION_PLAN.md`'s "Open questions" section with the actual decisions:
  - Tag convention: chose `Scenario: [contract-id] — description` inline.
  - Background scope: per-feature `Background:` (already implemented in C.2).
  - Outline for messaging matrix: now used (Q5.A.1, Q5.A.2).
  - Failing test policy: fixed during Track 2.
- [ ] **Q6.2** Add a "Quality debt closed" pointer from `SDD_MIGRATION_PLAN.md` to this document.
- [ ] **Q6.3** Update `_AGENTS/CODEBASE_AUDIT.md` T2.1 entry to reflect the substantive coverage state (active vs deferred ratio).
- [ ] **Q6.4** Final verification:
  - `RUSTFLAGS="-D warnings" cargo build --workspace --all-targets` — clean
  - `cargo test --workspace --all-targets` — 0 failures, 0 ignored (outside documented carve-out)
  - `cargo run -p naia_npa -- coverage` — ≥220/240 active scenarios, ≤20 deferred (Category A only)
  - `cargo run -p naia_npa -- coverage --fail-on-deferred-non-policy` — exits 0
  - Run report committed and matches live run
- [ ] **Q6.5** Mark this plan COMPLETE at top, push to dev. (Release-time merge to main per `RELEASE_PROCESS.md` is a separate step.)

---

## Section 4: Acceptance criteria for this plan

This plan is "done" when **all** of the following hold:

1. ✅ Workspace build clean with `RUSTFLAGS=-D warnings`.
2. ✅ `cargo test --workspace` — 0 failures, 0 ignored outside documented `integration_only/` carve-out.
3. ✅ NPA active-scenario count: **≥220** (was 172). Pass rate: 100%.
4. ✅ Deferred-stub count: **≤20** (Category A only, with no fake step bodies).
5. ✅ Every step binding ≤25 LOC. Verifiable with a one-shot grep + line-count.
6. ✅ ≥3 `Scenario Outline:` blocks present (messaging channel matrix, messaging request matrix, connection protocol-id matrix at minimum).
7. ✅ No file in `test/tests/src/steps/` exceeds 500 LOC.
8. ✅ `_AGENTS/scripts/coverage_diff.py` does not exist. `_AGENTS/scripts/` does not exist (or contains only legitimate utilities, currently none).
9. ✅ `naia_npa coverage` subcommand works and `--fail-on-deferred-non-policy` exits 0.
10. ✅ `test/specs/resolved_plan.json` and `test/specs/run_report.json` match a live run reproducibly (commit refresh after every BDD-affecting change).
11. ✅ `SDD_MIGRATION_PLAN.md` open questions resolved.
12. ✅ Memory record updated to point at Naia's new active/deferred ratio (the agent's project-state ledger).

---

## Section 5: Out of scope (Category C — separate follow-up plan)

Genuinely-missing test coverage to be planned in a successor doc once Q1–Q6 land:

- `entity-authority-02 through 15` — state machine transitions
- `server-events-00 through 13` (excluding currently-tagged 07/09)
- `client-events-*` for component CRUD
- `observability-01 through 10` — RTT, jitter, bandwidth metric assertions
- `entity-ownership-13/14` — events and concurrent operations
- `entity-publication-06 through 11` — publication state transitions

Estimated scope: ~80 new BDD scenarios + 30–50 new step bindings + harness primitives (wall-clock, capacity config, multi-protocol). Defer.

---

## Change log

- **2026-05-07** — Document created, originally as `SDD_AUDIT_2026-05-07.md`. Renamed to `SDD_QUALITY_DEBT_PLAN.md` and expanded into a full implementation plan after Connor's directives 1–9. Q-phases drafted for autonomous execution.
