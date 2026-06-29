---
title: "MISSION — naia pipelined-sim consumer API + boundary restoration"
status: G3a COMPLETE — audit (§2h) incorporated + all blockers RESOLVED (editor in-scope, H3 single-world, §2e internal-queueing no-panic) + G9pre spike COMPLETE/GREEN (§2i — pipelined send content byte-identical to resident; oracle = synchronously-driven Pipelined bracket, NOT Resident) — G7 ready for sign-off
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

**(REWRITTEN per §2h C1/C2 + Connor 2026-06-29 — NO panic arms; internal queueing.)**

The original panic-arm design was wrong. The pristine model: **every mutating unified-`Server` method is available in Pipelined mode from any system at any time** — it does not require the workers to be parked. Methods that can't apply immediately **queue into internal lock-protected buffers** and drain at the correct phase inside `receive`/`send`. This extends naia's *existing* deferred-queue pattern — `scope_change_queue`, `pending_world_hooks`, `pending_disconnect_requests` (`handles.rs`) — to the remaining mutating ops:

- **Reads** (`entity_authority_status`, `user_keys`, `current_tick`, …) → return current coord state (rests on main); always valid.
- **Coord mutations** (`create_room`, `room_*`, `configure_entity_replication`, `mark_entity_as_static`, `spawn_replicated`, `take_authority`, …) → already queue (or newly queue) into coord/shared buffers; drained at their defined phase.
- **Message sends** (`send_message`, `broadcast_message`, `send_message_to_user`) → enqueue into a new internal `pending_outbound_messages` buffer, drained during `send`'s preamble (before transmit). This is what makes C1's desync broadcast and any future per-tick gameplay message work without the consumer touching a handle.
- **Authority ops** (`entity_take_authority`, C2) → queue; drained at the editor-ops phase.

**NO method panics.** The park-window discipline disappears from the consumer's mental model entirely.

**Byte-exactness constraint (load-bearing):** each queue must drain at the *same logical phase* cyberlith applies it today (e.g. `take_authority` BEFORE despawn-replication — `do_park_window_tick` Step 5 ordering; outbound messages before transmit). Drain-point fidelity is a G-impl correctness requirement, validated by the moat.

This keeps the single consumer-facing param (over a separate `PipelinedServer<'w>`) AND makes the full mutating surface first-class in both modes. `listen`/`accept_connection` at startup remain the G2 startup-window path.

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

**(per §2h M4)** G7 also owns the **Armed→Running lifecycle** transition (handle spawn + listen-before-startup ordering currently tangled with bevy `Startup`/`Resource` at `plugin_full.rs:609`), not just thread spawn.

**(per §2h H3 — RESOLVED:)** `receive` is single-world (the entity world) and returns `ReceiveOutput`; the bevy adapter fans the returned events into main vs sim `Messages<X>`. See the World-model section below.

The consumer's own code can call `self.spawn_replicated(...)`, `self.room_add_entity(...)`, `self.send_message(...)`, `self.take_authority(...)`, `self.receive_tick_buffer_messages(tick)` **from any system, any time** (§2e internal-queueing model) — not only between `receive` and `send`. Mutations queue and drain at their defined phase.

#### `do_park_window_tick` step ledger (per §2h M1 — nothing falls through)

Every step of cyberlith's real per-tick body, classified:

