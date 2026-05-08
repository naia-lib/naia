# Naia — Living Implementation Plan

**Owner:** Connor + twin-Claude  
**Branch:** `dev` (never commit to `main`; `main` is touched only at tag time)  
**Gate:** `namako gate --specs-dir test/specs --adapter-cmd "cargo run --manifest-path test/npa/Cargo.toml --"` — must pass after every phase  
**Created:** 2026-05-07 (consolidates all prior plan docs; see §Archives)

---

## Current state snapshot

| Metric | Value |
|---|---|
| Active BDD scenarios | **306** (100% pass, `namako gate` green) |
| @PolicyOnly (messaging) | **4** (messaging-03, 11, 12, 19 — all justified) |
| Plain @Deferred (junk) | **0** ✅ |
| Step bindings | ~275, all ≤25 LOC |
| Step files max LOC | 477 (`network_events_transport.rs`) |
| Build warnings | **0** (`-D warnings`) |
| `cargo test --workspace` | green, 0 ignored outside documented carve-out |
| `cargo-deny` advisories | 6 ignores expiring **2026-06-01** (25 days!) |
| Production `todo!()` | **0** ✅ |

### What's done (do not re-audit)

- Replicated Resources (R1–R9 + Mode B + D13) — `release-0.25.0-e`
- Perf upgrade (Phases 0–10, 6,356× idle improvement, 29/0/0 bench) — `dev`
- Priority accumulator (Fiedler pacing, A+B+C) — `dev`
- SDD migration (215 contracts → namako, 8 feature files) — `dev`
- SDD quality debt (Q0–Q6, junk→0, Outlines×3, 300 active) — `dev`
- Test infra audit (C1, C2, H1–H5, M1–M5, L1–L4 all closed) — `dev`
- Codebase audit: T0.1 (todo!→unreachable!), T1.3 (64-kind limit removed), T2.2 (demo naming), T3.4, T4.1 first-pass
- P3: server-events-08 converted to live test; 5 duplicate @PolicyOnly justified; 2 new step bindings — `dev`
- P4: unpublish/republish bug fixed (scope preserved + diff handler deregistered); entity-publication-11 live; 5 observability @Deferred → @PolicyOnly — `dev`
- P5: messaging-13/14/18/20 converted to live BDD; TickBufferedChannel + EntityProperty test infra added; messaging-11/12/19 kept @PolicyOnly with justified comments — `dev`

---

## Priority stack

Active phases run in order P1 → P3 → P4 → P5 → P6 → P8 → P9 → P11. P1, P3, P4, and P5 complete. Deferred phases (D-P0, D-P2, D-P7, D-P9.A3, D-P10, D-P12) are listed in §Deferred and will not be scheduled without explicit instruction.

---

## P1 — Category C BDD, Phase 1: Entity-authority state machine — **COMPLETE** (2026-05-07)

All tasks delivered in commit `33016cc3` on `dev`.

**Delivered:**
- P1.5: `entity-authority-15` converted from `@PolicyOnly` to real test (duplicate give_authority idempotent)
- P1.6: Bevy integration tests A1 (`give_authority` → Granted) and A2 (`take_authority` → Denied) in `adapters/bevy/server/tests/authority_commands_bevy.rs`
- P1.7: `give_authority` todo!() implemented across all 4 layers (world_server, server, bevy server wrapper, commands)
- Bug fix: `insert_component_worldless` now short-circuits for already-delegated components (`component_already_host_registered`); `GlobalDiffHandler::has_component` added
- P1.8: namako gate green (301 active scenarios), `cargo check --workspace` clean, pushed to dev

**State snapshot after P1:**
| Metric | Value |
|---|---|
| Active BDD scenarios | **301** |
| Production `todo!()` | **0** ✅ |
| Build warnings | **0** |

---

## P3 — Category C BDD, Phase 2: Server/client events API — **COMPLETE** (2026-05-08)

