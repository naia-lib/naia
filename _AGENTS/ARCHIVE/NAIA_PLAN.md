# Naia — Living Implementation Plan

**Owner:** Connor + twin-Claude  
**Branch:** `dev` (never commit to `main`; `main` is touched only at tag time)  
**Gate:** `namako gate --specs-dir test/specs --adapter-cmd "cargo run --manifest-path test/npa/Cargo.toml --"` — must pass after every phase  
**Created:** 2026-05-07 (consolidates all prior plan docs; see §Archives)

---

## Current state snapshot

*Last updated: 2026-05-15 (post Rule(08) component-event bug fixes)*

| Metric | Value |
|---|---|
| Active BDD scenarios | **341** (100% pass, full suite recertified 2026-05-15) |
| Bevy BDD scenarios | **21** (100% pass) |
| @PolicyOnly | **86** (all justified) |
| Plain @Deferred (junk) | **0** ✅ |
| Build warnings | **0** (`-D warnings`) |
| `cargo test -p naia-shared --features test_time` | **218 passed**, 0 failures |
| `cargo-deny` advisories | 21 ignores, all expiring **2027-06-01** |
| Production `todo!()` | **0** ✅ |
| Production `#[allow(dead_code)]` | **20** (all justified or test-only — see V3.16) |
| TODO/FIXME in production code | **~33** (classified in AUDIT_REPORT_V2.md) |

### What's done (do not re-audit)

- **Rule(08) component-event correctness + harness architecture fix** (2026-05-14/15) — 4 new live scenarios ([client-events-13/14], [server-events-14/15]); 3 bugs fixed: (1) `remote_world_manager.rs` Despawn handler now fires `RemoveComponent` events on the client side before clearing the entity mapping; (2) `client.rs::remove_component_worldless` guarded for server-created delegated entities; (3) harness removes processing falls back to registry-only lookup for despawn-path removes. Harness gained persistent entity-event history (4 HashSets in `Scenario`) so step-boundary event draining can no longer cause false timeouts. `clear_event_history` extended to cover entity history. 341/341 NPA green — uncommitted pending explicit greenlight.
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
- P6: vocab.rs Phase A.3 — EntityRef activated (regex fixed), 5 dead typed params deleted, 2 {word}→{entity} migrations — `dev`
- P8: 10 unsafe impl Send/Sync removed from bevy adapters (T3.1); P8.3 N/A (handler signatures differ) — `dev`
- P9: connection-33/34/35 reconnect edge cases added; 309 active scenarios — `dev`
- P11: kebab-case renames (×3), println→debug! (×3), feature audit; 0 build warnings — `dev`
- D-P10: Full docs overhaul — README, SECURITY.md, CHANGELOG.md, MIGRATION.md, CONCEPTS.md, all crate //! + /// API surface docs, Bevy adapter trait docs — `dev`
- **V2 production-readiness audit** (2026-05-10): CRITICAL transmute UB fixed; 8 NOTABLE safety/reliability findings resolved (CacheMap OOM cap, handshake delete-user gap, dead migration stub, user_opt API, Safety comments ×20, pending-auth timeout, host_engine panic→warn, UDP panic messages) — `dev` commits `5815dfed`..`f54ef583`
- **V3 MINOR audit sweep** (2026-05-10): session.rs unsafe removed + Pending bug fixed; doc batch (magic numbers, API docs, CHANGELOG); adversarial BDD ×2 + `message_manager` channel-not-found panic fixed; SequenceBuffer wraparound unit tests ×7; dead_code audit (6 methods removed) — `dev` commits `7a9c6f35`..`e3579d4a`

---

## Priority stack

Active phases P1 → P3 → P4 → P5 → P6 → P8 → P9 → P11 → A1–A5 — **ALL COMPLETE** (2026-05-09). Deferred phases listed in §Deferred.

### V2 Audit-derived active phases (AUDIT_REPORT_V2.md, 2026-05-10)

