---
title: "MISSION — naia pipelined-sim consumer API + boundary restoration"
status: G3a COMPLETE — adversarial audit run (§2h, verdict NEEDS-REVISION) — spec corrected; 3 DECISIONS NEEDED (editor scope C2/H3, Resident≡Pipelined spike H1) before G7 sign-off
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

**(corrected per §2h — the original panic-arm classification was wrong; C1/C2.)**

The error in the first draft was assuming "send-side methods" are not callable in pipelined mode. They are: during the park window (between `receive` and `send`) the `recv`/`send` handles are **held** on `PipelinedServer`, so message-send, tick-buffer reads, and authority ops are all valid — cyberlith does exactly this today (`send_message_to_user` in `broadcast_desync_snapshots`, `entity_take_authority` in `drain_sim_editor_ops`, both inside the window). The correct classification:

- **Park-window-valid → REAL `Pipelined` arms** (the large majority). Three backing stores, all available during the window:
  - *coord-backed* (rest on main between ticks): `create_room`, `room_*`, `user_*`, entity reads, `mark_entity_as_static`, `configure_entity_replication`, `current_tick` (the G3a set).
  - *send-handle-backed* (the held `SendHandle`): `send_message`, `broadcast_message`, `broadcast`-style sends, scope fan-out — enqueue into per-connection outbound.
  - *recv-handle-backed* (the held `RecvHandle`): `receive_tick_buffer_messages`, request/response receive.
- **Genuinely-unavailable → panic arms (re-derived, must be empty or justified).** Only methods that need state truly absent in the window. The audit shows `entity_take_authority` is NOT one of these (editor cells reach it in-window) — see C2/G5b. **The panic set must be derived from actual park-window call sites in cyberlith, not assumed.** Target: empty.
- *Outside-window* methods (`listen`, `accept_connection` at startup) are handled by the G2 startup-window path, not the per-tick param.

This remains the chosen direction over a separate `PipelinedServer<'w>` param: single consumer-facing param. But the per-tick surface is far wider than the original draft claimed — message/authority/tick-buffer ops are first-class in pipelined mode.

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

**(per §2h H3 — world model, DECISION NEEDED.)** `receive` is written `&mut W` (single world), but delegated/editor cells apply recv events to **two** worlds in one call: `drain_recv_impl_split(main, Some(sim_world), …)` (`open_park_window`). Confirm whether the post-`#21-P4` end state is single-world for recv-apply. If multi-world persists, `receive` needs a designed two-world/composite contract — not "the adapter's choice."

Between `receive` and `send` the consumer runs its own code with workers parked — calling `self.spawn_replicated(...)`, `self.room_add_entity(...)`, `self.send_message(...)`, `self.receive_tick_buffer_messages(tick)` (the §2e-corrected park-window-valid surface). This is the park window, now implicit.

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

#### World model (corrected per §2h H3)

`receive` takes `&mut W: WorldMutType` (applies recv events); `send` takes `&W: WorldRefType + Sync` (reads for the snapshot). In a single-world consumer these are the same world. **DECISION NEEDED:** cyberlith's delegated/editor cells apply recv events to *two* worlds in one call (`drain_recv_impl_split(main, Some(sim_world), …)`). The single-`&mut W` signature does NOT model that. Either (a) confirm the post-`#21-P4` end state is single-world for recv-apply and editor is the only two-world case (then scope editor per C2/G5b), or (b) design a two-world/composite-world contract for `receive` in core. Do not ship the single-world signature until this is resolved.

#### Open questions for sign-off

