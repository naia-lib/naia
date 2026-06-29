---
title: "MISSION — naia pipelined-sim consumer API + boundary restoration"
status: G3a COMPLETE — MISSION REFRAMED (§2f: one authored tick, two modes) — G7 detailed design (§2g) + G8–G10 pending sign-off
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

#### The naia way: explicit method sequence, NO hooks/closures/traits (Connor 2026-06-29)

naia's existing core API is an **explicit imperative sequence the consumer writes**, interleaving their own logic between naia method calls (`server/src/lib.rs` resident main loop):
```rust
server.receive_all_packets();
server.process_all_packets(world, &now);
let ticks = server.take_tick_events(&now);
//  ... consumer's own logic ...
server.send_all_packets(world);
```
Pipelining follows the **same shape**. The generic mechanism is encapsulated behind two fat, mode-aware methods; the consumer's own code runs *between* them, calling the unified `Server` ops directly. **No callbacks, no builder closures, no consumer trait, no hook concept.**

```rust
server.receive(&mut world);   // PIPELINED: park + single recv-drain.  RESIDENT: receive_all + process_all.
//  ... consumer code, identical in both modes:
//      run the sim; server.spawn_replicated(...); server.room_add_entity(...);
//      for msg in server.receive_tick_buffer_messages(tick) { ... }
server.send(&mut world);      // PIPELINED: freeze-point send-prep + snapshot + send-job + unpark (worker).
                              // RESIDENT: send_all_packets inline.
```

The decisive realization: **none of the five "phases" needs to be a hook.**
- *Simulate* = the consumer's own code (no naia involvement).
- *ExtractCommands / FlushReplication / Scope* = the consumer calling existing methods (`receive_tick_buffer_messages`, `spawn_replicated`/`configure_entity_replication`, `room_add_entity`/user-scope) — exactly as in the resident loop.
- *BuildSnapshot* is **not a consumer concern at all** — naia already reads replicated state from the world generically via `WorldRefType`, and the trimmed `SnapshotWorld` is an internal pipeline optimization naia owns end-to-end (needed-set + component-copy are generic). The consumer never authors a snapshot.

So the only thing naia must encapsulate is the bracket: `receive` (park + recv-drain) and `send` (freeze-point send-prep + snapshot + send-job + unpark). Everything else is the consumer's normal code calling normal methods.

#### Two modes, one authored tick

`ServerMode::{Resident, Pipelined}` is the single knob. **There is NO separate `SendStrategy`** — `Pipelined` *implies* worker-thread send; `Resident` *implies* synchronous send. They are a byte-identical pair (the moat requirement), so the deterministic oracle simply runs in **`Resident`** mode — no "pipelined-but-synchronous" variant, retiring cyberlith's `#[cfg(feature = "deterministic")]` send fork.

Switching resident↔pipelined changes nothing in the consumer's code — `receive`/`send` keep the same signatures; only what they do internally differs. **G3a + G3c are the load-bearing substrate**: because the consumer's interleaved code calls the unified `Server` op surface, the *identical* code runs against either the resident `WorldServer` or the pipelined handles.

> OPEN (G9 design pass): confirm `Resident`-mode wire output is byte-identical to `Pipelined` so the deterministic oracle can run as `Resident`. Today cyberlith's deterministic build is pipeline-split + synchronous send, NOT pure resident — validate the collapse before relying on it (claim discipline). If they're not byte-identical, that's a real finding to resolve, not a knob to re-add.

#### Performance preservation (non-negotiable)

`send` (pipelined) executes the **exact same sequence** cyberlith hand-rolled — same single park, same freeze-point `prepare_send_job`, same one-tick lag. Byte-exact identity ⇒ the determinism/desync moat keeps validating it ⇒ the perf floor is preserved by construction. This is the G10 cutover gate: the moat must stay byte-exact-green, and `bench_profile` per-phase spans must match (no new barriers, no double-park).

#### Layering

- naia-server **core (FIRST — Connor 2026-06-29)**: the framework-agnostic `receive`/`send` mode-aware methods owning the skeleton + the four generic phases, over `RecvHandle`/`SendHandle`/`CoordHandle` + `WorldMutType<E>`. No consumer trait, no closures.
- naia-bevy-server **adapter (layered on top)**: maps the bracket onto the existing `ReceivePackets` / `SendPackets` system-set ordering — pipelined mode makes those sets run the parked/worker version internally. The consumer's systems sit between the sets exactly as in resident mode (`app.add_systems(Update, my_system)`), with **zero new consumer-facing concepts**. The runner manages handle transit and operates on a consumer-chosen entity world (so a Sim-SubApp split, like cyberlith's, is just a choice the consumer's systems make — not baked into naia).