**Delivered:**
- P3.1: Audit complete — found only 11 @PolicyOnly remain (not 23; prior sessions had already converted many). No new TrackedEvent variants needed.
- P3.2/P3.3: No-op — all convertible scenarios had existing infrastructure.
- P3.4: Added `"client {client} has the entity in its world"` and `"client {client} does not have the entity in its world"` step bindings in `state_assertions_entity.rs`.
- P3.5: Converted `server-events-08` (per-user isolation) to live test. Added justified @PolicyOnly comments to `server-events-11`, `server-events-12`, `world-integration-01/02/03` (all duplicates of existing live tests in Rule(01) and Rule(03)).
- P3.6: `namako gate` green (301 scenarios, all pass), committed + pushed `dev`.

---

## P4 — Category C BDD, Phase 3: Observability, ownership, publication — **COMPLETE** (2026-05-08)

**Delivered:**
- P4.1/P4.2: observability-01/-01a/-08/-10 all classified @PolicyOnly with justified comments (TestClock is discrete; wall-clock injection unsupported).
- P4.3: entity-ownership-13/14 already had justified @PolicyOnly comments (TrackedServerEvent lacks OwnerChange; single-threaded harness makes concurrent-op determinism untestable via BDD).
- P4.4: Bug fixed across three layers: (1) `unpublish_entity` replaced `cleanup_entity_replication` call with targeted non-owner despawn (preserving scope map + room membership); (2) `unpublish_entity` deregisters components from diff handler so republish can re-register; (3) `publish_entity` now enqueues `EntityEnteredRoom` scope changes for each room the entity is in, triggering scope re-evaluation for non-owner clients. `entity-publication-11` scenario converted from @PolicyOnly to live test.
- P4.5: entity-publication-06/07/10 were already live tests; entity-publication-08/09 have justified @PolicyOnly comments.
- P4.6: Gate green (306 active, 100% pass), committed + pushed `dev`.

---

## P5 — Category C BDD, Phase 4: Messaging infrastructure — **COMPLETE** (2026-05-08)

**Delivered:**
- P5.1/P5.2: `TickBufferedChannel` and `EntityCommandMessage` were already in the test protocol. No new protocol additions needed.
- P5.3: Per-channel reorder not implemented; messaging-11 kept @PolicyOnly (correct disposition).
- P5.4: messaging-13 (groups by tick) and messaging-14 (discards expired) converted using `send_tick_buffer_message` + `inject_tick_buffer_message`. messaging-18 (EntityProperty buffering) and messaging-20 (buffer cap FIFO eviction) converted using `set_entity_property` + `read_message` accumulation. messaging-11 (per-channel reorder) and messaging-19 (TTL, broken contract test) kept @PolicyOnly with justified comments.
- P5.5: Gate green (306 scenarios, 100% pass), 0 build warnings, committed + pushed `dev`.

---

## P6 — vocab.rs Phase A.3

**Context:** `test/tests/src/steps/vocab.rs` has 6 typed parameter wrappers with `#[allow(dead_code)]` — staged for migration in the A.3 pass that never landed: `EntityRef`, `ComponentName`, `ChannelName`, `AuthRole`, `RoomRef`, `MessageName`.

Migrating bindings to use these types improves cucumber error messages and enforces the vocabulary contract at compile time.

**Tasks:**
- [ ] **P6.1** Migrate entity-label `{word}` bindings to `EntityRef` (target: `then/state_assertions_entity.rs` and `given/state_entity.rs`).
- [ ] **P6.2** Migrate channel-name `{word}` bindings to `ChannelName` (target: `when/server_actions_entity.rs` messaging steps).
- [ ] **P6.3** Migrate authority-role `{word}` bindings to `AuthRole` (target: `then/state_assertions_delegation.rs`).
- [ ] **P6.4** Migrate `ComponentName`, `RoomRef`, `MessageName` at appropriate sites or document why direct `{word}` is better for those.
- [ ] **P6.5** Remove all `#[allow(dead_code)]` from vocab.rs as each type is activated. Gate: zero dead_code warnings, `namako gate` passes, commit + push `dev`.

