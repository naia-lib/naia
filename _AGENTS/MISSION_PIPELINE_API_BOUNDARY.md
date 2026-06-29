---
title: "MISSION — naia pipelined-sim consumer API + boundary restoration"
status: G7 SIGNED OFF + IMPL UNDERWAY (Connor 2026-06-29). G7-1 ✅ (core registry-free `SendStateView::build_needed_snapshot` assembler + N4 byte-identity tests GREEN — `g9pre_core_assembler_*`). G7-2 ✅ (`PipelinedServer::{receive,send}` bracket + the N1 D0–D9 `drain_and_send` ordering contract; structural drive GREEN — `pipeline_bracket`). G7-3 ✅ (worker-thread runtime + park/unpark barrier + worker loops + Armed→Running→Stopped lifecycle MOVED into core `pipeline_actors::PipelineRuntime<E>`; bevy `PluginInternalState` now a thin delegating wrapper; `workers_active` cfg + `deterministic` feature + `build.rs` moved to naia-server; bench timing via fn-ptr hooks; GREEN — naia-server lib 42, full naia harness ~150, bevy adapter deterministic suite incl. cross-thread panic-propagation). Prior: two adversarial audits incorporated (§2h, §2j); G9pre §2i. OPEN before G10: G6b host-sync (framework-agnosticism fork).
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

The original panic-arm design was wrong. The pristine model: **every mutating unified-`Server` method is available in Pipelined mode from any system at any time** — it does not require the workers to be parked. Methods that can't apply immediately **queue into internal lock-protected buffers** and drain at the correct phase inside `receive`/`send`. This extends naia's *existing* deferred-queue pattern — `scope_change_queue`, `pending_world_hooks`, `pending_disconnect_requests` (`server/src/server/server_shared.rs:116/192/200`, each a `Mutex`) — to the remaining mutating ops:

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

`ServerMode::{Resident, Pipelined}` is the single **consumer-facing** knob. **There is NO separate `SendStrategy`** — `Pipelined` *implies* worker-thread send; `Resident` *implies* synchronous live-world send. **Switching resident↔pipelined changes nothing in the consumer's code** — `receive`/`send` keep the same signatures; only what they do internally differs. **G3a + G3c are the load-bearing substrate**: because the consumer's interleaved code calls the unified `Server` op surface, the *identical* code runs against either the resident `WorldServer` or the pipelined handles.

