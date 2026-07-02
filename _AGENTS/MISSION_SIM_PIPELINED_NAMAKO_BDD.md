---
title: "MISSION - sim-pipelined Namako BDD coverage"
status: OPEN
origin: "2026-06-29 pipeline/API-boundary follow-up; persisted as standalone handoff 2026-07-02"
governing_rule: "Naia's primary executable specs must cover sim-pipelined behavior directly; Cyberlith's moat remains the external byte-exact referee."
---

# MISSION - sim-pipelined Namako BDD coverage

## Status

OPEN. This is follow-up testing coverage, not unfinished pipeline refactor work.

The pipeline elegance Phase C/D refactor and API-boundary P4/P5 work are complete:

- staged send-resident op classes landed on `dev`
- `with_world_server` is narrowed to the intended monolithic uses
- plugin constructors are unified
- `Scenario::new(ServerMode)` makes harness mode selection explicit
- `g8_real_ack_byte_identity.rs` covers real-client ACK byte identity

What remains is a Namako/BDD coverage gap for the sim-pipelined path.

## The Gap

Naia's sim-pipelined / sim-integration mode, which is the mode Cyberlith runs in
production, still has no Namako BDD adapter or feature specs.

As of 2026-07-02, fresh checks showed no:

- `test/sim_npa/`
- `test/sim_specs/`
- `sim_*.feature`

Current coverage is raw Rust tests under `adapters/bevy/server/tests/`, including
the `sim_*` and `apply_receive_output_with_sim_receiver` families:

- `sim_replicated_resources`
- `sim_host_sync`
- `sim_integration_iris2`
- `sim_enable_entity_replication`
- `sim_configure_entity_replication`
- `sim_configure_entity_replication_deferred`
- `sim_mark_entity_as_static`
- `sim_converter`
- `sim_event_receiver`
- `sim_integration_full_lifecycle`
- `sim_integration_full_park`
- `sim_integration_full_shutdown`
- `sim_integration_full_panic`
- `sim_integration_full_tick_buffer`
- `sim_integration_full_bracket_drive`
- `sim_integration_schedule_label`

Those tests are useful, but the primary spec surface is Namako. Sim-pipelined
behavior needs BDD scenarios that exercise the same public contract users rely on.

## Mission

Build a sim-mode NPA adapter and feature specs for the pipelined sim path.

Expected shape:

- Add a sim adapter, likely `test/sim_npa/`, analogous to `test/bevy_npa/`.
- Add feature specs, likely `test/sim_specs/`, analogous to `test/bevy_specs/`.
- Use the current repo layout and naming conventions; confirm naming only if the
  existing pattern does not fit.
- Drive the public bracket/API contract, not old Cyberlith glue.

The adapter should be able to drive:

- `spawn_server_handles`
- a real-ish client path where needed
- host-sync drain into the pipeline
- snapshot build / send packet flow
- receive/send bracket behavior

The feature specs should port the important behaviors from the raw sim tests:

- enable/configure replication
- room scope and user/entity visibility
- static entity marking
- entity spawn, insert, mutate, remove, despawn
- resource insert, mutate, remove, and re-insert
- scope/FoW-sensitive replication
- late join behavior
- panic/lifecycle/bracket-drive semantics where they are part of the public contract

## Primary Regression

Encode the resource remove -> re-insert regression.

This was a real production bug in the sim-pipelined path: removing and then
re-inserting a replicated resource could silently fail to replicate or trigger an
under-supply panic during sim-mode dirty trim.

The Naia-side fix lives in:

- `shared/src/world/host/host_world_manager.rs`
- `delivered_component_kinds` tracking

Resident-mode specs cannot reproduce the original failure because resident mode
does not exercise the same sim dirty-trim path. The Namako scenario must target
the sim-pipelined API.

Use today's Naia API surface:

- `pipeline_replicate_resource`
- `pipeline_remove_replicated_resource`
- related pipeline drain/bracket APIs

Do not port against the old Cyberlith resource carrier glue. That Cyberlith-side
machinery was superseded and deleted after the pipeline API moved into Naia.

## Behavioral Template

Use `test/specs/features/07_resources.feature` as the resource-lifecycle model.

That resident-mode spec covers:

- insert
- static resources
- late join
- field coalescing
- remove
- insert -> remove -> re-insert with a different value
- authority and priority interactions
- world isolation

Port the behaviors that are meaningful to sim-pipelined mode, especially the
resource re-insert case that only the sim path can validate.

## Gates

Before landing:

- New Namako feature scenarios pass through the sim NPA.
- Existing Namako lint/gate remains green.
- Naia workspace tests remain green.
- Cyberlith moat remains green:

```bash
cargo test -p cyberlith_test_harness --test simulation --features desync_detection
```

For broad changes to the sim pipeline, also run the full moat set documented in
`MISSION_PIPELINE_ELEGANCE.md`.

## Non-Goals

- Do not change wire format.
- Do not loosen Cyberlith moat assertions, regenerate snapshots, skip tests, or
  gate behavior behind test-only flags to make failures pass.
- Do not resurrect deleted Cyberlith pipeline glue. Naia owns pipelined-sim
  functionality; Cyberlith consumes it.
- Do not treat raw `#[test]` coverage as a substitute for the BDD adapter. Raw
  tests may remain, but the mission is to create the primary Namako surface.

