---
title: "MISSION — naia pipelined-sim consumer API + boundary restoration"
status: G1 IN PROGRESS (Option B + Strategy 1 approved by Connor 2026-06-29)
domain: architecture / engine-boundary
owner: connorcarpenter
origin: "2026-06-29 cyberlith↔naia boundary audit (after resource_replication.rs layering regression)"
governing_rule: "naia owns ALL pipelined-sim functionality; cyberlith consumes ONLY via ergonomic naia APIs (surfaced through diax_net_*)."
---

# MISSION — naia pipelined-sim consumer API + boundary restoration

> Full audit verdict + per-finding ledger lives in the cyberlith-side doc:
> `../cyberlith/_AGENTS/MISSION_NAIA_PIPELINE_API_BOUNDARY.md`.
> This naia-side file records design decisions, layering choices, and Connor sign-offs.

## 1. Core layering rule (Connor 2026-06-29)

All G1–G5 mechanics go in **`naia-server` core** (framework-agnostic).
G6 carrier management also goes in `naia-server` core; the `Res<R>` bevy surface goes in `naia-bevy-server`.

The bevy adapter's role is unchanged: thin delegation shims + bevy-native ergonomics (injectable resources, `CommandsExt`, `Res<R>`). A non-bevy consumer of naia-server gets G1–G5 against their own `WorldMutType<E>` impl.

## 2. Design decisions — RESOLVED

### 2a. G1 tick-driver shape — **APPROVED: Option B (Connor 2026-06-29)**

`spawn_server_handles` returns `SimPipeline<E>` (handles are internal).
```rust
pipeline.tick(&mut world_proxy, |ctx| { ... });
pipeline.mark_entity_as_static(&entity);   // G3 also on same type
```
Implemented:
- `SimPipeline<E>` + `TickCtx<'_, E, W>` in `naia-server`
- `SimPipelineRes(pub Option<SimPipeline<Entity>>)` replaces `SimHandleRes` in `naia-bevy-server`
- `take_sim()`/`restore_sim()` on `SimPipeline` for `drain_recv_impl_split` (which keeps taking recv/send via caller-provided Arc slots for backward compat)
- `PluginInternalState` fields: `armed_pipeline` replaces `armed_handles` + `armed_sim_handle`
- All naia-server + naia-bevy-server tests updated + passing

### 2b. G3 pub(crate) strategy — **APPROVED: Strategy 1 (Connor 2026-06-29)**

Thin named methods on `SimHandle<E>` / `SimPipeline<E>`. Bevy adapter `CommandsExt` becomes a shim.
```rust
sim.mark_entity_as_static(&entity);
sim.configure_entity(&entity, config);
sim.take_entity_authority(&entity, &user_key);
```

### 2c. Cyberlith lane — **DECIDED: Tycho owns both worktrees**

naia feature branch + cyberlith feature branch (naia path-dep repointed). Land atomically.

## 3. Sequence + status

| Step | Description | Status |
|------|-------------|--------|
| G1 | `SimPipeline<E>` + `TickCtx<E,W>` tick-driver; `SimPipelineRes` in bevy adapter; tests green | **IN PROGRESS** (all core changes landed; workspace tests running) |
| G2 | Startup window API — `io_load` on `SimPipeline` (no raw `WorldServer` reassembly in cyberlith) | PENDING |
| G3 | Coord-only ops as named methods on `SimHandle`/`SimPipeline`; `CommandsExt` becomes shim | PENDING |
| G4 | `spawn_replicated` fused op | PENDING |
| G5 | `enable_replication_for_existing_entity` | PENDING |
| G6 | `Res<R>` resource API (`SimPipeline::insert_resource` etc.) | PENDING |
| G7 | Ergonomic single-call opt-in wrapper: `PluginInternalState::with_parked_tick(world, \|ctx\| {...})` collapses park/tick/unpark into one bevy-adapter call; non-pipelined consumer keeps `Server` resource path unchanged | PENDING (Connor-approved 2026-06-29) |
| M2 | sim-namako BDD specs written against G1–G6+G7 contract, not the leaked shape | PENDING (after G1–G7) |

Each pending group = design sub-pass + Connor sign-off before impl.

## 4. Working model — worktrees (per audit spec §7.5)

- naia: feature branch off `dev` (branched after M1 reorg at `6ce04cc4`).
- cyberlith: feature branch off `main`, naia path-dep repointed at naia worktree.
- Land atomically: naia→`dev`, cyberlith→`main`.
- Mainline session (cyberlith action plan) stays on primary checkouts; sees no churn until land.

## 5. Absorbed items

- `enable_entity_replication` fail-loud guard (`naia dev 350f00c2`): committed but **moot/absorbed** — under G3/G6, `enable_entity_replication` becomes naia-internal; the invariant is enforced by API contract. Will be superseded when G3 lands.
- M2 (sim-namako BDD coverage): reshaped as the executable contract for G1–G6.

## 6. Gates (from audit spec §8)

- `server_access.rs`: zero `WorldServer` reassembly, zero handle `.take()`, zero `HostSyncEvent` construction.
- naia-primitive tripwire (cybertool check) green with empty/justified allowlist.
- naia sim-namako specs cover G1–G6 behavior (incl. resource remove→re-insert).
- cyberlith determinism moat green; naia-isolation green; native + wasm32 build.