| cyberlith step | Disposition |
|---|---|
| open: park + `drain_recv_impl_split` | **core `receive`** |
| Step 1 tick-buffer → PlayerCommands | consumer policy (calls `receive_tick_buffer_messages`) |
| Step 2 gate ticks to Sim | consumer policy |
| Step 3 PostSimSchedule | consumer policy |
| Step 4a UserManagerSnapshot ferry | consumer policy |
| Step 4 Sim SubApp update | consumer policy |
| Step 4b scope-delta drains | consumer policy (intent) → feeds Scope ops |
| Step 5 `drain_sim_registrations` | **G4/G5 ops** (`spawn_replicated`/`enable_replication`) |
| Step 5 `drain_sim_resource_registrations` | **G6 op** (`Res<R>`) |
| Step 5 `drain_sim_lifecycle` | **G4/G5 ops** |
| Step 5 `drain_sim_editor_ops` (`take_authority`) | **G5b** (per C2) |
| Step 5 `drain_sim_host_sync_pipelined` | **G6b** (per H2) |
| Step 5 `broadcast_desync_snapshots` | consumer policy (calls `send_message`, §2e-corrected) |
| Step 7 scope ledger writes | consumer policy → Scope ops |
| Step 7.5 send-prep | **core `send`** (preamble/scope/refresh) |
| Step 6 snapshot build | **core `send`** (internal assembler) |
| Step 6.6 prepare_send_job | **core `send`** |
| Step 8 (deterministic) inline send | **core `send`** (Resident/oracle path) |
| close: unpark | **core `send`** |

#### `send` internal sequence (the heart of the question)

Mapped to verified `SendHandle`/`SendState` methods, preserving cyberlith's load-bearing order:

1. `apply_pending_send_preamble()` (`pipeline_handles.rs:293`) — drain room changes / configure-repl; flush handshake + heartbeats.
2. `apply_pending_scope_changes(world)` (`:336`) — publish freshly-scoped entities into per-user send connections. Needs `WorldRefType`.
3. `refresh_needed_entities()` (`:303`) — recompute the cross-thread needed-set.
4. **Build the `SnapshotWorld<E>`** from `world` + `SendStateView::needed_live_and_snapshot_entries()` (`send_state_view.rs`) — a core, `WorldRefType<E>`-based assembler generalizing the bevy `build_snapshot` (`snapshot_builder.rs:45`). The trim is naia-internal; the consumer never authors a snapshot. **RESOLVED (2026-06-29, traced):** the assembler closes entirely on `WorldRefType<E>` — `world.component_of_kind(&e,&kind)` (`world_type.rs:39`) → `ReplicaDynRefWrapper` derefs to `&dyn Replicate` (`replica_ref.rs:154`) → `.copy_to_box()` (`replicate.rs:96`) → `Box<dyn Replicate>` for `SnapshotWorld::insert_component` (`snapshot_world.rs:193`). **No `SnapshotReaderRegistry` lift needed.** (The bevy adapter MAY keep its registry-based `&World` assembler as a perf fast-path — measured choice, not a correctness requirement; see §2h M3.) **(per §2h M2):** the core assembler must **skip-on-unregistered-kind** (match the registry's `continue` at `snapshot_builder.rs:82-88`), because `component_of_kind` itself `panic!`s on an unregistered kind — iterate `needed_*_entries()` and skip rather than panic.
5. Send-job:
   - **Pipelined (Worker):** `prepare_send_job(&snapshot)` (`:254`) captures frozen `DiffMask`s + clears live masks at the freeze point → `snapshot.attach_send_plan(plan)` → `snapshot_sender.send(snapshot)`. The send worker drains the slot and transmits **next tick** (the one-tick lag — MISSION_TICK_FLOOR Lever 3).
   - **Resident / deterministic oracle:** `send_all_packets(snapshot)` (`:245`) inline; no slot, no lag.
6. `unpark_workers()` — closes the window.

#### World model (RESOLVED per §2h H3 — core is single-world)

Core `receive` returns the events as data; it mutates only the entity world:
```rust
pub fn receive<W: WorldMutType<E>>(&mut self, entity_world: &mut W) -> ReceiveOutput<E>;
pub fn send<W: WorldRefType<E> + Sync>(&mut self, entity_world: &W);
```
The only world naia core mutates on recv is `entity_world` (spawn/despawn/component-apply). Connection-scoped events (Connect/Disconnect/Tick/Message/Request/Auth) and entity-scoped events are **returned in `ReceiveOutput`** — they touch no world in core. cyberlith's apparent "two worlds" (`apply_receive_output.rs:483-492`) is the **bevy adapter** fanning the returned events into bevy `Messages<X>` across main (connection-scoped) and sim (entity-scoped). That routing is adapter logic over `ReceiveOutput`; the core entity-op application targets the single `entity_world`. No composite-world contract in core.

#### Open questions for sign-off

1. Threading move: confirm relocating the worker runtime from `plugin_full.rs` into naia-server core (owned by `PipelinedServer`) is in-scope for G7 (it's required for a self-contained core bracket). — **CONFIRMED in-scope (Connor 2026-06-29).**
2. Snapshot assembler: core `WorldRefType`-based build vs lifting `SnapshotReaderRegistry` to core. — **RESOLVED (item 4): pure `WorldRefType` + `copy_to_box`, no registry lift.**
3. Unified core server enum: defer to post-G9 (not G7). — **CONFIRMED deferred (Connor 2026-06-29).**

### 2h. Adversarial audit — findings & resolutions (2026-06-29, verdict: NEEDS-REVISION)

A hostile audit (citations independently re-verified) found the factual layer solid but **§2e and §2f/§2g design not sign-off-ready**. Resolutions below; the affected sections are corrected in place and tagged "(corrected per §2h)".

- **C1 (CRITICAL — CONFIRMED → RESOLVED) — §2e panic arms break the moat.** `broadcast_desync_snapshots` calls `send.send_message_to_user::<DesyncDetectionChannel, WorldSnapshotRecord>` **inside the park window** (`server_access.rs:820`, called from `do_park_window_tick:1664`, gated `#[cfg(feature="desync_detection")]` — the moat build). §2e routed `send_message`/`broadcast_message` to `panic!` "unreachable in practice" — FALSE (verified directly). **Resolution (Connor 2026-06-29):** NO panic arms. Message-send queues into an internal `pending_outbound_messages` buffer drained during `send` — works from any system. §2e fully rewritten to the internal-queueing model.
- **C2 (CRITICAL — CONFIRMED → RESOLVED) — `entity_take_authority` panic arm contradicts editor cells.** `drain_sim_editor_ops` calls `ws.entity_take_authority(...)` via `run_with_naia_server` reassembly **inside the park window** (`server_access.rs:85`), for delegated/editor cells. **Resolution (Connor 2026-06-29): editor/delegated path is IN SCOPE.** Authority ops queue like every other mutation (§2e), drained at the editor-ops phase — **G5b** confirmed in-scope.
- **H1 (HIGH — UNVERIFIED-RISK → RESOLVED by G9pre spike, see §2i) — Resident≡Pipelined byte-identity is an unproven linchpin.** Today's deterministic oracle is pipeline-split + inline send (trimmed `SnapshotWorld`), NOT resident (full-world serialize via a different driver, `server.rs:364`). **Resolution (Connor 2026-06-29):** byte-identity confirmed a **hard G9 prerequisite spike (G9pre)**. The "deterministic oracle" = an execution MODE used by the determinism/desync harness, not a test suite: if G9pre passes, the harness runs `ServerMode::Resident` (inherently synchronous → deterministic); if it fails, retain a **workers-off / synchronous-send mode of the Pipelined runtime** (today's `deterministic` cfg behavior, promoted to a first-class `PipelinedServer` config). Either way a mode, not a suite.
- **H2 (HIGH — CONFIRMED gap) — the bracket drops host-sync.** `drain_sim_host_sync_pipelined` (`server_access.rs:275`) reassembles `WorldServer` to bridge bevy change-detection → naia replication config; it's generic mechanism but appears nowhere in §2g, yet §6 forbids reassembly post-G10. **Resolution:** add host-sync placement to §2g (new **G6b**: coord/window-safe host-sync drain), OR prove every `HostSyncEvent` producer is retired by explicit G4/G5 `spawn_replicated`/`enable_replication`. Note: `drain_sim_host_sync_pipelined` still runs against the **Sim** world despite the `#21 P4` "main-world host-sync retired" claim — verify.
- **H3 (HIGH — LIKELY-FLAW → RESOLVED) — single-world `receive`/`send`.** Traced (`apply_receive_output.rs:483-492`): the "two worlds" are NOT two entity worlds — they are **one entity world + event routing**. `entity_world` gets entity-scoped *events*; `world` (coordinator) gets connection-scoped events. The only thing naia **core mutates** is the entity world (spawn/despawn/component-apply). Everything else is `ReceiveOutput` returned as DATA. **Resolution:** core `receive<W: WorldMutType<E>>(&mut self, entity_world: &mut W) -> ReceiveOutput<E>` is genuinely single-world; the two-world *event fan-out* (writing bevy `Messages<X>` into main vs sim) is the bevy adapter routing the returned `ReceiveOutput` — adapter concern, not core. No composite-world contract needed. §2g world-model rewritten.
- **M1 (MEDIUM) — "almost all generic" over-claims; needs a step ledger.** **Resolution:** §2g gains a line-by-line ledger of all ~10 `do_park_window_tick` steps → {core bracket | G4/G5/G6 op | consumer policy}, so nothing falls through (resource/lifecycle/scope-delta drains were unplaced).
- **M2 (MEDIUM) — assembler panic-vs-skip divergence.** Confirmed the assembler CHAIN is sound (the bevy registry itself just calls `copy_to_box` — `snapshot_reader_registry.rs:64`), but `component_of_kind` panics on an unregistered kind whereas the registry path skips. **Resolution:** core assembler must **skip-on-unregistered** to match. Noted in §2g.
- **M3 (MEDIUM) — "perf preserved by construction" ≠ moat-guaranteed.** The moat proves correctness, not speed; the `WorldRefType` assembler (HashMap + dyn dispatch + `copy_to_box`) may be slower than the registry's typed `get::<C>()`. **Resolution:** §6 gains a **numeric `bench_profile` per-phase gate** at G10 (not just "spans match"); decide up front whether the bevy adapter keeps its registry fast-path (if so, that's an acknowledged exception to "fully unified").
- **M4 (MEDIUM) — worker-move understates lifecycle entanglement.** Threading move is feasible (no real bevy coupling in the loops; `TestClock` is just a re-export), but the Armed→Running spawn lifecycle is tangled with bevy `Startup`/`Resource` (`plugin_full.rs:609`). **Resolution:** G7 must spec **core ownership of the Armed→Running / listen-timing state transitions**, not just thread spawn.
- **L1 (LOW) — sequencing.** G7→G10 are NOT all independently green: editor/desync paths have no valid Pipelined surface until §2e is corrected, so G10 can't compile-pass those features mid-sequence. **Resolution:** mark the atomic-only steps in §2d/§3.