1. Threading move: confirm relocating the worker runtime from `plugin_full.rs` into naia-server core (owned by `PipelinedServer`) is in-scope for G7 (it's required for a self-contained core bracket). — **CONFIRMED in-scope (Connor 2026-06-29).**
2. Snapshot assembler: core `WorldRefType`-based build vs lifting `SnapshotReaderRegistry` to core. — **RESOLVED (item 4): pure `WorldRefType` + `copy_to_box`, no registry lift.**
3. Unified core server enum: defer to post-G9 (not G7). — **CONFIRMED deferred (Connor 2026-06-29).**

### 2h. Adversarial audit — findings & resolutions (2026-06-29, verdict: NEEDS-REVISION)

A hostile audit (citations independently re-verified) found the factual layer solid but **§2e and §2f/§2g design not sign-off-ready**. Resolutions below; the affected sections are corrected in place and tagged "(corrected per §2h)".

- **C1 (CRITICAL — CONFIRMED) — §2e panic arms break the moat.** `broadcast_desync_snapshots` calls `send.send_message_to_user::<DesyncDetectionChannel, WorldSnapshotRecord>` **inside the park window** (`server_access.rs:820`, called from `do_park_window_tick:1664`, gated `#[cfg(feature="desync_detection")]` — the moat build). §2e routed `send_message`/`broadcast_message` to `panic!` "unreachable in practice" — FALSE. **Resolution:** the §2e classification was wrong. Message-send is a **park-window-valid op** — the `SendHandle` is *held* during the window — so it gets a **REAL Pipelined arm** that enqueues into the held `SendHandle`'s per-connection outbound (exactly what the desync broadcast does today). §2e rewritten below.
- **C2 (CRITICAL — CONFIRMED) — `entity_take_authority` panic arm contradicts editor cells.** `drain_sim_editor_ops` calls `ws.entity_take_authority(...)` via `run_with_naia_server` reassembly **inside the park window** (`server_access.rs:85`), for delegated/editor cells. §2e panics on it. **Resolution + DECISION NEEDED (Connor):** is the editor/delegated-cell path in scope for this mission? If yes, authority ops need real park-window arms (a coord/window-safe `take_authority`, not in G3a today) — add as **G5b**. If no, scope it out explicitly. *Defaulting to in-scope pending Connor.*
- **H1 (HIGH — UNVERIFIED-RISK) — Resident≡Pipelined byte-identity is an unproven, fallback-pre-banned linchpin.** Today's deterministic oracle is pipeline-split + inline send (trimmed `SnapshotWorld`), NOT resident (full-world serialize via a different driver, `server.rs:364`). **Resolution:** promoted from "OPEN note" to a **hard G9 prerequisite spike** — prove byte-identity BEFORE adopting "no SendStrategy". The "not a knob to re-add" purity goal is **conditionally lifted**: if the spike fails, keep a pipelined-synchronous oracle. G9 corrected below.
- **H2 (HIGH — CONFIRMED gap) — the bracket drops host-sync.** `drain_sim_host_sync_pipelined` (`server_access.rs:275`) reassembles `WorldServer` to bridge bevy change-detection → naia replication config; it's generic mechanism but appears nowhere in §2g, yet §6 forbids reassembly post-G10. **Resolution:** add host-sync placement to §2g (new **G6b**: coord/window-safe host-sync drain), OR prove every `HostSyncEvent` producer is retired by explicit G4/G5 `spawn_replicated`/`enable_replication`. Note: `drain_sim_host_sync_pipelined` still runs against the **Sim** world despite the `#21 P4` "main-world host-sync retired" claim — verify.
- **H3 (HIGH — LIKELY-FLAW) — single-world `receive`/`send` ignores two-world recv-apply.** Delegated cells call `drain_recv_impl_split(main, Some(sim_world), …)` — recv events apply to **two** worlds in one call (`open_park_window`). §2g's `receive(&mut W)` is single-world. **Resolution + DECISION NEEDED (Connor):** confirm whether the post-`#21-P4` end state is single-world for recv-apply (editor's `delegated` branch suggests not). If multi-world persists, the core bracket needs a designed two-world/composite-world contract — NOT "the adapter's choice". §2g world-model corrected to flag this.
- **M1 (MEDIUM) — "almost all generic" over-claims; needs a step ledger.** **Resolution:** §2g gains a line-by-line ledger of all ~10 `do_park_window_tick` steps → {core bracket | G4/G5/G6 op | consumer policy}, so nothing falls through (resource/lifecycle/scope-delta drains were unplaced).
- **M2 (MEDIUM) — assembler panic-vs-skip divergence.** Confirmed the assembler CHAIN is sound (the bevy registry itself just calls `copy_to_box` — `snapshot_reader_registry.rs:64`), but `component_of_kind` panics on an unregistered kind whereas the registry path skips. **Resolution:** core assembler must **skip-on-unregistered** to match. Noted in §2g.
- **M3 (MEDIUM) — "perf preserved by construction" ≠ moat-guaranteed.** The moat proves correctness, not speed; the `WorldRefType` assembler (HashMap + dyn dispatch + `copy_to_box`) may be slower than the registry's typed `get::<C>()`. **Resolution:** §6 gains a **numeric `bench_profile` per-phase gate** at G10 (not just "spans match"); decide up front whether the bevy adapter keeps its registry fast-path (if so, that's an acknowledged exception to "fully unified").
- **M4 (MEDIUM) — worker-move understates lifecycle entanglement.** Threading move is feasible (no real bevy coupling in the loops; `TestClock` is just a re-export), but the Armed→Running spawn lifecycle is tangled with bevy `Startup`/`Resource` (`plugin_full.rs:609`). **Resolution:** G7 must spec **core ownership of the Armed→Running / listen-timing state transitions**, not just thread spawn.
- **L1 (LOW) — sequencing.** G7→G10 are NOT all independently green: editor/desync paths have no valid Pipelined surface until §2e is corrected, so G10 can't compile-pass those features mid-sequence. **Resolution:** mark the atomic-only steps in §2d/§3.