| Phase | Finding | Status |
|---|---|---|
| **V2.1** | [CRITICAL] Fix transmute lifetime UB in local transport (C.2) | ✅ |
| **V2.2** | [NOTABLE] Apply CacheMap LRU to address_to_timestamp_map (I.2/K.1) | ✅ |
| **V2.3** | [NOTABLE] Prune been_handshaked_users on disconnect (K.4) | ✅ |
| **V2.4** | [NOTABLE] Guard incomplete migration stub in host_world_manager (J.1) | ✅ |
| **V2.5** | [NOTABLE] Convert stale-key panics to Option returns (A.3) | ✅ |
| **V2.6** | [NOTABLE] Add // Safety: comments to all unsafe blocks (C.1/D.4) | ✅ |
| **V2.7** | [NOTABLE] Add pending-auth connection timeout (K.1) | ✅ |
| **V2.8** | [NOTABLE] Convert host_engine entity-not-found panic to warn+return (B.2) | ✅ |
| **V2.9** | [NOTABLE] Fix empty panic messages in UDP URL parsing (B.2) | ✅ |

---

### V3 — Post-V2 sweep: MINOR findings + residual debt (2026-05-10)

Prioritized by correctness → API quality → test coverage → housekeeping.

#### Correctness / safety

| Phase | Task | Status |
|---|---|---|
| **V3.1** | Investigate session.rs "catastrophic" TODO on Pin::new_unchecked — confirm soundness or fix | ✅ |
| **V3.2** | Resolve bandwidth_accumulator half-finished feature — complete it or delete it | ✅ |
| **V3.3** | simple_handshaker TODO bubble-up consistency check vs advanced_handshaker | ✅ |

#### API quality

| Phase | Task | Status |
|---|---|---|
| **V3.4** | B.4 — Give RecvError a payload so consumers can distinguish failure modes | ✅ |
| **V3.5** | D.1 — Add doc comments to pause/resume/enable/disable_entity_replication | ✅ |
| **V3.6** | A.2 — Document broadcast_message silent per-user error discard behaviour | ✅ |
| **V3.7** | A.5 — Add explanation of E: Copy+Eq+Hash+Send+Sync bound to Server<E> doc | ✅ |
| **V3.8** | C.3/C.4 — Add caller-constraint doc to Send/Sync impls; document e2e_debug extern "Rust" linking contract | ✅ |
| **V3.9** | I.1 — Name magic numbers: digest CacheMap cap (64), scope-check assertion (1024), FRAGMENTATION_LIMIT_BYTES rationale | ✅ |
| **V3.10** | I.3 — Add rationale comments to ConnectionConfig::default() timeout values | ✅ |
| **V3.11** | H.1 — Document scope_checks_all() allocation cost; add warning to guide callers toward scope_checks_pending() | ✅ |
| **V3.12** | CHANGELOG.md — Record V2 audit fixes (V2.1–V2.9) under [Unreleased] | ✅ |

#### Test coverage

| Phase | Task | Status |
|---|---|---|
| **V3.13** | E.3 — Adversarial BDD scenarios: truncated/corrupted Data packet discard path | ✅ |
| **V3.14** | E.5 — Property-based tests for priority accumulator and SequenceBuffer wrap-around | ✅ |

#### Housekeeping

| Phase | Task | Status |
|---|---|---|
| **V3.15** | F.2 — Add inline rationale comments to all 24 cargo-deny ignore entries in deny.toml | ✅ |
| **V3.16** | J.2 — Audit 22 #[allow(dead_code)] sites; remove any that are genuinely unreachable | ✅ |

---

### Audit-derived active phases — ALL COMPLETE (2026-05-09, commit 9f150630)

| Phase | Finding | Status |
|---|---|---|
| **A1** | Panic→graceful discard in reliable_message_receiver + fragment_receiver | ✅ |
| **A2** | `debug_assert!` overflow guard on MessageKind + ChannelKind ID gen | ✅ |
| **A3** | Documents the force-publish packet-ordering invariant (removes hacky TODO) | ✅ |
| **A4** | @Scenario(38) [entity-authority-17] — 328 active, 10/10 gate | ✅ |
| **A5** | entity_request/release_authority return NotInScope not NotDelegated for absent entity | ✅ |

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

## P6 — vocab.rs Phase A.3 — **COMPLETE** (2026-05-08)

