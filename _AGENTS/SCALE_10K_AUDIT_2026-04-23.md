# Naia 10K Scaling Audit — 2026-04-23

**Branch:** `release-0.25.0-e`
**Auditor:** Claude (automated review post-Phase-5 completion)
**Companion docs:** `SCALE_10K_ENTITIES_PLAN.md`, `SCALE_10K_ENTITIES_LANDING.md`

> **Note:** Some findings below may be inaccurate — this is a machine-generated audit and Connor has flagged that certain observations are incorrect. Treat as a reference starting point, not ground truth. Verify before acting on any specific claim.

---

## Overall Score: ~65 / 100

Architecture fully delivered. Landing obligations substantially incomplete.

---

## Architecture (Wins 1–5) — 100% ✅

All five wins verified as implemented:

### Win 1 — `ReplicationConfig` struct + `ScopeExit::Persist` ✅
- `server/src/world/replication_config.rs`: struct with `Publicity` + `ScopeExit` axes
- `world_server.rs:2783–2790`: scope-exit branches on `ScopeExit::Persist` vs `Despawn`
- `local_world_manager.rs:695–706`: `pause_entity` / `resume_entity` implemented
- `local_world_manager.rs:980–982`: paused entities filtered from update emission

### Win 2 — Push-based scope-change queue ✅
- `update_entity_scopes` contains no full-entity iteration
- Drains per-room removal queues + `scope_change_queue: VecDeque<ScopeChange>`
- Idle rooms: O(1) per tick

### Win 3 — Push-based dirty candidate set ✅
- `get_updatable_world()` replaced by `UserDiffHandler::dirty_receiver_candidates()`
- Iterates only registered (in-scope, mutable) receivers
- Idle entities: O(0) per tick

### Win 4 — `SpawnWithComponents` ✅
- `EntityCommand::SpawnWithComponents(GlobalEntity, Vec<ComponentKind>)` exists
- Client expansion in `remote_entity_channel.rs:259–296`
- Wire: 1 message per entity instead of 1+K

### Win 5 — Immutable component zero-allocation ✅
- `ReplicateBuilder::is_immutable()`, `ComponentKinds::kind_is_immutable()` implemented
- Early-return in `insert_component_diff_handler` for immutable kinds
- Skip in `init_entity_send_host_commands` for `register_component`

---

## Projected Performance at 10K Tiles

| Scenario | Pre-refactor | Post-refactor |
|---|---|---|
| Idle room, scope stable | O(10K × users) per tick | O(1) per tick |
| Camera pan (10% entities leave scope) | 1K despawn + respawn pairs | 0 despawn (Persist) |
| Idle update scan | 50K receiver checks | 0 — only dirty receivers |
| Level load burst (10K × 5-component tiles) | ~60K reliable messages | ~10K coalesced |
| Per-tile allocations (immutable) | 9 Arc<RwLock> + HashMap entries | 0 |

---

## Landing Obligations — ~30% ⚠️

### Missing entirely (audit's view — may be partially wrong)

| Item | LANDING ref | Notes |
|---|---|---|
| Criterion benchmarks | §8.1 | No `benches/` dir found. Perf claims unverified. |
| Loom concurrency tests | §9.1 | No `test/loom/` dir found. Win 3 dirty set untested under interleavings. |
| Feature flag `v2_push_pipeline` | §13.1 | Not found in Cargo.toml. Phases 2/3 shipped as hard cuts. |
| `tracing` instrumentation | §10.1 | No `tracing::instrument` spans on hot paths found. |
| Migration guide | §12.1 | No `docs/` directory. |
| 10K entity demo | §12.2 | No `demos/10k_entities_demo/`. |
| CI gates | §4.1–4.5 | `naia_spec_tool verify`, `traces check`, `cargo-audit` not in CI. |
| Supply chain audit | §4.3 | `cargo-audit` / `cargo-deny` not gated. |

### Present but incomplete

| Item | Status |
|---|---|
| 9 `todo!()` in `host_world_manager.rs:271–295` | Still present; landing §3.1 required replacing with `unreachable!()` before Phase 1 |
| Golden traces | Only 3 (contracts 06/07/10); Phases 2/3/4 regression fence traces missing |
| Compile-fail fixtures | `test/compile_fail/` dir exists; Phase 5 fixtures (Property<T>/EntityProperty/Delegated+immutable) unconfirmed |
| Contracts 15–19 | Spec files exist; scenario completeness unconfirmed |
| Rustdoc on touched types | Minimal or absent on `ReplicationConfig`, `HostEntityChannel`, `UserDiffHandler` |

---

## Potential Risks (audit's view — some may be wrong)

1. **Unverified perf numbers.** No benchmarks means O(1) claim is theoretical. HashMap overhead, lock contention on Win 3 dirty set could surprise.
2. **Win 3 concurrency.** New cross-thread write path on per-user dirty set; loom not run.
3. **Cyberlith opt-in required.** Win 1 only helps if tile entities use `.persist_on_scope_exit()`. Nothing forces this from Naia's side.
4. **`todo!()` panics.** 9 panicking arms in `host_world_manager.rs:271–295` could be triggered.
5. **SpawnWithComponents MTU fragmentation.** Contract 18 t6 scenario unconfirmed.

---

## Items Completed in This Session

- Phase 3 (Win 3): dirty-receiver candidate set — committed `358e7f68`
- Phase 4 (Win 4): SpawnWithComponents — committed `fa4ae594`
- Phase 5 (Win 5): Immutable component zero-allocation — committed `bf198c34`
- BDD gate: 158/158 scenarios passing on `release-0.25.0-e`

## Items Completed in Follow-up Session (2026-04-24)

- **Code hygiene:**
  - Replaced 9 `todo!()` in `host_world_manager.rs:271–295` with `unreachable!()` + invariant docs
  - Removed commented-out `has_diff_mask` from `user_diff_handler.rs:76–78`
  - Removed commented-out `info!` from `global_diff_handler.rs:52–55`
- **Compile-fail harness (Phase 5):**
  - `test/compile_fail/fixtures/immutable_property.rs` + `.stderr` — verifies Property<T> in immutable is rejected
  - `test/compile_fail/fixtures/immutable_entity_property.rs` + `.stderr` — verifies EntityProperty in immutable is rejected
  - Both fixtures verified: trybuild test passes 2/2
- BDD gate re-confirmed: 158/158 passing after hygiene changes

---

## Remaining Deferred (per Connor)

- Benchmarks (criterion benches crate)
- Loom concurrency tests for Win 3 dirty set
- Feature flag `v2_push_pipeline` / stabilization path
- `tracing` instrumentation on hot paths
- Migration guide in `docs/`
- 10K entity demo
- CI gates (spec-verify, traces check, cargo-audit)
- Supply chain audit (cargo-audit, cargo-deny)
- Golden traces for contracts 15–19
- Rustdoc on touched types
