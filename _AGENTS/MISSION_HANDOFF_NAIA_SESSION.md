# naia session — mission handoff (2026-06-29)

You are a **naia-scoped** Claude Code session. A parallel session is running
in `../cyberlith` on the cyberlith action plan. This brief is self-contained
because our memory namespaces differ (memory is keyed by cwd). Read it fully
before touching anything.

## Ground rules (non-negotiable)
- **naia is dev-trunk**: commit on `dev`, NEVER `main`. (cyberlith merges to main.)
- **No wire-format changes in naia, ever** — byte-padding, bit→byte, D.1 bumps
  are off the table. Any protocol addition goes through Connor as an approval gate.
- **No env vars. No tokio.** (`thiserror`/`anyhow` are fine in test/tooling only
  where already used; core stays async-std/smol style.)
- **Determinism moat is the gate**: `cargo test -p cyberlith_test_harness
  --features desync_detection --test simulation` must stay byte-exact (54/54 as of
  this handoff). Physics is 25 Hz locked.
- **Claim discipline**: trace + reconcile against measurement; tag VERIFIED vs
  INFERRED. Never assert runtime behavior from a fragment.
- Commit messages end with:
  `Claude-Session: https://claude.ai/code/session_01BdLVfYUTF6VcxLSWWtn1NS`
- **namako feature specs are naia's PRIMARY testing surface** — not raw `#[test]`.
  Two suites: CORE `test/specs/features/*.feature` (run via `test/npa`, step defs
  in the `naia-tests` crate at `test/tests/`, resident mode) and BEVY ADAPTER
  `test/bevy_specs/features/*.feature` (run via `test/bevy_npa`, standard bevy
  App mode). Durable regressions belong in a `.feature` + step def.

## Coordination protocol with the concurrent cyberlith session
- **Single-writer-per-repo.** You own **naia** exclusively. You ALSO own the
  one-time landing of the coupled fix below, including its cyberlith files
  (Connor's call). The cyberlith session will NOT touch the fix's files until
  you report the fix committed.
- After **Mission 0** lands, **report back**: "coupled fix committed (naia dev
  <sha> + cyberlith <sha>)". After **Mission 1** lands, report "reorg landed on
  naia dev <sha>" (so the cyberlith session knows naia's layout shifted — though
  cyberlith path-deps naia as a whole, so no cyberlith change should be needed).
- Run the missions **strictly in order: 0 → 1 → 2.** Mission 1 (reorg) relocates
  `test/` wholesale; doing Mission 2 first would get clobbered.

---

## MISSION 0 — land the coupled re-insert fix (do this FIRST)

A background fix is **complete but uncommitted** in both repos. Review, then land.

**The bug:** replicated-resource remove → re-insert on the reused carrier failed
to replicate the new value to clients (silent no-replicate; or under-supply panic
at `world_writer.rs:430` with a naive skip). Two layers, both VERIFIED by tracing
+ reproduction:

1. **cyberlith double-enable (silent no-replicate).** `enable_carrier_replication`
   re-pushed an already-registered carrier to the registration outbox, re-running
   the non-idempotent `SimHandle::enable_entity_replication`
   (`GlobalEntityMap::spawn` allocates a NEW GlobalEntity, orphaning every
   connection's mapping). Fix: skip the registration drain when the carrier
   already has `HostOwned` (a retained carrier from a prior remove); the re-add
   flows through the existing registration via `on_component_added`.
2. **naia send-side premature-retire (the under-supply panic, exposed once #1 is
   fixed).** `HostWorldManager::host_entity_fully_delivered` judged delivery via
   `RemoteEntityChannel::has_component_kind`, but `process_messages` never removes
   a kind on a delivered `RemoveComponent` (the map is keyed for ordering, not
   presence) — so after remove→re-add it reported a stale "delivered=true" and
   retired the carrier from `pending_outbound` one tick after re-insert, before
   the new value was acked. cyberlith's dirty-trim snapshot then dropped it →
   panic. (Resident/standard mode never trims → `07_resources.feature` &
   `replicated_resources_bevy` pass; this is sim-mode-only.) Fix: track a
   per-entity `delivered_component_kinds` set maintained from acked Insert/Remove
   deliveries (honors removes) and use it in `host_entity_fully_delivered`. Purely
   send-side retire accounting — **zero wire bytes, zero client-receive change**.

**Uncommitted files to review + land:**
- naia (`dev`): `shared/src/world/host/host_world_manager.rs` (Layer 2 fix +
  despawn cleanup of the set).
- cyberlith (currently on `main` — branch per cyberlith flow, then merge):
  `services/game/cell/src/resource_replication.rs` (Layer 1 guard) and
  `test/game/harness/tests/simulation/level_lighting_replication.rs` (the
  restored `replicated_resource_reinsert_replicates_new_value` regression +
  de-staled remove-test docstring).

**Gates the fix agent already ran green (re-run to confirm before committing):**
- moat: `cargo test -p cyberlith_test_harness --features desync_detection --test simulation` → 54/54 byte-exact
- showcase e2e: `--test e2e default_level_showcase` → 3/3
- naia resident: `naia-bevy-server --test replicated_resources_bevy` → 6/6
- full `naia-shared` / `naia-server` / `naia-bevy-server` suites pass
- warning-clean on touched files

Commit naia on `dev`; commit cyberlith via its normal branch→merge-to-main flow.

---

## MISSION 1 — project reorg (after Mission 0 lands)

naia top-level is disorganized: 4 scattered perf crates, a spec tool living under
`test/`, fuzz at root. Reorg into `test/ | bench/ | tools/`.

**Target structure:**
```
naia/
  test/
    specs/ bevy_specs/        (feature files)
    npa/ bevy_npa/            (SDD adapters)
    tests/                    (namako step bindings — naia-tests crate)
    harness/                  (scenario/local-transport harness + contract_tests)
    loom/  compile_fail/
    fuzz/                     (MOVED from ./fuzz — Connor wants it in test/)
  bench/                      (consolidate ALL 4 perf crates)
    criterion/                (was ./benches      = naia-benches, Criterion)
    iai/                      (was ./iai          = naia-iai, iai-callgrind)
    wins/                     (was ./test/bench   = naia-bench, domain win-checks, dep slag crucible)
    suite/                    (was ./test/suite_bench = naia-suite-bench, BDD suite timing)
  tools/
    spec_tool/                (was ./test/spec_tool = naia_spec_tool CLI; IS used — see test/specs/README.md)
```
Note: `test/harness` (scenario harness) and `test/tests` (step bindings) are
DISTINCT — keep both; only the names are confusing.

**Lockstep updates (the breakage surface):**
- root `Cargo.toml` workspace `members` paths.
- intra-naia path-deps referencing any moved crate (grep each crate name).
- `.github/workflows/{main,dependencies,deploy-book}.yml` — reference benches/iai/
  fuzz/test paths.
- **fuzz caveat:** `./fuzz` is a `cargo-fuzz` crate (`[package.metadata]
  cargo-fuzz = true`); cargo-fuzz's convention is `<root>/fuzz`. Under `test/fuzz`,
  every `cargo fuzz` invocation needs `--fuzz-dir test/fuzz` (wire that into CI +
  any docs). Connor wants it under test/ — handle the flag, don't move it back.

**Safe:** NO external repo (cyberlith/diax/slag) depends on any naia test/bench/
fuzz crate — moves are naia-internal. Verify with a grep across the workspace
siblings before and after.

**Gate:** workspace builds; moat 54/54; `cargo fuzz` smoke runs with the new
`--fuzz-dir`; CI yaml paths resolve. Use `git mv` to preserve history.

---

## MISSION 2 — sim-pipelined namako BDD coverage (after Mission 1 lands)

**The gap:** naia's sim-pipelined / sim-integration mode (the mode cyberlith runs
in prod) has ZERO namako BDD coverage — only ~16 raw `#[test]` files in
`adapters/bevy/server/tests/`: `sim_replicated_resources`, `sim_host_sync`,
`sim_integration_iris2`, `sim_enable_entity_replication`,
`sim_configure_entity_replication`(`_deferred`), `sim_mark_entity_as_static`,
`sim_converter`, `sim_event_receiver`, `apply_receive_output_with_sim_receiver`,
`sim_integration_full_{lifecycle,park,shutdown,panic,tick_buffer}`,
`sim_integration_schedule_label`. This violates the primary-surface rule.

**The mission (FULL PORT, Connor 2026-06-29):** build a **sim-mode NPA adapter**
(drives `spawn_server_handles` + a client + `drain_host_sync_into_pipeline` +
`build_snapshot` + `send_all_packets`, with given/when/then steps for
enable/configure/room/static + entity spawn/insert/mutate/remove/despawn +
resource insert/mutate/remove/**re-insert** + scope/FoW + late-join) and feature
spec(s) (e.g. `sim_replication.feature`), porting the behaviors in those ~16 raw
tests to BDD scenarios. The new adapter is the sim analog of `test/bevy_npa`
(likely a sim-mode TestWorld + steps beside bevy_npa) — placed per the Mission 1
layout.

**Primary regression to encode:** the resource remove → **re-insert** scenario —
this is the durable engine-level home for the bug fixed in Mission 0, which has no
namako home today (resident mode can't reproduce it; only the dirty-trim sim path
does).

**Behavioral template:** core `test/specs/features/07_resources.feature` is the
rich resource-lifecycle contract (insert/static/late-join/field-coalesce/remove/
insert-remove-RE-INSERT-with-different-value via `MatchState`/authority/priority/
world-isolation) and PASSES in resident mode. Port its behaviors to sim-pipelined
mode where the bug actually lives.

**Gate:** new `.feature` scenarios green via the sim NPA; moat 54/54 unaffected.