**Delivered:**
- P6.1: EntityRef regex updated to `[A-Za-z][A-Za-z0-9_]*` (was lowercase-only) to accept uppercase entity labels "A"/"B". Two `{word}` bindings migrated to `{entity}`: `when/server_actions_scope.rs:342` and `then/state_assertions_replication.rs:166`.
- P6.2/P6.3: No applicable `{word}` bindings for channel names or authority roles exist. All authority-status steps use literal text; all channel names are in the step literal, not parameterized.
- P6.4: `ComponentName`, `ChannelName`, `AuthRole`, `RoomRef`, `MessageName` deleted — no migration targets exist and keeping dead types is clutter. `npa/src/manifest.rs` custom_parameters list updated to match.
- P6.5: All `#[allow(dead_code)]` removed from vocab.rs. Gate: 0 build warnings, `namako gate` passes, committed + pushed `dev`.

---

## P8 — Bevy adapter cleanup (T3.1, T3.3) — **COMPLETE** (2026-05-08)

**Delivered:**
- P8.1/P8.2: Removed all 10 `unsafe impl Send/Sync` from bevy adapters (`component_event_registry.rs` ×2, `bundle_event_registry.rs` ×2, `world_data.rs` ×1). All inner handler traits already had `: Send + Sync` bounds; removal confirmed by `cargo check --workspace --all-targets` (clean, 0 warnings).
- P8.3: **N/A** — handler method signatures differ fundamentally between server (`Vec<(UserKey, Entity)>`) and client (`Vec<Entity>`, `Vec<(Tick, Entity)>`). `UserKey`/`Tick` are crate-private types that cannot live in `naia_bevy_shared`. A `TParams` with associated types would save only the HashMap declaration while leaking crate-specific types through the shared boundary — net negative. Deferred to D-P8.3.
- P8.4: Gate: `cargo check --workspace` clean, committed + pushed `dev`.

---

## P9 — Test infrastructure additions (A4) — **COMPLETE** (2026-05-08)

**Delivered:**
- P9.4/P9.7: `[connection-33]` — reconnect with in-scope entity → entity re-enters scope cleanly. Uses `"a server-owned entity exists"` + `"the client and entity share a room"` + `"the client eventually sees the last entity"`.
- P9.5/P9.7: `[connection-34]` — reconnect while resource live → Score replicated to new session.
- P9.6/P9.7: `[connection-35]` — reconnect while authority held → authority Available on reconnect. Fixed `when_client_reconnects` to store key under `client_key_storage("ReconnectedClient")` for named lookup.
- All 3 under `Rule: Reconnection edge cases` (@Rule(13)) in `01_lifecycle.feature`. Gate: 309 active scenarios (306 + 3), 100% pass.

---

## P11 — Hygiene — **COMPLETE** (2026-05-08)

**Delivered:**
- P11.1: Renamed 3 snake_case package names to kebab-case: `naia_npa→naia-npa`, `naia_bevy_npa→naia-bevy-npa`, `naia_spec_tool→naia-spec-tool`. Updated `_AGENTS/RELEASE_PROCESS.md` and `_AGENTS/DEBUGGING_PLAYBOOK.md`. Binary `[[bin]]` names (file-level) unchanged (snake_case is conventional for binary file names).
- P11.2: `e2e_debug` is actively useful (auto-dump-on-timeout + state-snapshot APIs for BDD debugging). Verified `cargo check -p naia-tests --features e2e_debug` builds cleanly (85 gates, 0 warnings). CI matrix entry deferred — CI is paused.
- P11.3: `grep -rn "todo!()" shared/ server/ client/ adapters/` returns 0 real hits. ✅
- P11.4: Replaced 3 `println!` with `debug!` in `shared/src/transport/local/hub.rs:296,325,353`. 0 build warnings.
- P11.5: Feature audit complete. `zstd_support`, `transport_udp`, `advanced_handshake` have zero workspace callers but have real implementations — not retired (valid public API for external users). No FEATURES.md created (per session conventions). Features with workspace callers: `transport_local` (18), `test_time` (13), `bevy_support` (9), `transport_webrtc` (6), `wbindgen` (4), `mquad` (4), `interior_visibility` (3), `test_utils` (2), `bench_instrumentation` (2).
- P11.6: Skipped — CI completely paused.