**What the audit confirmed RIGHT:** the snapshot-assembler correctness (no registry lift — both paths funnel through `copy_to_box`), the one-tick-lag/freeze-point ordering, the "no hooks/closures/traits" shape matching naia's resident loop, and every `file:line` citation.

## 3. Sequence + status

| Step | Description | Status |
|------|-------------|--------|
| G1 | `SimPipeline<E>` + `TickCtx<E,W>` tick-driver; `SimPipelineRes` in bevy adapter; tests green | ✅ COMPLETE (`55272fad`) |
| G2 | `SimPipeline::listen(socket)` startup-window API; `PluginInternalState::listen` delegates to it | ✅ COMPLETE (`1e851a73`) |
| G3a | forwarding methods on `PipelinedServer<E>` for all coord-only ops | ✅ COMPLETE (`175d4bc7` rename + G3a impl) |
| G3b | cyberlith D11 `CellCommandsExt` dies, replaced by direct `pipelined_server.method()` calls in the park window | PENDING (design signed off) |
| G3c | unified `Server` param: add `ServerImpl::Pipelined(PipelinedServer<Entity>)` variant; **park-window-valid methods (coord-, send-handle-, and recv-handle-backed) get REAL arms** (incl. `send_message`/`broadcast_message`); panic set re-derived from actual call sites, target empty; retire raw `ResMut<PipelinedServer>` access | PENDING (design §2e corrected per §2h) |
| G4 | `spawn_replicated` fused op | PENDING |
| G5 | `enable_replication_for_existing_entity` | PENDING |
| G5b | **(per §2h C2)** coord/window-safe `entity_take_authority` (+ related authority ops editor/delegated cells use in-window) | PENDING — DECISION NEEDED: editor path in scope? |
| G6 | `Res<R>` resource API (`SimPipeline::insert_resource` etc.) | PENDING |
| G6b | **(per §2h H2)** coord/window-safe host-sync drain (bevy change-detection → replication config), OR proof every `HostSyncEvent` producer is retired by explicit G4/G5 | PENDING |
| G7 | **naia-server core `receive`/`send` bracket** (FIRST) — `PipelinedServer::receive(&mut world)` (park + single recv-drain + apply events) and `::send(&world)` (send-prep + snapshot + send-job + unpark); **+ moves the worker-thread runtime from the bevy adapter into core**. Explicit method sequence; consumer interleaves own code via unified `Server` ops. **No trait, no closures, no hooks.** **Supersedes** the old `with_parked_tick`. **Detailed design: §2g.** | PENDING (design §2g — pending sign-off) |
| G8 | **naia-bevy-server mode-aware system sets** (layered on G7) — pipelined mode makes the existing `ReceivePackets` / `SendPackets` system sets run the parked/worker bracket internally; consumer systems sit between them via plain `add_systems(Update, …)`. **Zero new consumer-facing concepts.** Manages handle transit + consumer-chosen entity world. | PENDING (design §2f — pending sign-off) |
| G9pre | **(per §2h H1) PREREQUISITE SPIKE** — prove Resident-mode wire output ≡ Pipelined-mode byte-for-byte across scope/diff-mask cases. Gate for G9. | PENDING (blocks G9) |
| G9 | **`ServerMode::{Resident, Pipelined}` — single knob** — Pipelined⇒worker send, Resident⇒synchronous send. Same `receive`/`send` signatures; consumer code unchanged. Deterministic oracle runs as `Resident` **IFF G9pre passes**; else retain a pipelined-synchronous oracle (the "no SendStrategy" purity goal is conditional on G9pre). | PENDING (design §2f — pending sign-off) |
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