**What the audit confirmed RIGHT:** the snapshot-assembler correctness (no registry lift — both paths funnel through `copy_to_box`), the one-tick-lag/freeze-point ordering, the "no hooks/closures/traits" shape matching naia's resident loop, and every `file:line` citation.

### 2i. G9pre spike — RESULT (2026-06-29, GREEN)

**Spike:** `test/harness/contract_tests/integration_only/g9pre_resident_pipelined_byte_identity.rs` (registered in `test/harness/Cargo.toml`). Two independent `Scenario`s (each resets the thread-local `TestClock` to 0 ⇒ byte-identical packet history through handshake + spawn) are stepped in lockstep through identical setup, trace-captured AFTER the spawn settles, then driven one mutation tick:
- **Resident** = `Server::send_all_packets(&live)` (= `prepare_send_job(&live)` + `transmit_send_job(live)`, same tick, live world — `world_server.rs:1018-1024`).
- **Pipelined** = `prepare_send_job(&live)` at the freeze point (captures the frozen `DiffMask`, clears the live mask) + `transmit_send_job(snapshot)` reading a `SnapshotWorld` (Lever-3 lagged path, driven synchronously via `Scenario::transmit_and_pump`).

**Result — server→client wire bytes byte-identical (envelope + payload) across every diff-mask case:**