---

## A1 — Receiver panic cleanup — **COMPLETE** (2026-05-09)

- `reliable_message_receiver.rs`: `match message_kinds.read(...)` with `warn!` + discard on `Err`
- `fragment_receiver.rs`: same; `FragmentId` gained `Debug` derive for `{:?}` format

---

## A2 — ID overflow checks — **COMPLETE** (2026-05-09)

- `message_kinds.rs` + `channel_kinds.rs`: `debug_assert!(current_net_id < NetId::MAX)` before increment; stale TODO removed

---

## A3 — Delegation enable force-publish — **COMPLETE** (2026-05-09)

Root cause was packet-ordering race (enable-delegation arrives before publish), not a protocol error. The force-publish is correct; the TODO was wrong. Replaced self-deprecating TODO with invariant explanation.

---

## A4 — Authority double-send BDD coverage — **COMPLETE** (2026-05-09)

@Scenario(38) [entity-authority-17]: two-client `give_authority` → client A Granted, client B Denied. 328 active scenarios, 10/10 gate.

---

## A5 — Client request_authority wrong error code — **COMPLETE** (2026-05-09)

`entity_request_authority` + `entity_release_authority` now return `NotInScope` (not `NotDelegated`) when entity absent from records or auth handler not yet initialised.

---

## Archives (outdated plans — do not re-audit)

The following documents are superseded by this plan. All have been moved to `_AGENTS/ARCHIVE/`.

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
| `API_CLEANUP.md` | ✅ COMPLETE 2026-05-08 | This document |
| `AUDIT_PLAN.md` + `AUDIT_REPORT_2026.md` | ✅ COMPLETE 2026-05-09 | This document |
| `BENCHMARKS.md` | ✅ COMPLETE 2026-04-24 | Memory record |
| `BRANCH_REWIND_2026-05-07.md` | ✅ Executed 2026-05-07 (one-time runbook) | N/A |
| `DOCS_PLAN.md` | ✅ COMPLETE 2026-05-08 (D-P10) | This document |
| `WORLDSERVER_DECOMP.md` | ✅ COMPLETE 2026-05-10 (D-P2) | This document |

---

## Acceptance criteria for "done" state

1. BDD gate passes at 100%, ≥ 325 active scenarios with real steps. ← **TEST_INFRA_PLAN.md** (old 350 target retired — see that doc)
2. Zero `todo!()` in production code. ← **P1.7**
3. `vocab.rs` has zero `#[allow(dead_code)]` attributes. ← **P6**
4. All step bindings ≤25 LOC, no file in `steps/` exceeds 500 LOC. ← **TEST_INFRA_PLAN.md T5**
5. `cargo test --workspace` green, 0 ignored (outside documented carve-out). ← ongoing

## Active sub-plans

- `_AGENTS/TEST_INFRA_PLAN.md` — test infrastructure overhaul (T0–T5); see that doc for phases

---

## Deferred (indefinitely)

The following phases are parked. Do not schedule without explicit instruction from Connor.

### D-A1 — CONCEPTS.md Bevy T-tag explanation (audit finding) — COMPLETE
The `T` type parameter on `Client<'w, T>` and `Plugin<T>` is completely absent from `docs/CONCEPTS.md`. A Bevy newcomer will not understand why the generic exists or how to create a marker type. Add a §Bevy client tag section explaining `T` as a zero-cost phantom marker for compile-time client identity.
Delivered: §10 "Bevy adapter — the client tag type T" added to `docs/CONCEPTS.md`. Commit `91985ca4`.

### D-A2 — CHANGELOG catch-up (audit finding) — COMPLETE
CHANGELOG.md covers through the Resources feature era. Missing: P1 (`give_authority` API), P9 (reconnect edge cases), P11 (kebab-case renames, println→debug!). Add entries under the appropriate version heading.
Delivered: Added [Unreleased] ### Added entries for `give_authority`/`take_authority` and reconnect handling; ### Changed entries for kebab-case renames and debug! migration. Commit `91985ca4`.