> **The determinism/desync oracle is NOT `Resident` mode (UPDATED per G9pre §2i, supersedes the earlier "oracle == Resident" framing).** Resident reads the LIVE world and carries **no one-tick send lag**, so its wall-tick framing differs from `Pipelined` production — running the oracle as Resident would validate a different schedule than ships. Instead the oracle drives the **`Pipelined` `receive`/`prepare_send_job`/`transmit_send_job` bracket synchronously (no spawned workers)** — exactly what `naia_test_harness` already does. The G9pre spike (§2i) proves the pipelined transmit content is a pure function of `(snapshot, plan)` — thread-independent and freeze-isolated — so the synchronous oracle drive emits **bytes identical to worker-driven production by construction**. The worker runtime (G7) is an async *driver* around the same core bracket; calling the bracket directly is a test-driver concern, not a `ServerMode` variant. This retires cyberlith's `#[cfg(feature = "deterministic")]` send fork without needing Resident. **Net: the single-knob purity goal holds** — no `SendStrategy`, no consumer-facing "synchronous" mode.

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
    pub fn receive<W: WorldMutType<E>>(&mut self, world: W) -> ReceiveOutput<E>;
    // PIPELINED: send-prep + snapshot build + send-job publish + unpark_workers().
    pub fn send<W: WorldRefType<E> + Sync>(&mut self, world: &W);
}
```

> **IMPLEMENTED (G7-2, 2026-06-29).** Both methods land on `PipelinedServer<E>`
> (`server/src/pipeline_actors/sim_pipeline.rs`). `send` delegates the load-bearing
> order to a private `drain_and_send` implementing the D0–D9 contract below (D1–D7
> are documented no-op stubs until their queues exist; D8/D9 live). **Signature
> accuracy fix:** `receive` takes `world` **by value** (`W: WorldMutType<E>`), not
> `&mut W` — `apply_recv_to_world` consumes the world proxy by value (naia's
> proxy-per-call idiom; a proxy is not `&mut`-reusable). At G7-2 the bracket runs
> the synchronous/oracle shape (transmit inline); the worker-driven production
> shape (publish to send slot, worker transmits next tick) lands with the runtime
> in G7-3. Coverage: `pipeline_bracket` (structural drive, zero-client) + the
> g9pre byte-identity suite (the send primitives the bracket composes).

The "single knob, consumer code unchanged" property is realized at the bevy layer by `ServerImpl` (G3c). A unified *core* server type (`Resident | Pipelined` enum) is OPTIONAL future ergonomics — NOT in G7. For a core binary, mode is the constructor you call; the interleaved op code is textually identical because both types expose the same op surface (G3a gave `PipelinedServer` the coord ops; `Server<E>` already has them).

#### Threading ownership move (the substantive G7 work)

Today the worker threads + park/unpark barrier + worker loops live in the **bevy adapter** (`plugin_full.rs`: `park_workers`/`unpark_workers`/`spawn` at `:649/:718/:559-607`). For `receive`/`send` to be self-contained and framework-agnostic, this runtime moves **into naia-server core**, owned by `PipelinedServer<E>` (or a `PipelineRuntime` it holds): thread spawn, the parked-count barrier, the recv/send worker loops, and the `SnapshotSender`/`SnapshotReceiver` wiring (the slot type is already core — `pipeline_actors/snapshot_sender.rs`). Uses only `std::thread` + channels — no bevy. The bevy adapter then *calls* `receive`/`send` from the `ReceivePackets`/`SendPackets` sets instead of hand-rolling the window.

> **IMPLEMENTED (G7-3, 2026-06-29).** Landed as `pipeline_actors::PipelineRuntime<E>`
> (`server/src/pipeline_actors/runtime.rs`): `ParkControl` (the parked-count
> condvar barrier), `PanicSlot`, `WorkerHandle`, the `RuntimeState`
> (Armed→Running→Stopped) lifecycle, `worker_park_checkpoint`, and the
> `recv_worker_loop`/`send_worker_loop` — all ported verbatim from the adapter and
> generalized over `E`. Entry points: `new_armed(recv_slot, send_slot,
> snapshot_receiver, timing)` → `spawn_workers(recv_readiness)` →
> `park_workers`/`unpark_workers`/`propagate_panic_if_any`; `Drop` joins the
> workers (5s soft-deadline). The bevy `PluginInternalState` keeps ONLY the
> bevy-coupled wiring (`armed_pipeline`, `sim_event_receiver`,
> `_snapshot_sender_keep`, the two slot `Arc`s, and a `runtime` field) and
> delegates every lifecycle call. The event fan-out (`drain_recv_impl*`) stays in
> the adapter (it writes bevy `Messages<X>`); it now reads the recv channel via
> `runtime.recv_out_receiver()`.
>
> **Resolved sub-decisions** (none needed a design change): (1) the
> `workers_active = not(deterministic)` cfg + the `deterministic` feature + the
> `build.rs` that derives it moved to **naia-server**; the adapter's
> `deterministic` feature now forwards to `naia-server/deterministic`, and the
> adapter's vestigial `build.rs`/`smol`/`crossbeam-channel` deps were removed.
> (2) `TestClock` is called directly as `naia_shared::TestClock` (it lives in
> `naia-socket-shared`, re-exported by `naia-shared`) — no bevy. (3) the bench
> `pipeline_timing` aggregator stays adapter-side and is wired into the core
> runtime via `RuntimeTimingHooks` (`Option<fn(u64)>` per stage — recv/send/
> barrier), so the core stays instrumentation-crate-free with zero overhead when
> the feature is off. The byte-exact determinism path is unchanged: the parked
> (not(workers_active)) worker loops are byte-identical to the pre-move code.

> **G7-3 de-risk (VERIFIED 2026-06-29):** the move has **no real bevy coupling**.
> The worker closures' only adapter-looking dependency is
> `naia_bevy_shared::TestClock::{install_shared,shareable_handle,detach_shared}` —
> but `TestClock` is defined in **`naia-shared`** (`shared/src/backends/test_time/timestamp.rs`)
> and merely *re-exported* by `naia_bevy_shared` (M4 confirmed). Core calls it
> directly under `feature = "test_time"`. `bevy_ecs::World` appears only in the
> **main-side** `drain_recv_impl*` (which stays in the adapter — it fans events
> into bevy `Messages`), NOT in the worker loops. `naia-server` already depends on
> `smol` + `parking_lot`; the move adds only `crossbeam_channel` (the
> `ReceiveOutput` hand-off). The `workers_active` / `pipeline_timing` cfg flags
> and the `Entity`→`E` generalization come along mechanically. **Approach is
> clean; the risk is execution-fidelity (determinism gate), not architecture.**

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

#### Drain-phase ordering contract (per audit N1 — THE load-bearing total order)

§2e lets the consumer queue mutations "from any system, any time," draining "at their defined phase." That ergonomics win **relocates the byte-exactness-critical part — the total ORDER in which queued drains apply — out of cyberlith's explicit, hand-commented `do_park_window_tick` and into naia.** That order is NOT incidental; cyberlith's code flags it as load-bearing (`server_access.rs:1655` "Runs BEFORE host-sync"; `:1657` "editor naia ops BEFORE host-sync (take-authority precedes the reject/replace despawn's replication)"; `:1691-1706` Step 7.5 "Order is load-bearing"). **naia core MUST define this total order as a first-class contract — a single `pub` drain sequence, not an emergent property "preserved by the moat."** The moat *detects* a regression; the contract *prevents* one.

The canonical order naia owns (between `receive` and the snapshot build), distilled from `do_park_window_tick` Step 5 → Step 7.5 (`server_access.rs:1647-1707`):

| # | drain | source queue | why this position |
|---|---|---|---|
| D0 | recv events → entity world | `RecvHandle::receive()` | `receive` body (before any consumer code) |
| — | *(consumer code: sim update, scope intent — interleaved, calls op surface)* | — | between `receive` and `send` |
| D1 | entity-replication registrations (`spawn_replicated`/`enable_replication`) | scope/lifecycle queue | before resource & host-sync (new entities must exist before scope/host-sync references them) |
| D2 | resource-replication registrations (`Res<R>` carriers) | resource queue | **before host-sync** (carrier entities must exist first — `:1655`) |
| D3 | lifecycle (despawn/insert/remove) | lifecycle queue | after spawns, before authority/host-sync |
| D4 | authority/editor ops (`take_authority`) | authority queue (G5b) | **before host-sync** — take-authority must precede the reject/replace despawn's replication (`:1657`) |
| D5 | host-sync (change-detection → repl config) | host-sync queue (G6b) | after all entity/authority mutations settle |
| D6 | outbound messages (`send_message`/`broadcast_message`, incl. desync snapshot) | `pending_outbound_messages` (§2e) | after world state final, before send-prep packs them |
| D7 | scope-ledger writes (`ScopeToggled` enqueue) | `scope_change_queue` | before send-prep (scope intent must be visible to preamble) |
| D8 | send-prep: `apply_pending_send_preamble` → `apply_pending_scope_changes(world)` → `refresh_needed_entities` | per-conn queues | **strict sub-order** (`:1691-1706`) — preamble drains room/configure; scope-changes queues spawns into per-user outbound; refresh recomputes the needed-set FROM those — must run in this order or the snapshot trims wrong |
| D9 | snapshot build + send-job (prepare/transmit) | — | reads the now-final needed-set |

This table is the G7 acceptance contract. Implementation: a single core method (e.g. `PipelinedServer::drain_and_send`, called by `send`) that executes D1–D9 in this exact order; `receive` owns D0. Each step is a no-op early-return when its queue is empty (matches cyberlith). **A naia-internal `debug_assert` should pin the ordering invariants that have a checkable precondition** (e.g. D8's scope-enter-for-absent-entity assert already exists — `MISSION_SNAPSHOT_DIRTY_TRIM.md §4`; add one that authority drains before lifecycle-despawn replication). Any future drain (new op class) MUST be slotted explicitly into this table, not appended ad hoc.

#### `send` internal sequence (the heart of the question)

Mapped to verified `SendHandle`/`SendState` methods, preserving cyberlith's load-bearing order:

1. `apply_pending_send_preamble()` (`pipeline_handles.rs:293`) — drain room changes / configure-repl; flush handshake + heartbeats.
2. `apply_pending_scope_changes(world)` (`:336`) — publish freshly-scoped entities into per-user send connections. Needs `WorldRefType`.
3. `refresh_needed_entities()` (`:303`) — recompute the cross-thread needed-set.
4. **Build the `SnapshotWorld<E>`** from `world` + `SendStateView::needed_live_and_snapshot_entries()` (`send_state_view.rs`) — a core, `WorldRefType<E>`-based assembler generalizing the bevy `build_snapshot` (`snapshot_builder.rs:45`). The trim is naia-internal; the consumer never authors a snapshot. **RESOLVED (2026-06-29, traced):** the assembler closes entirely on `WorldRefType<E>` — `world.component_of_kind(&e,&kind)` (`world_type.rs:39`) → `ReplicaDynRefWrapper` derefs to `&dyn Replicate` (`replica_ref.rs:154`) → `.copy_to_box()` (`replicate.rs:96`) → `Box<dyn Replicate>` for `SnapshotWorld::insert_component` (`snapshot_world.rs:193`). **No `SnapshotReaderRegistry` lift needed.** (The bevy adapter MAY keep its registry-based `&World` assembler as a perf fast-path — measured choice, not a correctness requirement; see §2h M3.) **(per §2h M2):** the core assembler must **skip-on-unregistered-kind** (match the registry's `continue` at `snapshot_builder.rs:82-88`), because `component_of_kind` itself `panic!`s on an unregistered kind — iterate `needed_*_entries()` and skip rather than panic.
5. Send-job — **three distinct execution shapes** (do not conflate; per audit N5):
   - **Pipelined-production (worker):** `prepare_send_job(&snapshot)` (`:254`) captures frozen `DiffMask`s + clears live masks at the freeze point → `snapshot.attach_send_plan(plan)` → `snapshot_sender.send(snapshot)`. The send worker drains the slot and transmits **next tick** (the one-tick lag — MISSION_TICK_FLOOR Lever 3).
   - **Pipelined-oracle (synchronous):** identical `prepare_send_job(&snapshot)` + `transmit_send_job(snapshot, plan)` driven **inline on the calling thread** — same snapshot, same frozen plan, same one-tick lag, just no worker thread. Byte-identical to production (G9pre §2i). This is the determinism/desync oracle.
   - **Resident:** `send_all_packets(&live_world)` (`server.rs:364` → `world_server.rs:1018`) inline against the **LIVE** world — no snapshot, no slot, **no lag**. A separate production mode; NOT the oracle (it has different wall-tick framing — see §2f).
6. `unpark_workers()` — closes the window (pipelined modes only).

#### World model (RESOLVED per §2h H3 — core is single-world)

Core `receive` returns the events as data; it mutates only the entity world:
```rust
pub fn receive<W: WorldMutType<E>>(&mut self, entity_world: &mut W) -> ReceiveOutput<E>;
pub fn send<W: WorldRefType<E> + Sync>(&mut self, entity_world: &W);
```
The only world naia core mutates on recv is `entity_world` (spawn/despawn/component-apply). Connection-scoped events (Connect/Disconnect/Tick/Message/Request/Auth) and entity-scoped events are **returned in `ReceiveOutput`** — they touch no world in core. cyberlith's apparent "two worlds" (`apply_receive_output.rs:483-492`) is the **bevy adapter** fanning the returned events into bevy `Messages<X>` across main (connection-scoped) and sim (entity-scoped). That routing is adapter logic over `ReceiveOutput`; the core entity-op application targets the single `entity_world`. No composite-world contract in core.

#### Deliberate non-goals (per audit N6 — don't over-claim future-proofing)

The design is **generic over `E`** (instantiated at `Entity` in the bevy adapter, `ServerImpl::Pipelined(PipelinedServer<Entity>)`) but makes two bounded assumptions that are scope, not oversight:
- **One entity world per `PipelinedServer`.** A future consumer wanting multiple *replicated* entity worlds (e.g. per-region shards each with its own connection/needed-set) is NOT served by a single `PipelinedServer` — that would be a new composition above this layer, not a tweak. "Future-proof" here means *clean to extend*, NOT *already multi-world*.
- **Single send pipeline / one needed-set per server.** The trim + per-user connection set is server-global.

These are fine for cyberlith (one cell = one world) and for the foreseeable roadmap (MatchState/Team are components in the one world). Flagged so the spec doesn't claim a generality it doesn't deliver; revisit only if sharded worlds become a real requirement.

#### Open questions for sign-off

1. Threading move: confirm relocating the worker runtime from `plugin_full.rs` into naia-server core (owned by `PipelinedServer`) is in-scope for G7 (it's required for a self-contained core bracket). — **CONFIRMED in-scope (Connor 2026-06-29).**
2. Snapshot assembler: core `WorldRefType`-based build vs lifting `SnapshotReaderRegistry` to core. — **RESOLVED (item 4): pure `WorldRefType` + `copy_to_box`, no registry lift.**
3. Unified core server enum: defer to post-G9 (not G7). — **CONFIRMED deferred (Connor 2026-06-29).**

### 2h. Adversarial audit — findings & resolutions (2026-06-29, verdict: NEEDS-REVISION)

A hostile audit (citations independently re-verified) found the factual layer solid but **§2e and §2f/§2g design not sign-off-ready**. Resolutions below; the affected sections are corrected in place and tagged "(corrected per §2h)".

- **C1 (CRITICAL — CONFIRMED → RESOLVED) — §2e panic arms break the moat.** `broadcast_desync_snapshots` calls `send.send_message_to_user::<DesyncDetectionChannel, WorldSnapshotRecord>` **inside the park window** (`server_access.rs:820`, called from `do_park_window_tick:1664`, gated `#[cfg(feature="desync_detection")]` — the moat build). §2e routed `send_message`/`broadcast_message` to `panic!` "unreachable in practice" — FALSE (verified directly). **Resolution (Connor 2026-06-29):** NO panic arms. Message-send queues into an internal `pending_outbound_messages` buffer drained during `send` — works from any system. §2e fully rewritten to the internal-queueing model.
- **C2 (CRITICAL — CONFIRMED → RESOLVED) — `entity_take_authority` panic arm contradicts editor cells.** `drain_sim_editor_ops` (`server_access.rs:1085`) calls `ws.entity_take_authority(...)` (`:1146`) via `run_with_naia_server` reassembly **inside the park window**, for delegated/editor cells. **Resolution (Connor 2026-06-29): editor/delegated path is IN SCOPE.** Authority ops queue like every other mutation (§2e), drained at the editor-ops phase — **G5b** confirmed in-scope.
- **H1 (HIGH — UNVERIFIED-RISK → RESOLVED by G9pre spike, see §2i) — Resident≡Pipelined byte-identity is an unproven linchpin.** Today's deterministic oracle is pipeline-split + inline send (trimmed `SnapshotWorld`), NOT resident (full-world serialize via a different driver, `server.rs:364`). **Resolution (Connor 2026-06-29):** byte-identity confirmed a **hard G9 prerequisite spike (G9pre)**. The "deterministic oracle" = an execution MODE used by the determinism/desync harness, not a test suite: if G9pre passes, the harness runs `ServerMode::Resident` (inherently synchronous → deterministic); if it fails, retain a **workers-off / synchronous-send mode of the Pipelined runtime** (today's `deterministic` cfg behavior, promoted to a first-class `PipelinedServer` config). Either way a mode, not a suite.
- **H2 (HIGH — CONFIRMED gap) — the bracket drops host-sync.** `drain_sim_host_sync_pipelined` (`server_access.rs:275`) reassembles `WorldServer` to bridge bevy change-detection → naia replication config; it's generic mechanism but appears nowhere in §2g, yet §6 forbids reassembly post-G10. **Resolution (UPDATED per audit N3 — this is OPEN, not resolved):** host-sync gets its own design pass (**G6b**, demoted to OPEN in §3). The change-detection→repl-config bridge is the least framework-agnostic mechanism in the mission and the design fork (bevy-adapter-only vs core; survives no-reassembly?; producers retired by G4/G5?) genuinely gates G10. VERIFIED: `drain_sim_host_sync_pipelined` (`server_access.rs:275-320`) runs against the **Sim** world (`sub_apps.sub_apps.get_mut(&SimApp...)` :293-297), consistent with the `#21 P4` note that only the **main-world** host-sync was retired (`:262-266`) — the Sim-world bridge remains live.
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