| case | resident | pipelined | bytes |
|------|----------|-----------|-------|
| full mask (both props) | 1 pkt, 24B | 1 pkt, 24B | ✓ identical |
| partial mask (only x) | 1 pkt, 20B | 1 pkt, 20B | ✓ identical |
| partial mask (only y) | 1 pkt, 20B | 1 pkt, 20B | ✓ identical |
| multi-entity, mixed masks | 1 pkt, 30B | 1 pkt, 30B | ✓ identical |

Partial-mask cases shrink to 20B (only the dirty property serializes) and both modes agree exactly — i.e. the **frozen** mask gates serialization identically to the **live** mask.

**What this proves (the real unknown, now retired):** for a given `(world-view, DiffMask)`, the pipelined transmit reading a `SnapshotWorld` + a frozen plan serializes **bit-for-bit identically** to the resident transmit reading the live world. The send *content* is a pure function of `(snapshot, plan)` — independent of (a) live-vs-snapshot world source and (b) which thread runs transmit.

**What this does NOT claim:** Resident and Pipelined are NOT whole-*stream* byte-identical under sustained operation — Pipelined deliberately applies the one-tick send *lag* (MISSION_TICK_FLOOR Lever 3), so a given value lands in a later wall-tick's packet (different seq/ack envelope). The lag is a **scheduling shift, not a content change.**

**Refined recommendation (supersedes the "oracle == Resident" framing in §2f/H1):** the determinism/desync oracle should NOT be Resident mode (no lag ⇒ different wall-tick framing than production). It should drive the **Pipelined `receive`/`prepare_send_job`/`transmit_send_job` bracket synchronously (no spawned workers)** — exactly what this spike and the existing `naia_test_harness` already do. Because send content is thread-independent (proven above), production (Pipelined + worker threads) and the oracle (Pipelined + synchronous drive) emit identical bytes by construction. Consequence:
- **No `SendStrategy` knob and no consumer-facing "synchronous mode" are needed.** The worker runtime (G7) is an async *driver* around the same core bracket; the determinism harness simply calls the bracket methods directly. That's a test-driver concern, not a `ServerMode` variant.
- **G9's single knob `ServerMode::{Resident, Pipelined}` stays clean** (Pipelined ⇒ workers in production). The conditional "purity goal depends on G9pre" in the G9 row is resolved: purity holds, because the oracle never needs Resident.
- Spawn / scope-entry / despawn wire paths remain covered by the existing cyberlith desync moat through the real park window; the spike targeted the previously-unproven serialization-equivalence question (live vs snapshot+frozen-mask), which is the load-bearing one for G9.

## 3. Sequence + status