### D-A3 — `give_authority` Bevy doc error cases (audit finding) — COMPLETE
`adapters/bevy/server/src/commands.rs:60-68` — the `give_authority` doc states the Delegated precondition but does not document what happens if the entity is not found or not Delegated (silent no-op? panic? error?). Add error behavior to the doc.
Delivered: Expanded doc with all three silent-no-op cases (not found, not Delegated, not in scope) and success-path fan-out. Commit `91985ca4`.

### D-A4 — Entity migration between rooms BDD coverage — **COMPLETE** (2026-05-09)
@Rule(19) @Scenario(01) `[room-migration-01]` added to `04_visibility.feature`. New steps: `given_entity_in_client_a_room_only`, `when_server_migrates_entity_to_client_b_room`, `then_entity_out_of_scope_for_client_a`. Gate: 330 active, 10/10 pass.

### D-A5 — remove_resource under authority BDD coverage — **COMPLETE** (2026-05-09)
@Rule(11) @Scenario(01) `[resource-authority-03]` added to `07_resources.feature`. New steps: `when_server_removes_playerselection`, `then_alice_no_longer_has_playerselection`. Gate: 330 active, 10/10 pass.

### D-A6 — Broadcast allocation (audit finding) — COMPLETE
`world_server.rs:513` — `message_box.clone()` in the broadcast loop allocates one `Box<dyn Message>` per connected user per broadcast. At 1262 CCU this is the most avoidable per-tick allocation. Consider `Arc<MessageBox>` or a ref-counted wrapper to amortize. Blocked on profiling to confirm it's a real bottleneck at target CCU.
Delivered: `MessageContainer.inner` changed from `Box<dyn Message>` to `Arc<Box<dyn Message>>`. Both `broadcast_message_inner` and `room_broadcast_message` now wrap once in `Arc` before the loop; per-user clone is a refcount increment. `send_message_inner` now accepts `MessageContainer` directly. Also fixed the same pattern in `room_broadcast_message`. Commit `bbb197df`.

### D-A7 — Error propagation overhaul (audit finding) — COMPLETE
18 TODOs of the form "pass this on and handle above" scattered across `world_server.rs`, `client.rs`, `main_server.rs`, `base_time_manager.rs`, `connection.rs`, `advanced_handshaker.rs`, `simple_handshaker.rs`. All indicate IO errors silently dropped. Systematic fix: propagate to a `WorldEvents::IoError` variant or similar. Large scope.
Delivered: 12 TODOs found and resolved (audit count was approximate; some files cited had none). `WorldEvents::IoError` variant not appropriate — UDP send failures are transient and connection timeout already detects persistent dead links. Each TODO replaced with a one-line comment explaining why log-and-continue is correct at that site. No behaviour change. Commit `fd228a00`.

### D-A8 — Benchmark expansion (audit finding) — COMPLETE
Three benchmark scenarios missing: (1) single-client round-trip latency, (2) resource replication throughput, (3) authority grant/revoke cycle cost. Add to `benches/` suite.
Delivered: `update/round_trip`, `resources/throughput` (insert_latency + mutation_throughput), `authority/cycle` (grant_revoke_cycle). Added `BenchWorldBuilder::delegated()`, `BenchResource`, and 7 new `BenchWorld` helpers. Commit `f787ea94`.

### A-17 — naia-metrics + naia-bevy-metrics observability crates — **COMPLETE** (2026-05-11)
New crates expose naia's internal network health via the `metrics` facade (any exporter). Commit `bce12600` (API prereqs) + `7bec32b0` (DefaultServerTag) + `89fdd0b4` (naia-metrics/naia-bevy-metrics Phase 1) + `98f0848b` (client RTT p99 ring buffer) + `64578202` (replication counters + channel throughput).
- `naia-metrics`: 19 gauges + 6 counters. 14 connection gauges (RTT/jitter/loss/bw), 3 server aggregates, 5 replication counters (spawns/despawns/component inserts+removes, messages_sent_total{channel}).
- `naia-bevy-metrics`: `NaiaServerMetricsPlugin` + `NaiaClientMetricsPlugin<T>` emit each tick after `SendPackets`.
- `naia-shared/observability` feature gates all counter call sites; activated transitively by `naia-metrics`.
- Client `rtt_p99_ms` is now a real 32-sample ring buffer (was EWMA fallback).

