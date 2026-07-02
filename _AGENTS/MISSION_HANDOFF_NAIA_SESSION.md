# naia session — mission handoff (2026-06-30)

You are a **naia-scoped** Claude Code session. This brief orients you on the one
open item carried forward from the 2026-06-29 handoff; the two missions that
shipped that day (coupled re-insert fix, test/bench/tools reorg) are done and
archived — see `_AGENTS/ARCHIVE/MISSION_HANDOFF_NAIA_SESSION.md` for the record
if you need the history. Read this fully before touching anything.

## Ground rules (non-negotiable)
- **naia is dev-trunk**: commit on `dev`, NEVER `main`.
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
  App mode). Durable regressions belong in a `.feature` + step def. This is the
  rule the open mission below exists to satisfy.

---

## MISSION 2 — sim-pipelined namako BDD coverage (OPEN)

> Canonical standalone follow-up doc:
> `_AGENTS/MISSION_SIM_PIPELINED_NAMAKO_BDD.md`.

> **Still open after `MISSION_PIPELINE_ELEGANCE` and boundary P4/P5 (verified
> 2026-07-02):** the Phase C/D pipeline elegance work added
> resident≡pipelined-oracle byte-equality specs for the staged op classes, and
> boundary follow-up `3bdaa367` added `g8_real_ack_byte_identity.rs`, but neither
> built the sim-mode NPA adapter requested here. Fresh check: no
> `test/sim_npa/`, no `test/sim_specs/`, and no `sim_*.feature` exist.

**The gap:** naia's sim-pipelined / sim-integration mode (the mode cyberlith runs
in prod) has ZERO namako BDD coverage — only raw `#[test]` files in
`adapters/bevy/server/tests/` (currently 17, prefixed `sim_*` /
`apply_receive_output_with_sim_receiver`), e.g. `sim_replicated_resources`,
`sim_host_sync`, `sim_integration_iris2`, `sim_enable_entity_replication`,
`sim_configure_entity_replication`(`_deferred`), `sim_mark_entity_as_static`,
`sim_converter`, `sim_event_receiver`, `sim_integration_full_{lifecycle,park,
shutdown,panic,tick_buffer,bracket_drive}`, `sim_integration_schedule_label`.
This violates the primary-surface rule. Verified still true as of 2026-06-30:
no `sim` NPA adapter and no `sim_*.feature` files exist anywhere in the repo.

**The mission (FULL PORT, Connor 2026-06-29):** build a **sim-mode NPA adapter**
(drives `spawn_server_handles` + a client + `drain_host_sync_into_pipeline` +
`build_snapshot` + `send_all_packets`, with given/when/then steps for
enable/configure/room/static + entity spawn/insert/mutate/remove/despawn +
resource insert/mutate/remove/**re-insert** + scope/FoW + late-join) and feature
spec(s) (e.g. `sim_replication.feature`), porting the behaviors in those ~17 raw
tests to BDD scenarios. The new adapter is the sim analog of `test/bevy_npa`
(likely a sim-mode TestWorld + steps beside bevy_npa) — place it per the current
`test/{specs,bevy_specs,npa,bevy_npa,tests,harness,loom,fuzz}/` layout (landed
2026-06-29 via `6ce04cc4`; e.g. `test/sim_npa/` + `test/sim_specs/` would follow
the existing `npa`/`specs` and `bevy_npa`/`bevy_specs` naming pattern, but use
your judgment — confirm naming with Connor if it's not obvious).

**Primary regression to encode:** the resource remove → **re-insert** scenario —
this was a real production bug (silent no-replicate / under-supply panic on
sim-mode dirty-trim) fixed 2026-06-29 in `shared/src/world/host/
host_world_manager.rs` (`delivered_component_kinds` tracking — still in place).
It has no namako home today: resident mode can't reproduce it (no dirty-trim),
only the sim-pipelined path does. NOTE: the cyberlith-side half of that fix
(`services/game/cell/src/resource_replication.rs`) was since superseded and
deleted (cyberlith `7a8fc2a9a`, 2026-06-30) — the resource-replication carrier/
enable machinery now lives in naia itself via the `pipeline_*` API
(`adapters/bevy/server/src/server.rs`, naia `286bdbab`/`00f09b2a`). Port the
re-insert regression against today's `pipeline_replicate_resource` /
`pipeline_remove_replicated_resource` surface, not the old cyberlith carrier
glue described in the archived handoff.

**Behavioral template:** core `test/specs/features/07_resources.feature` is the
rich resource-lifecycle contract (insert/static/late-join/field-coalesce/remove/
insert-remove-RE-INSERT-with-different-value via `MatchState`/authority/priority/
world-isolation) and PASSES in resident mode. Port its behaviors to sim-pipelined
mode where the bug actually lives.

**Gate:** new `.feature` scenarios green via the sim NPA; moat
(`cargo test -p cyberlith_test_harness --features desync_detection --test
simulation`) 54/54 unaffected.