| Step | Description | Status |
|------|-------------|--------|
| G1 | `SimPipeline<E>` + `TickCtx<E,W>` tick-driver; `SimPipelineRes` in bevy adapter; tests green | ✅ COMPLETE (`55272fad`) |
| G2 | `SimPipeline::listen(socket)` startup-window API; `PluginInternalState::listen` delegates to it | ✅ COMPLETE (`1e851a73`) |
| G3a | forwarding methods on `PipelinedServer<E>` for all coord-only ops | ✅ COMPLETE (`175d4bc7` rename + G3a impl) |
| G3b | cyberlith D11 `CellCommandsExt` dies, replaced by direct `pipelined_server.method()` calls in the park window | PENDING (design signed off) |
| G3c | unified `Server` param: add `ServerImpl::Pipelined(PipelinedServer<Entity>)` variant; **NO panic arms** — all mutating methods (incl. `send_message`/`broadcast_message`/`take_authority`) work from any system via internal queueing (§2e); reads return coord state; retire raw `ResMut<PipelinedServer>` access | PENDING (design §2e, internal-queueing model) |
| G4 | `spawn_replicated` fused op | PENDING |
| G5 | `enable_replication_for_existing_entity` | PENDING |
| G5b | **(per §2h C2 — editor/delegated path IN SCOPE, Connor 2026-06-29)** queued `entity_take_authority` (+ related authority ops), drained at the editor-ops phase | PENDING |
| G6 | `Res<R>` resource API (`SimPipeline::insert_resource` etc.) | PENDING |
| G6b | **(per §2h H2)** coord/window-safe host-sync drain (bevy change-detection → replication config), OR proof every `HostSyncEvent` producer is retired by explicit G4/G5 | PENDING |
| G7 | **naia-server core `receive`/`send` bracket** (FIRST) — `PipelinedServer::receive(&mut world)` (park + single recv-drain + apply events) and `::send(&world)` (send-prep + snapshot + send-job + unpark); **+ moves the worker-thread runtime from the bevy adapter into core**. Explicit method sequence; consumer interleaves own code via unified `Server` ops. **No trait, no closures, no hooks.** **Supersedes** the old `with_parked_tick`. **Detailed design: §2g.** | PENDING (design §2g — pending sign-off) |
| G8 | **naia-bevy-server mode-aware system sets** (layered on G7) — pipelined mode makes the existing `ReceivePackets` / `SendPackets` system sets run the parked/worker bracket internally; consumer systems sit between them via plain `add_systems(Update, …)`. **Zero new consumer-facing concepts.** Manages handle transit + consumer-chosen entity world. | PENDING (design §2f — pending sign-off) |
| G9pre | **(per §2h H1) PREREQUISITE SPIKE** — prove pipelined send content ≡ resident serialization byte-for-byte across diff-mask cases. | ✅ COMPLETE/GREEN (§2i) — byte-identical (envelope + payload) across full/partial/multi-entity masks; pipelined transmit content is a pure fn of `(snapshot, plan)`, thread- and world-source-independent |
| G9 | **`ServerMode::{Resident, Pipelined}` — single knob** — Pipelined⇒worker send, Resident⇒synchronous send. Same `receive`/`send` signatures; consumer code unchanged. **Oracle = synchronously-driven Pipelined bracket (NOT Resident)** — per §2i, identical bytes to production by construction; **no `SendStrategy` knob needed**, single-knob purity holds. | PENDING (design §2f — pending sign-off) |
| G10 | **cyberlith cutover** (cyberlith worktree) — delete `open_park_window`/`do_park_window_tick`/`close_park_window` + the `drain_sim_*` glue; re-express as ordinary systems around naia's `ReceivePackets`/`SendPackets` sets; `cell.update()` collapses to naia's bracket. **ATOMIC-only (per §2h L1): editor/desync Pipelined surface must exist (G3c-corrected + G5b + G6b) before this compiles green.** **Gate: determinism/desync moat byte-exact-green + numeric `bench_profile` per-phase parity.** | PENDING (design §2f — pending sign-off) |
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

- **Correctness (G10 gate):** cyberlith determinism/desync moat stays byte-exact-green through the cutover — same single-park / freeze-point send-prep / one-tick-lag sequence.
- **Perf (G10 gate, per §2h M3 — SEPARATE from correctness):** the moat proves correctness, NOT speed. Add a **numeric `bench_profile` per-phase gate**: `pw::s6_snapshot_build` and total `cell::update` must stay within a bounded delta of pre-cutover (no new barriers, no double-park, no snapshot-assembler regression from the `WorldRefType` path vs the registry path). Decide up front whether the bevy adapter keeps its registry fast-path (acknowledged exception to "fully unified") or accepts the generic path's cost.
- `server_access.rs` (until deleted at G10): zero `WorldServer` reassembly, zero handle `.take()`, zero `HostSyncEvent` construction. At G10 the file's park-window machinery is gone entirely.
- naia-primitive tripwire (cybertool check) green with empty/justified allowlist.
- naia sim-namako specs cover G1–G6 behavior (incl. resource remove→re-insert).
- cyberlith determinism moat green; naia-isolation green; native + wasm32 build.