### 2g. G7 design pass — the `receive`/`send` bracket, in detail (PROPOSED — pending Connor sign-off, 2026-06-29)

All method/file cites below are VERIFIED against the current naia tree.

#### Placement + signatures

`receive`/`send` are inherent methods on **`PipelinedServer<E>`** (naia-server core). Their signatures match the resident equivalents (G9 adds the same two methods to resident `Server<E>`, bundling `receive_all_packets`+`process_all_packets` and `send_all_packets`), so the consumer's interleaved tick code is source-identical across modes:

```rust
impl<E> PipelinedServer<E> {
    // PIPELINED: park_workers() + single recv-drain + apply recv events to world.
    pub fn receive<W: WorldMutType<E>>(&mut self, world: &mut W);
    // PIPELINED: send-prep + snapshot build + send-job publish + unpark_workers().
    pub fn send<W: WorldRefType<E> + Sync>(&mut self, world: &W);
}
```

The "single knob, consumer code unchanged" property is realized at the bevy layer by `ServerImpl` (G3c). A unified *core* server type (`Resident | Pipelined` enum) is OPTIONAL future ergonomics — NOT in G7. For a core binary, mode is the constructor you call; the interleaved op code is textually identical because both types expose the same op surface (G3a gave `PipelinedServer` the coord ops; `Server<E>` already has them).

#### Threading ownership move (the substantive G7 work)

Today the worker threads + park/unpark barrier + worker loops live in the **bevy adapter** (`plugin_full.rs`: `park_workers`/`unpark_workers`/`spawn` at `:649/:718/:559-607`). For `receive`/`send` to be self-contained and framework-agnostic, this runtime moves **into naia-server core**, owned by `PipelinedServer<E>` (or a `PipelineRuntime` it holds): thread spawn, the parked-count barrier, the recv/send worker loops, and the `SnapshotSender`/`SnapshotReceiver` wiring (the slot type is already core — `pipeline_actors/snapshot_sender.rs`). Uses only `std::thread` + channels — no bevy. The bevy adapter then *calls* `receive`/`send` from the `ReceivePackets`/`SendPackets` sets instead of hand-rolling the window.

#### `receive` internal sequence

1. `park_workers()` — workers deposit handles in their slots.
2. single synchronous recv-drain: `RecvHandle::receive()` (`pipeline_handles.rs:101`) → apply `ReceiveOutput` events to `world` (the `WorldMutType` spawn/insert/despawn application currently in the bevy `drain_recv_impl_split` / `apply_recv_to_world`, generalized to `WorldMutType<E>`).
3. leave `coord`/`recv`/`send` held on `self` for the duration of the window (existing `take_handles` shape).

Between `receive` and `send` the consumer runs its own code with workers parked — calling `self.spawn_replicated(...)`, `self.room_add_entity(...)`, `self.receive_tick_buffer_messages(tick)` (G3a/G4/G5 ops). This is the park window, now implicit.

#### `send` internal sequence (the heart of the question)

Mapped to verified `SendHandle`/`SendState` methods, preserving cyberlith's load-bearing order:

1. `apply_pending_send_preamble()` (`pipeline_handles.rs:293`) — drain room changes / configure-repl; flush handshake + heartbeats.
2. `apply_pending_scope_changes(world)` (`:336`) — publish freshly-scoped entities into per-user send connections. Needs `WorldRefType`.
3. `refresh_needed_entities()` (`:303`) — recompute the cross-thread needed-set.
4. **Build the `SnapshotWorld<E>`** from `world` + `SendStateView::needed_live_and_snapshot_entries()` (`send_state_view.rs`) — a core, `WorldRefType<E>`-based assembler generalizing the bevy `build_snapshot` (`snapshot_builder.rs:45`). The trim is naia-internal; the consumer never authors a snapshot. **RESOLVED (2026-06-29, traced):** the assembler closes entirely on `WorldRefType<E>` — `world.component_of_kind(&e,&kind)` (`world_type.rs:39`) → `ReplicaDynRefWrapper` derefs to `&dyn Replicate` (`replica_ref.rs:154`) → `.copy_to_box()` (`replicate.rs:96`) → `Box<dyn Replicate>` for `SnapshotWorld::insert_component` (`snapshot_world.rs:193`). **No `SnapshotReaderRegistry` lift needed.** (The bevy adapter MAY keep its registry-based `&World` assembler as a perf fast-path — measured choice, not a correctness requirement.)
5. Send-job:
   - **Pipelined (Worker):** `prepare_send_job(&snapshot)` (`:254`) captures frozen `DiffMask`s + clears live masks at the freeze point → `snapshot.attach_send_plan(plan)` → `snapshot_sender.send(snapshot)`. The send worker drains the slot and transmits **next tick** (the one-tick lag — MISSION_TICK_FLOOR Lever 3).
   - **Resident / deterministic oracle:** `send_all_packets(snapshot)` (`:245`) inline; no slot, no lag.
