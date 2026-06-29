---
title: "MISSION — naia pipelined-sim consumer API + boundary restoration"
status: G3a COMPLETE — MISSION REFRAMED (§2f: one authored tick, two runners) — G7–G10 design pending sign-off
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

`PipelinedServer::new(config, protocol)` returns `PipelinedServer<E>` (handles are internal).
```rust
pipeline.tick(&mut world_proxy, |ctx| { ... });
pipeline.mark_entity_as_static(&entity);   // G3 also on same type
```
Implemented (post G1 rename):
- `PipelinedServer<E>` + `TickCtx<'_, E, W>` in `naia-server` (formerly `SimPipeline<E>`)
- `CoordHandle<E>` (formerly `SimHandle<E>`) — main-thread coordination handle; exposed as `ctx.coord` inside tick body
- Bevy resource `PipelinedServer` (no suffix; formerly `SimPipelineRes`) wraps `Option<PipelinedServer<Entity>>`
- `take_sim()`/`restore_sim()` → `take_coord()`/`restore_coord()` on `PipelinedServer`
- `PluginInternalState` fields: `armed_pipeline` (unchanged)
- All naia-server + naia-bevy-server tests updated + passing

### 2b. Naming decisions — **SIGNED OFF (Connor 2026-06-29)**

Full rename table (finalized):

| Old name | New name | Notes |
|---|---|---|
| `SimPipeline<E>` | `PipelinedServer<E>` | Core pipeline handle |
| `SimHandle<E>` | `CoordHandle<E>` | Main-thread coord; `ctx.coord` inside tick |
| `SimPipelineRes` (Bevy resource) | `PipelinedServer` | No suffix; IS the server resource |
| `SimConverter<E>` | `ServerEntityConverter<E>` | Not `EntityConverter` — avoids confusion with naia_shared naming |
| `spawn_server_handles(config, proto)` | `PipelinedServer::new(config, proto)` | Constructor on the type |
| `PluginSimConfig` | `PipelineConfig` | |
| `Plugin::sim_integration_full()` | `Plugin::pipelined(...)` | |
| `Plugin::sim_integration()` variants | `Plugin::pipelined()` variants | |
| `SimEventReceiver<E>` + `SimEventReceiverRes` | deleted / internalized | Events flow through existing Bevy event types |
| `SimConnectEvent`, `SimTickEvent`, `SimSpawnEntityEvent`, etc. | deleted — reuse existing `ConnectEvent`, `TickEvent`, `SpawnEntityEvent` etc. | Confirmed: no Cyberlith filtering on Sim* vs base events |
| `TickCtx<'a, E, W>` | `TickCtx<'a, E, W>` | Keep; `coord` field name replaces `sim` |

### 2c. G3 design — **SIGNED OFF (Connor 2026-06-29)**

**G3a** (naia-server core): forwarding methods on `SimPipeline<E>` for every public coord-only op already on `SimHandle<E>` — entity ops, room ops, user ops, authority reads, tick. `enable_entity_replication` excluded (G5).
```rust
sim_pipeline.mark_entity_as_static(&entity);
sim_pipeline.configure_entity_replication(&entity, config);
sim_pipeline.entity_authority_status(&entity);
sim_pipeline.room_add_entity(&room_key, &entity);
// ... etc. — all SimHandle<E> methods forwarded
```

G3a is IMPLEMENTED (`PipelinedServer<E>` forwarding methods landed; naming finalized to `PipelinedServer`/`CoordHandle`). The forwarded set: entity reads (`is_resource_entity`, `entity_owner`, `entity_authority_status`, `entity_is_static`, `entity_converter`), user ops (`user_exists`, `user_keys`, `user_address`, `receive_user`, `disconnect_user`), room ops (`create_room`, `room_destroy`, `room_add_user`, `room_remove_user`, `room_add_entity`, `room_remove_entity`), replication config (`mark_entity_as_static`, `configure_entity_replication`, `apply_pending_world_hooks`), tick/queue introspection (`current_tick`, `scope_change_queue_len`, `pending_world_hooks_len`). `enable_entity_replication` excluded (G5).

**G3b** (cyberlith worktree): D11 `CellCommandsExt` dies; cyberlith Sim systems call `pipelined_server.method()` directly in the park window. Bevy adapter `CommandsExt` stays unchanged — it's for the resident path and deferred `Commands` semantics the pipelined path doesn't need.

### 2d. Cyberlith lane — **DECIDED: Claude owns both worktrees**

naia feature branch + cyberlith feature branch (naia path-dep repointed). Land atomically.

### 2e. Unified `Server` SystemParam — **DECIDED: `ServerImpl::Pipelined` variant (Connor 2026-06-29)**

Bevy server apps access the server through the single `Server<'w>` `#[derive(SystemParam)]`, which wraps the private `#[derive(Resource)] enum ServerImpl`. Today: `Full(naia_server::Server<Entity>)` + `WorldOnly(WorldServer<Entity>)`. **Decision: add a third variant `Pipelined(PipelinedServer<Entity>)`** so pipelined consumers use the same `Server` param — no separate `PipelinedServer` SystemParam, no raw `ResMut<PipelinedServer>` + `.0.as_mut()` ceremony.