**Scope of the proof — what the spike does and does NOT establish (per audit N4, honest accounting):**
- ✅ **Serialization equivalence:** `transmit(snapshot, frozen_plan)` == `transmit(live, live_mask)` when the snapshot carries the frozen values — across full / partial-x / partial-y / multi-entity masks (`g9pre_resident_pipelined_byte_identity`).
- ✅ **Freeze isolation (added 2026-06-29):** after `prepare_send_job`, a concurrent post-freeze live mutation to a *different* value does NOT leak onto the wire — the lagged transmit emits the FROZEN value, byte-identical to a resident send of it (`g9pre_freeze_isolates_transmit_from_concurrent_mutation`). This is the property that actually justifies the snapshot+lag (worker transmits tick N while MAIN advances to N+1).
- ✅ **Production needed-set assembler (DISCHARGED 2026-06-29, G7 step 1):** the core registry-free `WorldRefType` assembler now exists — `SendStateView::build_needed_snapshot` / `build_full_snapshot` (`server/src/pipeline_actors/send_state_view.rs`), closing on `component_of_kind → copy_to_box → insert_component` with skip-on-absent (no panic, §2h M2). Two acceptance tests added that drive the **REAL** assembler against the resident baseline: `g9pre_core_assembler_byte_identity` (all four diff-mask cases) and `g9pre_core_assembler_freeze_isolation` (assembler + concurrent post-freeze live mutation). **All four g9pre tests GREEN.** This was the last open §2i/N4 G7 obligation; the assembler is now empirically byte-identical, not merely asserted. (The bevy adapter's registry-based `build_snapshot` remains as a measured perf fast-path per §2h M3.)

### 2j. Second adversarial audit — findings & resolutions (2026-06-29, verdict: NEEDS-REVISION → all addressed)

A hostile second pass (citations independently re-verified) ran after §2i. It confirmed the §2h resolutions are sound and the citations spot-check accurate, but surfaced one real architectural gap and several over-claims. All seven addressed in place:

- **N1 (CRITICAL → RESOLVED) — bracket bodies omitted the queued-mutation drain ORDER.** §2e relocated drain ordering from cyberlith's hand-commented `do_park_window_tick` into naia, but no total order was written down ("drained at their defined phase" with no phase defined). The order IS load-bearing (`server_access.rs:1655` "before host-sync"; `:1657` take-authority before despawn-replication; `:1691-1706` send-prep sub-order). **Resolution:** added the **D0–D9 drain-phase ordering contract** (§2g) — a single enumerated total order implemented as one ordered core method with debug-asserts, a G7 acceptance contract. This was the spec's missing core deliverable.
- **N2 (HIGH → RESOLVED) — §2f still said "oracle runs in Resident," contradicting §2i.** Rewrote §2f's two-modes paragraph to the §2i conclusion (oracle = synchronously-driven Pipelined bracket; Resident is a distinct production mode).
- **N3 (HIGH → RESOLVED) — H2 host-sync marked RESOLVED was actually an open OR.** Demoted **G6b to OPEN** (own design pass); it's the least framework-agnostic mechanism and genuinely gates G10. Status header no longer claims "all blockers resolved."
- **N4 (HIGH → RESOLVED) — G9pre spike proved a near-tautology.** Correct: the original spike built the snapshot from the post-mutation live world with no mutation between freeze and transmit. **Resolution:** (a) added `g9pre_freeze_isolates_transmit_from_concurrent_mutation` proving the freeze isolates the transmit from a concurrent post-freeze live mutation (GREEN); (b) honestly scoped §2i — the production needed-set assembler is NOT yet exercised (it's G7 code), made an explicit G7 byte-identity acceptance obligation.
- **N5 (MEDIUM → RESOLVED) — §2g send item 5 conflated Resident with the snapshot-reading oracle.** Split into three distinct execution shapes: Pipelined-production (worker), Pipelined-oracle (synchronous, snapshot, lag), Resident (live, no lag).
- **N6 (MEDIUM → RESOLVED) — "single knob" baked in single-world / `E=Entity`.** Added a **deliberate non-goals** subsection (one entity world per `PipelinedServer`; server-global needed-set) — generic over `E` but not multi-world; "future-proof" = clean-to-extend, not already-general.
- **N7 (LOW → RESOLVED) — two stale citations.** Fixed: existing queues are in `server_shared.rs:116/192/200` (not `handles.rs`); C2's `entity_take_authority` is at `server_access.rs:1146` inside `drain_sim_editor_ops:1085` (not `:85`).

**What audit-2 confirmed RIGHT (do not regress):** the registry-free snapshot-assembler chain (`world_type.rs:39` → `replica_ref.rs:154` → `replicate.rs:96`); the M2 panic-vs-skip diagnosis (`world_proxy.rs:591` `panic!`); the C1/C2 panic-arm reversal (`broadcast_desync_snapshots:820`, `entity_take_authority:1146` both genuinely in-window); the H3 single-world resolution; all cited naia method signatures; the no-hooks/closures/traits explicit-method-sequence shape matching the resident loop.

### 2k. Third adversarial audit — post-G7-impl (2026-06-29, verdict: NEEDS-REVISION → all addressed)

A hostile pass ran against the **landed** G7-1/G7-2 code (not the design). It confirmed the D8/D9 ordering is faithful to cyberlith Step 7.5/6/6.6 and refuted several suspected bugs (`copy_to_box`/mutator-state, snapshot/freeze ordering, the `is_empty` handshake gate — `received_addresses.insert` runs for every packet, generic-over-`E`). Two real defects + three minor; all resolved on `feature/pipeline-api`:

- **A1 (HIGH → RESOLVED) — `drain_and_send` omitted `drain_all_acks`.** Both reference send paths drain the cross-half ACK channel before transmit (resident `SendState::send_all_packets` `send_state.rs:625`; active send worker `plugin_full.rs:1266`); the bracket did not. Effect: acked `sent_updates` never trimmed → endless retransmits + a needed-set that never clears → divergence from resident the instant a client acks. **Resolution:** `send.drain_all_acks()` is now the FIRST action of the D8 sub-order, documented as non-optional and distinct from the preamble (which flushes/heartbeats/empty-acks but does NOT consume the inbound ACK channel). Real-ack byte-identity assertion is a tracked **G8** obligation (needs replication-through-bracket + a `PipelinedServer`-vs-resident harness); recorded in `pipeline_bracket.rs` "known coverage gap", not silently dropped.
- **A2 (MEDIUM → RESOLVED) — core assembler could panic on an unregistered kind; doc claim false.** The bevy *assembler* skips unregistered kinds (reader-registry `None`), but the core assembler's `WorldRefType::component_of_kind` on the bevy world **panics** (`world_proxy.rs:591`) — so "matches the bevy assembler's continue arm" was untrue about the mechanism (M2 was only half-fixed). **Resolution:** `assemble` now gates each pair on the fully-fallible `has_component_of_kind` (`world_proxy.rs:561` — entity-absent / component-absent / kind-unregistered all → `false`) BEFORE `component_of_kind`, reproducing the bevy assembler's skip semantics exactly and making it genuinely panic-free per §2h M2. Byte-identical (present→Some→insert unchanged); g9pre stays GREEN. Doc corrected.
- **A3 (MEDIUM → NOTED, not a G7 defect) — `receive`'s `is_empty` gate doesn't surface timeout/kick disconnects.** `RecvHandle::receive` does not push `outstanding_disconnects`/`pending_disconnect_requests` into `ReceiveOutput`, so a disconnect detected on an otherwise-empty tick defers. **Disposition:** this is a pre-existing property of naia-core's shared `RecvHandle::receive`/`ReceiveOutput::is_empty`, and the bevy split path uses the **identical** gate (`plugin_full.rs:1419,1435`). Masked in practice (recv owns tick-advance → `pending_ticks` non-empty every live tick). Fixing it ONLY in the bracket would diverge from the byte-identical bevy path; if it's a real bug it's a shared-core fix applying to both paths — out of G7 scope.
- **A4 (LOW → RESOLVED) — promised D8→D9 `debug_assert` was absent.** Added `debug_assert!(send.send_prep_done_this_tick())` before the D9 snapshot build (new `pub(crate)` accessor reads the preamble+scope per-tick idempotency flags), machine-pinning that send-prep ran before the build.
- **A5 (LOW → RESOLVED) — handle-slot poison on mid-bracket panic.** Documented the take/restore poisoning caveat on `take_handles` (a panic between take and restore leaves slots empty → next entry's `"not in slot"` panic misdescribes the fault).

### 2l. G8 design pass — adapter mode-aware system sets + bracket dual send-shape (SIGNED OFF, Connor 2026-06-29)

Investigation of the landed G7 code surfaced a fork §2f had not resolved, and Connor signed off the resolution before impl (the design-sign-off-per-step cadence).

**The fork (VERIFIED against the landed bracket).** `PipelinedServer::send` → `drain_and_send` only ever runs the **oracle** (synchronous, inline `transmit_send_job`) shape (`sim_pipeline.rs:473-549`). The worker-driven **production** shape — build → `prepare_send_job` (freeze) → `attach_send_plan` → publish to the send worker via `SnapshotSender::send`, worker transmits NEXT tick (one-tick lag, MISSION_TICK_FLOOR Lever 3) — lives only in the runtime's send worker (`runtime.rs:787-840`) fed by a publisher the consumer hand-rolls. So if G8 simply pointed the adapter `SendPackets` set at `PipelinedServer::send`, production would transmit inline and **lose the worker overlap** — violating §2f "performance preservation (non-negotiable)."

**Decision 1 — production send-shape lives in the core bracket (Connor: "core bracket owns both").** `PipelinedServer<E>` gains `send_publisher: Option<SnapshotSender<E>>`. `drain_and_send` branches:
- `send_publisher = None` ⇒ **oracle**: `drain_all_acks` (main) → preamble → scope → refresh → build → `prepare_send_job` → `transmit_send_job` **inline**. (unchanged — the determinism/desync path the moat validates).
- `send_publisher = Some(tx)` ⇒ **production**: preamble → scope → refresh → build → `prepare_send_job` (freeze) → `snapshot.attach_send_plan(plan)` → `tx.send(snapshot)`. **NO main-side `drain_all_acks`** — the send worker is the single-owner ack consumer (`runtime.rs:833`); draining on main too would double-consume the cross-half ACK channel and trim `sent_updates` at the wrong tick → divergence. The worker transmits the lagged frozen job (`take_send_plan` → `transmit_send_job`).

Both reduce to the same `(snapshot, frozen plan)`, so they are byte-identical modulo the one-tick scheduling shift (G9pre §2i). The shape is set at construction by the adapter under **`#[cfg(workers_active)]`** (deterministic builds leave it `None` ⇒ oracle; production/bench set `Some` ⇒ worker) — consistent with the existing `workers_active = not(deterministic)` cfg that already switches the worker bodies. This is the mechanism G9's `ServerMode` knob will sit over; G9 exposes the *choice*, G8 puts both mechanisms in core.

**Decision 2 — the real-ack byte-identity test waits for G4/G5 (Connor).** The tracked A1 obligation (a `PipelinedServer`-vs-resident harness with a client acking) needs an entity replicated *through* the bracket, which is `spawn_replicated`/`enable_replication` (G4/G5, not built). It stays the tracked obligation in `pipeline_bracket.rs`; G8 does NOT build it from low-level primitives that G4/G5 would supersede.

**Adapter wiring (the rest of G8).** `PipelineConfig` gains `drive_bracket_in_update` (default **false**). When set, the adapter registers, **in the existing `ReceivePackets` / `SendPackets` sets**:
- `ReceivePackets` ⇒ `park_workers()` then `drain_recv_impl` (single-world, `entity_world = None`).
- `SendPackets` ⇒ `PipelinedServer::send(&world)` (now dual-shape) then `unpark_workers()`.
The consumer's systems sit between the sets via plain `add_systems(Update, …)` — workers parked, handles round-tripping through their slots, exactly the cyberlith park window but adapter-owned. **Opt-in (default false) is load-bearing for a non-breaking land:** the 6 existing pipelined adapter tests (`sim_integration_full_*`, `iris2`) drive park/unpark + tick-buffer **manually** and rely on the adapter NOT auto-driving a window in `Update`; cyberlith likewise still hand-rolls its window until G10. The end-state (post-G10) can flip the default.

**Deliberate G8 boundary — receive stays adapter-orchestrated (honest scope; RESOLVED by G8b §2m).** Only the SEND shape moved into core at G8 (Connor's decision was send-specific). The RECV park-window orchestration stayed in the adapter's `drain_recv_impl` because (a) it already composes the core `apply_recv_to_world` primitive faithfully, and (b) it additionally drains the recv worker's **output channel** (`runtime.recv_out_receiver()`, production-only) and fans `ReceiveOutput` into bevy `Messages<X>` — the channel-ownership move into a core `receive` that returns the multiple per-iteration outputs is a symmetric refactor NOT covered by the send-shape decision. Flagged as candidate follow-up G8b — **now done, §2m.** The single-world (`entity_world = None`) turnkey path is what a simple/non-bevy consumer gets; cyberlith's Sim-SubApp cross-world routing remains a consumer choice (its systems pick the worlds) per §2f, addressed at G10.

### 2m. G8b — recv-channel drain folded into core `receive` (SIGNED OFF, Connor; 2 decisions)

**Goal.** Make core `PipelinedServer::receive` the exact mirror of `send`: fold the recv-worker output-channel drain (today the adapter's `drain_recv_impl` hand-rolls it) INTO core, selected by a `recv_subscriber: Option<Receiver<ReceiveOutput<E>>>` field mirroring `send_publisher`. `None` ⇒ oracle (synchronous `recv.receive()` only); `Some` ⇒ worker production (drain channel FIFO via `try_iter()` + synchronous straggler-catch). Event fan-out into bevy `Messages` stays adapter-side (bevy-specific, §2h H3); single-world only (the dual-world `_split` path stays on `drain_recv_impl` until G10).

**Decision 1 (SIGNED OFF) — `receive` returns `Vec<ReceiveOutput<E>>`.** Worker mode yields N outputs/tick (channel burst + straggler), oracle yields 1; a uniform `Vec` is true send-symmetry and one code path. Touched only the single core-`receive` caller (the bracket test).

**Decision 2 (SIGNED OFF) — `set_recv_subscriber(Receiver<…>)` setter**, perfectly symmetric to `set_send_publisher`; the adapter wires it under `not(deterministic)`. The crossbeam `Receiver` is already public via `runtime.recv_out_receiver()`, so no new type leak.

**The N-proxy mechanism (root-cause, not band-aid).** A `WorldMutType` proxy is single-use by value, but worker mode applies N outputs to one world. So `apply_recv_to_world` + `WorldServer::process_all_packets` now take `world: &mut W` (reborrowable), threaded through. This is byte-identical: `process_all_packets` never moved `world`, only ever borrowed it `&mut`. `Server::process_all_packets` / `Client::process_all_packets` (the resident/bench/harness callers) KEEP their by-value signatures — only the WorldServer-internal path changed. The byte-exact **moat path (cyberlith `drain_recv_impl`) is UNTOUCHED**; only the new opt-in `drive_bracket_in_update` path now routes through core `receive`.

**Adapter wiring.** `wire_recv_subscriber_into_armed` (post-construction, because the recv channel is born inside `PipelineRuntime::new_armed` — asymmetric to `set_send_publisher`, whose channel is created earlier in `install_full_pipelining`). `pipelined_receive` now: gate-on-`Running` + park → `resource_scope` pulls the pipeline out → `pipeline.receive(&mut world.proxy_mut())` (core drains + applies) → fan out each output via `apply_receive_output_pipeline_with_event_receiver_split` (FIFO). Reorder vs the old `drain_recv_impl` (apply-all-then-fanout-all vs interleaved) is benign: `apply_recv_to_world` reads no fan-out state (fan-out only writes bevy `Messages`/markers); the real-ack byte-identity test (deferred G4/G5) is the eventual proof.

### 2n. UNIFIED `WorldServer` enum — the consumer-API keystone (SIGNED OFF, Connor; supersedes standalone G4/G3c framing)

**Context.** Designing G4 as a `spawn_replicated` fused op was REJECTED by Connor: keep naia's imperative style, "one way to do something", out-of-order calls deserve a panic. Instead the consumer-facing surface is the **chainable builder** `server.entity_mut(entity).enable_replication().configure_replication(config).enter_room(room_key)` — mirroring naia's EXISTING resident `EntityMut` (`server/src/world/entity_mut.rs`). The raw coord ops (`enable_entity_replication`/`configure_entity_replication`/`mark_entity_as_static`/room ops) go `pub(crate)`; the builder is the only public way.

**The deeper decision (Connor, against type proliferation + HARD requirement: ergonomic pipelining for NON-bevy naia users).** Do NOT add a `PipelinedEntityMut` (proliferation) and do NOT unify only at the bevy adapter (non-bevy users need it too). Instead unify in **core**:

- **REVISED (Connor 2026-06-29):** do NOT introduce a thin `ResidentWorldServer` newtype — it earns nothing. The monolithic engine becomes **`InternalWorldServer<E>`** (the fused engine = the 3 handles inline) and the enum's Resident variant holds it DIRECTLY. Net **removes** a type (anti-proliferation).
- New public enum **`WorldServer<E>` = `WorldServerImpl::{ Resident(InternalWorldServer<E>), Pipelined(PipelinedWorldServer<E>) }`** (`PipelinedServer` → `PipelinedWorldServer` for symmetry; `PipelinedServer` kept as alias for the diax facade). `InternalWorldServer` and `PipelinedWorldServer` are siblings built from the same 3 handles (`CoordHandle`/`RecvHandle`/`SendHandle`): the former holds them inline + synchronous drives; the latter holds them in park-slots + assembles a transient `InternalWorldServer` per drive.
- The enum exposes ONE unified consumer surface dispatching per variant: **FULL operational unify** — a single `receive(world)`/`send(&world)` drive (Resident's wraps its fused `process_all_packets`/send path, byte-identical), plus `listen`/`io_load`, room/user/config ops, and ONE `entity_mut(entity)` builder (single `EntityMut`, dispatches per variant — no `PipelinedEntityMut`).
- `MainServer`-based `Server<E>` (Full) stays an **orthogonal resident-only composite** wrapping the unified `WorldServer`. (Verified: `PipelinedServer::listen` → `ws.io_load` and handshakes in the engine recv path — Pipelined NEVER touches `MainServer`; it is the split form of the bare engine, exactly like `WorldOnly`.)
- **Consequence (Connor's insight, confirmed): the bevy adapter's `ServerImpl` needs NO `Pipelined` variant.** `WorldOnly` now wraps the unified core `WorldServer` (which internally handles Pipelined); the separate `PipelinedServer` bevy Resource path collapses into it.

This SUPERSEDES the old G3c ("adapter `ServerImpl::Pipelined`") and the standalone G4/G5/G5b op-steps — they all become methods on the unified `WorldServer` + its `EntityMut`. **Component ops** (insert/remove/despawn) ARE in the unified `EntityMut` ("all at once", Connor) — for the Pipelined variant they need the deferred-send mirror (the `configure_entity_replication` D.2.2 pattern) since `insert_component_worldless` touches both coord (`global_world_manager`) and send (`send_user_connections`) state. Phased execution plan: see §2o. Naia dev-trunk; byte-exact moat green at every phase.

### 2o. Phased execution of the unified `WorldServer` enum (Plan agent, each phase moat-byte-green)

Verification per phase: `cargo test -p naia-server` + `--lib`, `-p naia-test-harness` (esp. `g9pre_resident_pipelined_byte_identity`, `g9pre_core_assembler_byte_identity`), `-p naia-bevy-server` (default + `deterministic`), `cargo build --workspace`.

- **Phase 1 — pure mechanical rename `WorldServer` → `ResidentWorldServer`.** ✅ **COMPLETE.** Word-boundary rename across all 27 `server/src` files (struct + every impl/ref); `server/mod.rs` + `lib.rs` carry a **transitional `pub use ResidentWorldServer as WorldServer` alias** so ALL downstream (`Server` Full, bevy adapter, harness, cyberlith) compile UNCHANGED — the rename is invisible. Zero logic touched; byte-exact path identifiers only. Gates: naia-server lib 42 · harness g9pre 4/4 byte-identity · adapter 91 · deterministic build green.
- **Phase 2 — enum shell + `InternalWorldServer` extraction (Connor 2026-06-29, SIGNED OFF).** Duplication confirmed REAL and DRIFTING: every coord-only op is triplicated (monolith body + `CoordHandle`/`RecvHandle`/`SendHandle` body + `PipelinedServer` forwarder), and `entity_owner` already diverged (resident `.unwrap()` vs CoordHandle `Ok-else→Local`). Fix: extract a shared **`InternalWorldServer<E>`** = the fused engine (coord/recv/send sub-states over one `Arc<ServerShared>`) = today's monolith struct re-homed. It owns **coord-only ops + the main-thread park-window fused drives** (`process_all_packets`/`prepare_send_job`/apply). **Worker-thread socket I/O (`RecvHandle::receive`, `SendHandle` transmit) STAYS on the handles** — a different thread owns them (Connor decision). **Dedup mechanism (Connor signed off): `InternalWorldServer` = the 3 handles held INLINE** — restructure its fields to `{ coord: CoordHandle, recv: RecvHandle, send: SendHandle }`. Coord/recv/send op logic lives ONCE on the handles; `InternalWorldServer` delegates; fused drives use all three. `into/from_pipeline_handles` collapse to trivial destructure/construct. `PipelinedWorldServer<E>` = the same 3 handles in park-slots + worker runtime that **assembles `InternalWorldServer` transiently per drive** (the `with_world_server`/reassembly pattern made first-class — naia owns the mechanism, the mission's whole point); its ~40 forwarders + duplicate handle bodies collapse to delegation. **No `ResidentWorldServer` type** (Connor: the thin wrapper earns nothing) — enum `WorldServer = WorldServerImpl::{ Resident(InternalWorldServer), Pipelined(PipelinedWorldServer) }` holds the engine directly. `Server` (Full) holds `InternalWorldServer` directly (it IS the resident composite). Sub-steps: **2a ✅** mechanical rename monolith→`InternalWorldServer` (alias keeps downstream green); **2b-1 ✅** restructure `InternalWorldServer` fields → 3 handles inline (`recv: RecvHandle`, `send: SendHandle`, `sim_handle: CoordHandle`; explicit `.state` access, NOT Deref — `RecvHandle::receive`→ReceiveOutput would shadow `RecvState::receive`→() and break the moat; byte-identical); **2b-2 ✅** delegate the byte-identical coord-only bodies to `CoordHandle` (8 methods: entity_is_static, is_resource_entity, entity_authority_status, user_exists, current_tick, user_address, mark_entity_as_static + entity_owner with its drift fixed → safe `Local` fallback). Surveyed the rest and STOPPED: `create_room`→`RoomMut` / `entity_converter`→`&dyn` are different APIs (not dup); `user_keys` filters by send-connection (richer query); `enable/configure_entity_replication` + the worldless ops carry guard/deferred-send semantics → unified correctly in 2c/entity_mut, not band-aided. **2c-1 ✅** `PipelinedServer`→`PipelinedWorldServer` rename (core; transitional alias for the diax facade; adapter's own `PipelinedServer` Resource is a different type, untouched). **2c-2 ✅** stand up the real enum `WorldServer = WorldServerImpl::{Resident(InternalWorldServer), Pipelined(PipelinedWorldServer)}` (`server/world_server_enum.rs`) + `ServerMode`; shell = `new`/`new_pipelined` + dispatched `listen`/`current_tick`; private variant carrier (no pattern leak); resident-engine consumers repointed to concrete `InternalWorldServer`; adapter re-exports the enum; smoke test `world_server_enum_shell` (both modes). **Note:** the planned "collapse forwarders onto transient `InternalWorldServer` assembly" is largely MOOT after 2b — both `PipelinedWorldServer`'s forwarders and `InternalWorldServer`'s coord methods now route to the SAME `CoordHandle` bodies (single source of truth), which is cleaner than indirecting through a transient assembly. **Remaining:** Phase 3 (unified `receive`/`send` on the enum), Phase 4 (`entity_mut` builder on the enum), Phase 5 (adapter `WorldOnly`→enum: the 48-method forwarding surface + **remove the adapter `PipelinedServer` Resource in favour of the `Server` SystemParam routing through the enum** — Connor 2026-06-29).
- **Phase 3 — unified `receive`/`send` (moat-critical).** Resident grows `receive<W>(&mut W) -> Vec<ReceiveOutput>` (= `receive_with_world` sequence, `&mut W`, wrapped `vec![..]`) + `send<W>(&W)` (forwards to `send_all_packets`); enum dispatches both. Verbatim wrappers over the existing fused drives → byte-identical (g9pre guards).
- **Phase 4 — unified `EntityMut`.** `EntityMut.server` field → enum; `entity_mut`/`spawn_entity` dispatch; Pipelined arm uses coord-deferred ops (or stub where no op exists yet). No wire bytes.
- **Phase 5 — bevy adapter collapse.** `WorldOnly` carries the enum's `Pipelined` variant for the pipelined path; remove standalone `PipelinedServer` bevy Resource + `pipelined`/`world_only_pipeline` split; route worker park-window machinery through the enum. HIGHEST risk; full `sim_integration_full_*` + harness parallel-send suite.

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
| G6b | **(per §2h H2 + audit N3 — OPEN, design pass required)** host-sync drain (bevy change-detection → replication config). This is the **least framework-agnostic** piece in the mission: `drain_sim_host_sync_pipelined` (`server_access.rs:275-320`) is driven by bevy `Messages<HostSyncEvent>` against the **Sim** world. Open forks: (a) can a non-bevy consumer even have host-sync, or is it a bevy-adapter-only convenience? (b) does it survive in a no-reassembly core (§6 forbids reassembly post-G10)? (c) can every `HostSyncEvent` producer be retired by explicit G4/G5 `spawn_replicated`/`enable_replication`, removing the bridge entirely? **Decide before G10.** | **OPEN** (own design pass) |
| G7 | **naia-server core `receive`/`send` bracket** (FIRST) — `PipelinedServer::receive(world) -> ReceiveOutput` (park + single recv-drain + apply events) and `::send(&world)` (send-prep + snapshot + send-job + unpark); **+ moves the worker-thread runtime from the bevy adapter into core**. Explicit method sequence; consumer interleaves own code via unified `Server` ops. **No trait, no closures, no hooks.** **Supersedes** the old `with_parked_tick`. **Detailed design: §2g.** **Acceptance contracts: (a) the §2g drain-phase ordering table D0–D9 (N1) implemented as a single ordered core method; (b) the §2i assembler byte-identity test driving the real core `WorldRefType` assembler (N4).** | **G7-1 ✅** (assembler + N4 tests green) · **G7-2 ✅** (`receive`/`send` bracket + `drain_and_send` D0–D9 contract; `pipeline_bracket` + g9pre green) · **G7-3 ✅** (worker-runtime + park barrier + worker loops + Armed→Running→Stopped lifecycle moved into core `PipelineRuntime<E>`; `workers_active`/`deterministic`/`build.rs` relocated; bevy `PluginInternalState` delegates; naia-server lib + full naia harness + bevy deterministic suite green) → **G7 COMPLETE** |
| G8 | **naia-bevy-server mode-aware system sets** (layered on G7) — pipelined mode makes the existing `ReceivePackets` / `SendPackets` system sets run the parked/worker bracket internally; consumer systems sit between them via plain `add_systems(Update, …)`. **Zero new consumer-facing concepts.** Manages handle transit + consumer-chosen entity world. **+ core bracket dual send-shape (oracle inline / worker publish), per §2l Decision 1.** | **✅ COMPLETE** (design §2l SIGNED OFF) — core `PipelinedServer::{set_send_publisher, send}` dual send-shape (oracle inline / worker publish, NO double ack-drain); opt-in `PipelineConfig::drive_in_update` adapter wiring (`pipelined_receive` in `ReceivePackets`: park + recv-drain; `pipelined_send` in `SendPackets`: bracket send + unpark; gated on runtime `Running`). Tests: `pipeline_bracket` send-shape (oracle publishes-nothing / worker publishes-frozen-job) + `sim_integration_full_bracket_drive` (adapter drives a full window each `app.update()`, handles intact, park balanced). naia-server lib 42 · harness 143 (8 ign) incl. g9pre byte-identity · adapter 91 · both modes build. **Real-ack byte-identity test deferred to G4/G5 (§2l Decision 2); recv stays adapter-orchestrated (§2l boundary → follow-up G8b).** |
| G8b | **recv-channel drain folded into core `receive`** (symmetric to G8 send-shape) — core `PipelinedServer::{set_recv_subscriber, receive}` dual recv-shape (oracle synchronous / worker channel-drain); `receive` returns `Vec<ReceiveOutput>`. `apply_recv_to_world` + `WorldServer::process_all_packets` take `&mut W` (reborrow for N outputs; byte-identical, never moved world). Adapter `pipelined_receive` drives core `receive` + bevy fan-out; `drain_recv_impl` retained for the legacy/dual-world path (moat untouched). **Design §2m.** | **✅ COMPLETE** (design §2m SIGNED OFF, 2 decisions). naia-server lib 42 · harness clean (pipeline_bracket 3 · g9pre byte-identity 4) · adapter 91 (default + deterministic) · both modes build. Real-ack byte-identity still deferred to G4/G5 (§2l Decision 2). |
| G9pre | **(per §2h H1) PREREQUISITE SPIKE** — prove pipelined send content ≡ resident serialization byte-for-byte across diff-mask cases. | ✅ COMPLETE/GREEN (§2i) — byte-identical (envelope + payload) across full/partial/multi-entity masks + freeze-isolation (concurrent post-freeze live mutation does NOT leak); pipelined transmit content is a pure fn of `(snapshot, plan)`. **Scope caveat (N4): production needed-set assembler not yet exercised → G7 obligation.** |
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