6. `unpark_workers()` — closes the window.

#### World model

`receive` takes `&mut W: WorldMutType` (applies recv events); `send` takes `&W: WorldRefType + Sync` (reads for the snapshot). In a single-world consumer these are the same world. cyberlith's Sim-SubApp-vs-main split is purely the bevy adapter's choice of *which* world to pass each call — not baked into core.

#### Open questions for sign-off

1. Threading move: confirm relocating the worker runtime from `plugin_full.rs` into naia-server core (owned by `PipelinedServer`) is in-scope for G7 (it's required for a self-contained core bracket). — **CONFIRMED in-scope (Connor 2026-06-29).**
2. Snapshot assembler: core `WorldRefType`-based build vs lifting `SnapshotReaderRegistry` to core. — **RESOLVED (item 4): pure `WorldRefType` + `copy_to_box`, no registry lift.**
3. Unified core server enum: defer to post-G9 (not G7). — **CONFIRMED deferred (Connor 2026-06-29).**

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
| G7 | **naia-server core `receive`/`send` bracket** (FIRST) — `PipelinedServer::receive(&mut world)` (park + single recv-drain + apply events) and `::send(&world)` (send-prep + snapshot + send-job + unpark); **+ moves the worker-thread runtime from the bevy adapter into core**. Explicit method sequence; consumer interleaves own code via unified `Server` ops. **No trait, no closures, no hooks.** **Supersedes** the old `with_parked_tick`. **Detailed design: §2g.** | PENDING (design §2g — pending sign-off) |
| G8 | **naia-bevy-server mode-aware system sets** (layered on G7) — pipelined mode makes the existing `ReceivePackets` / `SendPackets` system sets run the parked/worker bracket internally; consumer systems sit between them via plain `add_systems(Update, …)`. **Zero new consumer-facing concepts.** Manages handle transit + consumer-chosen entity world. | PENDING (design §2f — pending sign-off) |
| G9 | **`ServerMode::{Resident, Pipelined}` — single knob, no `SendStrategy`** — Pipelined⇒worker send, Resident⇒synchronous send. Same `receive`/`send` signatures in both; consumer code unchanged (relies on G3c unified `Server` param). Deterministic oracle runs as `Resident`. **OPEN: validate Resident≡Pipelined byte-identity before relying on the oracle collapse.** | PENDING (design §2f — pending sign-off) |
| G10 | **cyberlith cutover** (cyberlith worktree) — delete `open_park_window`/`do_park_window_tick`/`close_park_window` + the `drain_sim_*` glue; re-express as ordinary systems around naia's `ReceivePackets`/`SendPackets` sets; `cell.update()` collapses to naia's bracket. **Gate: determinism/desync moat byte-exact-green** (= perf floor preserved). | PENDING (design §2f — pending sign-off) |
| M2 | sim-namako BDD specs written against the `receive`/`send` bracket contract (G1–G9), not the leaked shape | PENDING (after G1–G9) |

Each pending group = design sub-pass + Connor sign-off before impl.

## 4. Working model — worktrees (per audit spec §7.5)

- naia: feature branch off `dev` (branched after M1 reorg at `6ce04cc4`).
- cyberlith: feature branch off `main`, naia path-dep repointed at naia worktree.
- Land atomically: naia→`dev`, cyberlith→`main`.
- Mainline session (cyberlith action plan) stays on primary checkouts; sees no churn until land.

## 5. Absorbed items

- `enable_entity_replication` fail-loud guard (`naia dev 350f00c2`): committed but **moot/absorbed** — under G3/G6, `enable_entity_replication` becomes naia-internal; the invariant is enforced by API contract. Will be superseded when G3 lands.
- M2 (sim-namako BDD coverage): reshaped as the executable contract for G1–G9 — the `receive`/`send` bracket contract of §2f, validated under both modes.

## 6. Gates (from audit spec §8)

- **Perf preservation (G10 gate):** cyberlith determinism/desync moat stays byte-exact-green through the cutover — the runner must execute the same single-park / freeze-point send-prep / one-tick-lag sequence. Byte-exact identity IS the perf-floor guarantee. Also confirm `bench_profile` per-phase spans match pre-cutover (no new barriers, no double-park).
- `server_access.rs` (until deleted at G10): zero `WorldServer` reassembly, zero handle `.take()`, zero `HostSyncEvent` construction. At G10 the file's park-window machinery is gone entirely.
- naia-primitive tripwire (cybertool check) green with empty/justified allowlist.
- naia sim-namako specs cover G1–G6 behavior (incl. resource remove→re-insert).
- cyberlith determinism moat green; naia-isolation green; native + wasm32 build.