Design constraints (the two access disciplines do NOT fully unify — encode this honestly):

- **Coord-safe subset → real `Pipelined` arms.** The coord handle rests on the main thread between ticks (`PipelinedServer.coord: Option<…>`); coord-only ops push to lock-protected queues that are safe against the concurrently-running recv/send workers by design. So the entire G3a forwarded set gets genuine `Pipelined` arms callable from any main-thread system: `create_room`, `room_*`, `user_*`/`receive_user`/`disconnect_user`, entity reads, `mark_entity_as_static`, `configure_entity_replication`, `current_tick`, queue introspection.
- **Send-side / full-server methods → loud panic arms.** `send_message`, `broadcast_message`, `send_request`/`send_response`/`receive_response`, `accept_connection`/`reject_connection`/`listen`, `user_scope_mut`, `global_entity_priority_mut`, `record_historian_tick`, `entity_take_authority`, resource ops — these route through the send handle / reassembled `WorldServer` and are only valid inside the `tick()` park window. Their `Pipelined` arm is `unimplemented!`/`panic!` with a clear "not valid in pipelined mode — perform this inside the tick window" message. Accepted leak: pipelined consumers do all send-side work inside the existing drain/tick systems, so these arms are unreachable in practice.
- The park-window discipline is still enforced by the plugin's drain/tick systems (the type cannot encode "only valid when parked"); the `Pipelined` variant only relocates the existing `PipelinedServer` resource into the enum and routes coord-safe methods.

This is the chosen direction over a separate `PipelinedServer<'w>` param: maximum API uniformity, single consumer-facing param across all three modes.

### 2f. Pristine end-state — one authored tick, two runners (PROPOSED — pending Connor sign-off, 2026-06-29)

**The realization that reframes the mission.** The entire pipeline coordinator — park/unpark, the synchronous recv drain, the freeze-point send-prep sequence, the one-tick-lag send-job prep, the deterministic synchronous send path, and the handle-transit dance — is hand-rolled in cyberlith `game_cell` (`cell.rs::update` + `server_access.rs::{open_park_window, do_park_window_tick, close_park_window}`, ~600 lines). Tracing it line-by-line: **almost all of it is generic pipeline MECHANISM, not cyberlith policy.** The perf-critical, byte-exactness-sensitive pieces (single-park barrier, freeze-point send-prep, one-tick-lag send-job, deterministic oracle path) are exactly the generic parts. Every naia consumer that wants pipelining is otherwise forced to re-derive this by hand. The mechanism belongs in naia.

This supersedes the piecemeal G7 (`with_parked_tick`): that only handed back a parked window with raw handles and left the consumer to hand-assemble send-prep/snapshot/send-job inside the closure. The pristine design has naia own the *whole skeleton*.

#### Canonical pipelined tick (naia-owned skeleton)

Fixed sequence; the **bold** rows are generic mechanism naia owns end-to-end:

| Phase | Owner |
|---|---|
| **Park workers + transit handles** | naia |
| **Synchronous recv drain (single park/tick — the barrier-orch optimization)** | naia |
| ExtractCommands — drain tick-buffer messages → game actions | consumer hook |
| Simulate — run gameplay | consumer hook |
| FlushReplication — register/configure/host-sync entities (via unified `Server` ops) | consumer hook |
| Scope — apply visibility policy (via unified `Server` scope ops) | consumer hook |
| **Freeze-point send-prep: `apply_pending_send_preamble` + `apply_pending_scope_changes` + `refresh_needed_entities`** | naia |
| BuildSnapshot — produce the (optionally trimmed) `SnapshotWorld` | consumer hook |
| **Send-job: `prepare_send_job` + `attach_send_plan` + publish to send worker (one-tick lag) OR synchronous `send_all_packets`** | naia |
| **Unpark workers** | naia |

#### Two runners, one authored tick

The consumer authors the five policy hooks **once** (bevy: systems in naia-defined canonical phase-schedule labels; non-bevy: trait methods / closures). naia provides two runners that execute those same hooks:

- **Pipelined** — workers + park/unpark + worker-thread send + one-tick lag.
- **Resident** — synchronous, monolithic `Server`, inline send. `SendStrategy::{Worker, Synchronous}` is runner config (the deterministic oracle becomes `Synchronous`, replacing cyberlith's `#[cfg(feature = "deterministic")]` fork).

Switching resident↔pipelined is a one-line runner/config change. **G3a + G3c are the load-bearing substrate, not mere ergonomics**: because the hooks call the unified `Server` op surface (`server.room_add_entity`, `server.configure_entity_replication`, …), the *identical* hook code runs against either the resident `WorldServer` or the pipelined handles.

#### Performance preservation (non-negotiable)

The runner executes the **exact same sequence** cyberlith hand-rolled — same single park, same freeze-point `prepare_send_job`, same one-tick lag, same deterministic synchronous oracle. Byte-exact identity ⇒ the determinism/desync moat keeps validating it ⇒ the perf floor is preserved by construction. This is the gate for the cyberlith cutover (G10): the moat must stay byte-exact-green.

#### Layering

- naia-server **core**: the framework-agnostic `PipelineDriver` owning the skeleton + the four generic phases, parameterized by hooks (trait/closure form) over `RecvHandle`/`SendHandle`/`CoordHandle` + `WorldMutType<E>`.
- naia-bevy-server **adapter**: canonical phase-schedule labels + a runner that maps labels→hooks, manages handle transit, and operates on a consumer-chosen entity world (so a consumer's Sim-SubApp split, like cyberlith's, is just a choice the Simulate/BuildSnapshot hooks make — not baked into naia).

## 3. Sequence + status

| Step | Description | Status |
|------|-------------|--------|
| G1 | `SimPipeline<E>` + `TickCtx<E,W>` tick-driver; `SimPipelineRes` in bevy adapter; tests green | ✅ COMPLETE (`55272fad`) |
| G2 | `SimPipeline::listen(socket)` startup-window API; `PluginInternalState::listen` delegates to it | ✅ COMPLETE (`1e851a73`) |
| G3a | forwarding methods on `PipelinedServer<E>` for all coord-only ops | ✅ COMPLETE (`175d4bc7` rename + G3a impl) |
| G3b | cyberlith D11 `CellCommandsExt` dies, replaced by direct `pipelined_server.method()` calls in the park window | PENDING (design signed off) |
| G3c | unified `Server` param: add `ServerImpl::Pipelined(PipelinedServer<Entity>)` variant; coord-safe methods get real arms, send-side methods get loud panic arms; retire raw `ResMut<PipelinedServer>` access | PENDING (design signed off §2e) |
| G4 | `spawn_replicated` fused op | PENDING |
| G5 | `enable_replication_for_existing_entity` | PENDING |
| G6 | `Res<R>` resource API (`SimPipeline::insert_resource` etc.) | PENDING |
| G7 | **naia-server core `PipelineDriver`** — owns the canonical pipelined-tick skeleton (park → recv-drain → freeze-point send-prep → send-job → unpark) + the four generic phases, parameterized by policy hooks (trait/closure form). Framework-agnostic. **Supersedes** the old minimal `with_parked_tick` wrapper. | PENDING (design §2f — pending sign-off) |
| G8 | **naia-bevy-server schedule-driven runner** — canonical phase-schedule labels (ExtractCommands / Simulate / FlushReplication / Scope / BuildSnapshot); a `PipelinedRunner` maps labels→hooks, manages handle transit, and drives a consumer-chosen entity world. Consumer registers systems in the labels. | PENDING (design §2f — pending sign-off) |
| G9 | **Resident runner parity** — same phase labels execute synchronously via the monolithic `Server`; `SendStrategy::{Worker, Synchronous}` runner config (deterministic oracle = `Synchronous`). Resident↔pipelined = one config line. Relies on G3c unified `Server` param so hooks are mode-agnostic. | PENDING (design §2f — pending sign-off) |
| G10 | **cyberlith cutover** (cyberlith worktree) — delete `open_park_window`/`do_park_window_tick`/`close_park_window` + the `drain_sim_*` glue; re-express them as phase systems in naia's labels; `cell.update()` → `runner.tick()`. **Gate: determinism/desync moat byte-exact-green** (= perf floor preserved). | PENDING (design §2f — pending sign-off) |
| M2 | sim-namako BDD specs written against the phase-hook contract (G1–G9), not the leaked shape | PENDING (after G1–G9) |

Each pending group = design sub-pass + Connor sign-off before impl.

## 4. Working model — worktrees (per audit spec §7.5)

- naia: feature branch off `dev` (branched after M1 reorg at `6ce04cc4`).
- cyberlith: feature branch off `main`, naia path-dep repointed at naia worktree.
- Land atomically: naia→`dev`, cyberlith→`main`.
- Mainline session (cyberlith action plan) stays on primary checkouts; sees no churn until land.

## 5. Absorbed items

- `enable_entity_replication` fail-loud guard (`naia dev 350f00c2`): committed but **moot/absorbed** — under G3/G6, `enable_entity_replication` becomes naia-internal; the invariant is enforced by API contract. Will be superseded when G3 lands.
- M2 (sim-namako BDD coverage): reshaped as the executable contract for G1–G9 — the phase-hook contract of §2f, validated under both runners.

## 6. Gates (from audit spec §8)

- **Perf preservation (G10 gate):** cyberlith determinism/desync moat stays byte-exact-green through the cutover — the runner must execute the same single-park / freeze-point send-prep / one-tick-lag sequence. Byte-exact identity IS the perf-floor guarantee. Also confirm `bench_profile` per-phase spans match pre-cutover (no new barriers, no double-park).
- `server_access.rs` (until deleted at G10): zero `WorldServer` reassembly, zero handle `.take()`, zero `HostSyncEvent` construction. At G10 the file's park-window machinery is gone entirely.
- naia-primitive tripwire (cybertool check) green with empty/justified allowlist.
- naia sim-namako specs cover G1–G6 behavior (incl. resource remove→re-insert).
- cyberlith determinism moat green; naia-isolation green; native + wasm32 build.