---

## P8 — Bevy adapter cleanup (T3.1, T3.3)

### T3.1 — unsafe impl Send/Sync documentation and fix

Ten `unsafe impl Send/Sync` in the Bevy adapter lack SAFETY comments. Better fix: add `: Send + Sync` to the `ComponentEventHandler` trait, letting the compiler derive auto-Send/Sync.

- [ ] **P8.1** Add `: Send + Sync` to `ComponentEventHandler` in both server and client adapters. Remove the 10 `unsafe impl` blocks (or add SAFETY docs if the bound can't be added).
- [ ] **P8.2** Verify `cargo check` for all Bevy adapter crates.

### T3.3 — ComponentEventHandler registry dedup

Server and client `component_event_registry.rs` are near-mirror images (~100 duplicated lines each, plus parallel D13 resource-translation logic).

- [ ] **P8.3** Lift the common registry shape into `naia_bevy_shared` as a generic `ComponentEventRegistry<TParams>`. Each adapter declares the parameter type.
- [ ] **P8.4** Gate: `cargo check --workspace`, wasm32 builds, commit + push `dev`.

---

## P9 — Test infrastructure additions (A4)

### A4 — Reconnection stress tests

`connection-28` (reconnect) is happy-path only. Missing: reconnect with in-scope entities, reconnect while resource is live, reconnect while authority is held.

- [ ] **P9.4** Write `reconnect_with_in_scope_entities`: connect → spawn entity in scope → disconnect → reconnect → entity re-enters scope cleanly (no stale key panic).
- [ ] **P9.5** Write `reconnect_while_resource_live`: insert resource → disconnect → reconnect → resource value preserved on server, replicated to new session.
- [ ] **P9.6** Write `reconnect_while_authority_held`: grant authority → disconnect (authority reclaimed) → reconnect → authority is Available again.
- [ ] **P9.7** Convert these into BDD scenarios in `01_lifecycle.feature` under a new `Rule: Reconnection edge cases`.

---

## P11 — Hygiene

### Crate naming consistency (T2.3)
Mixed kebab-case vs snake_case across 30+ crates. Public crates must be kebab-case; internal ones are mixed.
- [ ] **P11.1** Decide: kebab-case for all (matches crates.io publishing convention). Rename ~10 internal-only crates. Update any `cargo build -p name` references.

### e2e_debug feature gate audit (T2.6)
88 `#[cfg(feature = "e2e_debug")]` gates throughout server/client/shared. Not built in default config — a regression in this feature path would go undetected.
- [ ] **P11.2** Determine if `e2e_debug` is still useful as a diagnostic tool. If yes: add CI matrix entry building `--features e2e_debug`. If no: retire the feature (delete all 88 gates).

### T2.5 — TODO/FIXME/HACK triage (production code only)
Only 2 remain after the T0.1 audit: one is a comment, one is the real `give_authority todo!()` (tracked in P1.7). After P1.7 lands, this is fully clean. No action needed unless new ones appear.
- [ ] **P11.3** After P1.7: verify `grep -rn "todo!()" shared/ server/ client/ adapters/` returns 0 real hits.

### T5.1 — println! in local hub
- [ ] **P11.4** Replace `println!` at `shared/src/transport/local/hub.rs:296,325,353` with `log::debug!`.

### T5.2 — Feature matrix audit
- [ ] **P11.5** Audit which of the 12–14 features per crate have zero downstream callers. Retire unused features. Document surviving ones in `_AGENTS/FEATURES.md`.

### T4.2 — CI audit
- [ ] **P11.6** Confirm CI matrix covers: linux + wasm32 (as pre-push hook), plus `e2e_debug` (if retained per P11.2), plus transport matrix (local + webrtc). Add missing entries.

---

## Archives (outdated plans — do not re-audit)

The following documents are superseded by this plan. Their content is preserved for history but all outstanding items have been migrated above.

| Document | Status | Superseded by |
|---|---|---|
| `SDD_MIGRATION_PLAN.md` | ✅ COMPLETE 2026-05-06 | This document |
| `SDD_QUALITY_DEBT_PLAN.md` | ✅ COMPLETE 2026-05-07 | This document |
| `BENCH_PERF_UPGRADE.md` | ✅ COMPLETE 2026-04-24 | Memory record |
| `RESOURCES_PLAN.md` | ✅ COMPLETE 2026-05-05 | Memory record |
| `RESOURCES_AUDIT.md` | ✅ COMPLETE 2026-05-05 | This document (T3.1, T3.3) |
| `TEST_INFRA_AUDIT_2026-05-07.md` | ✅ COMPLETE (C/H/M/L closed; A-items → P1/P3/P9) | This document |
| `CODEBASE_AUDIT.md` | ✅ Open items → P2, P7, P8, P11, P12 | This document |
| `CRUCIBLE_BENCH_PLAN_2026-04-27.md` | ✅ Implemented (crucible in slag, wired to naia bench) | Memory record |

---

## Acceptance criteria for "done" state

1. `namako gate` passes at 100% (active scenario count ≥ 350). ← **P1–P5**
2. Zero `todo!()` in production code. ← **P1.7**
3. `vocab.rs` has zero `#[allow(dead_code)]` attributes. ← **P6**
4. All step bindings ≤25 LOC, no file in `steps/` exceeds 500 LOC. ← ongoing
5. `cargo test --workspace` green, 0 ignored (outside documented carve-out). ← ongoing

---

## Deferred (indefinitely)

The following phases are parked. Do not schedule without explicit instruction from Connor.

### D-P0 — DTLS stack migration (deadline: 2026-06-01)
6 RUSTSEC `cargo-deny` ignores expire 2026-06-01. Migration requires replacing the DTLS transport with a `rustls`-based stack. Deferred due to scope; if the deadline passes without action, add new `ignore` entries with updated dates.

### D-P2 — WorldServer decomposition (T1.1)
`server/src/server/world_server.rs` — 3592 lines, 141 methods. Split plan exists (10 module files: connections, scope, rooms, entities, authority, resources, messages, priority, io, mod). Deferred: high-effort mechanical refactor with low immediate value.

### D-P7 — Replicate trait decomposition (T1.2)
`shared/src/world/component/replicate.rs` defines a 29-method monolith; `shared/derive/src/replicate.rs` is 1499 lines. Proposed sub-trait split: `ReplicateCore`, `ReplicateWrite`, `ReplicateRead`, `ReplicateMirror`, `ReplicateAuthority`, `ReplicateEntityRelations`. Deferred: user-facing surface would be unchanged, benefit is internal ergonomics only.

### D-P9.A3 — proptest for message ordering
`OrderedReliable` / `SequencedReliable` property-based tests using `proptest`. 3 tasks (add dep, write 2 proptests). Deferred in favour of BDD coverage.

### D-P10 — Docs and discoverability
README architecture overview, `_AGENTS/RESOURCES.md` user walkthrough, `test/TESTING_GUIDE.md`, Bevy adapter `//!` module docs. Deferred until API surface stabilises post-P1–P5.

### D-P12 — Large architectural refactors
Two items: (1) `client.rs` (2311 lines, 95 methods) mirrors WorldServer patterns — a `Host<E>` trait could unify them, but depends on D-P2 (WorldServer decomp). (2) 434 `Box<dyn ...>` instances — profile under `halo_btb_16v16` bench first, then evaluate per-kind enum dispatch via derive macro if they appear in top-10 hotspots. Deferred: large scope, blocked on other deferred work.