### D-P0 — DTLS stack migration (deadline: 2027-06-01)
6 RUSTSEC `cargo-deny` ignores expire 2026-06-01. Migration requires replacing the DTLS transport with a `rustls`-based stack. Deferred due to scope; if the deadline passes without action, add new `ignore` entries with updated dates.

### D-P2 — WorldServer decomposition (T1.1) — ✅ COMPLETE (2026-05-10)
`server/src/server/world_server.rs` — 3,826 → 3,699 lines (−127). `RoomStore` (Phase 1, `eae71471`) + `UserStore` (Phase 2, `75c7ab9b`). `ConnectionStore` (Phase 3) rejected after rigorous analysis: `user_connections` touches 37 distinct methods across every domain — there is no "connection domain" to consolidate, only orchestration. Architecture and rationale in `_AGENTS/WORLDSERVER_DECOMP.md`.

### D-P7 — Replicate trait decomposition (T1.2)
`shared/src/world/component/replicate.rs` defines a 29-method monolith; `shared/derive/src/replicate.rs` is 1499 lines. Proposed sub-trait split: `ReplicateCore`, `ReplicateWrite`, `ReplicateRead`, `ReplicateMirror`, `ReplicateAuthority`, `ReplicateEntityRelations`. Deferred: user-facing surface would be unchanged, benefit is internal ergonomics only.

### D-P9.A3 — proptest for message ordering
`OrderedReliable` / `SequencedReliable` property-based tests using `proptest`. 3 tasks (add dep, write 2 proptests). Deferred in favour of BDD coverage.

### D-P10 — Docs and discoverability — **COMPLETE** (2026-05-08)
Full documentation overhaul delivered across two sessions: README rewrite, `SECURITY.md`, `CHANGELOG.md`, `docs/MIGRATION.md`, `docs/CONCEPTS.md`, crate `//!` module docs for all four lib.rs files, full `///` API surface docs for `Server<E>`, `Client<E>`, all builder/accessor types (`EntityMut`, `EntityRef`, `RoomMut`, `RoomRef`, `UserMut`, `UserRef`, `UserScopeRef`, `UserScopeMut`), Bevy adapter `CommandsExt`/`ServerCommandsExt`/`ClientCommandsExt` traits, and both Bevy `Plugin` structs. `TESTING_GUIDE.md` and `RESOURCES.md` remain unwritten but are low-value given the existing `docs/CONCEPTS.md`.

### D-P12 — Large architectural refactors — ❌ INVESTIGATED, CLOSED (2026-05-10)

**Item 1 — `client.rs` manager-as-field decomposition:** Not actionable. `server_connection` appears in 37 distinct methods — the same profile as the D-P2 Phase 3 (ConnectionStore) rejection. `client.rs` has only 17 fields; none are collection fields with dedicated CRUD methods. There is no `rooms`/`users` analog. The handshake cluster (`handshake_manager`, `auth_message`, `auth_headers`, `manual_disconnect`, `server_disconnect`, `waitlist_messages`) looks groupable but `disconnect_reset_connection` crosses it with `io` and `global_world_manager` simultaneously — same cross-cutting problem.

**Item 2 — `Host<E>` trait unification:** Not actionable. The 31 shared public method names are superficially similar but the implementations diverge at every type boundary. Server has `user_connections: HashMap<SocketAddr, Connection>` (N connections); client has `server_connection: Option<Connection>` (0 or 1). `send_message` takes `(user_key, channel, message)` on the server and `(channel, message)` on the client — this is load-bearing divergence, not accidental. A unifying trait would require an associated `ConnectionKey` type (unit on client, `UserKey` on server), making every method signature more complex than the current two independent structs.

**Item 3 — `Box<dyn>` enum dispatch:** Actual count is 224 (not 434; plan count predated refactors). Transport-layer boxes (~120) are correctly polymorphic — new transports are external additions. `Box<dyn Message>` (22) and `Box<dyn Replicate>` (25) are runtime-registered via `BigMap`; enum dispatch would require compile-time exhaustion. Broadcast path already Arc-optimized (D-A6). Bench gate is 31 wins / 0 losses with no allocation regression — no profiling evidence of a box problem. Not worth pursuing without a concrete hotspot signal.
