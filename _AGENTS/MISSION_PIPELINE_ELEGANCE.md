# MISSION: Pipeline Elegance — collapse the coord/send seam & unify the server surface

**Status:** PLANNED (2026-06-30). Successor to `MISSION_PIPELINE_API_BOUNDARY.md`.
**Audience:** an implementing agent with ZERO prior context on this repo. Read
Phase 0 in full before touching code — it contains the mental model, the build/
test/verify commands, and the invariants you must not break. Then execute the
phases **in order** (A → B → C → D). Each phase is self-contained and lands on
its own, moat-GREEN, before the next begins.

Connor signed off all four work items as **non-negotiable**. Phases below are the
four items resequenced into execution order (the original item IDs are retained
in headers so the Definition-of-Done checklist stays traceable).

---

## PHASE 0 — Orientation (READ FIRST)

### 0.1 What this repo is

`naia` is a Rust networking/replication library (server + client + shared
protocol) with a Bevy adapter. It is the **path-dependency of `cyberlith`** (a
game), and cyberlith's byte-exact determinism test suite (the "moat") is the
ultimate referee for any change here. The two repos are siblings:

```
/home/connor/Work/specops/naia        ← you edit here (this repo)
/home/connor/Work/specops/cyberlith   ← consumes naia via a Cargo path patch
```

cyberlith's root `Cargo.toml` has `[patch.crates-io]` entries pointing at
`../naia/...`, so **any edit you make here is live in the next cyberlith build**
— no publish step. cyberlith builds naia from the **`dev`** branch (see 0.5).

### 0.2 naia top-level layout

| Dir | Holds |
|---|---|
| `server/` | Core server engine — **this is where most of this mission lives**. `src/server/` (the `WorldServer` enum, resident `InternalWorldServer`, coord/recv/send substates), `src/pipeline_actors/` (the pipelined runtime + `PipelinedWorldServer`). |
| `client/` | Core client. |
| `shared/` | Protocol, derive macros, transports, serde, `TestClock`. |
| `adapters/bevy/server/` | Bevy server adapter — the `Plugin` (Phase D), `ServerImpl`, and the `tests/` integration suite. |
| `test/` | Test infra: `specs/` (Gherkin `.feature` specs — the **primary** test surface), `npa/` (the namako adapter binary), `tests/` (Rust step bindings), `harness/` (`Scenario`/`World`), `bevy_specs/`. |
| `bench/`, `demos/`, `docs/`, `book/`, `_AGENTS/` | benchmarks, examples, docs, mission docs. |

### 0.3 The mental model you MUST hold (the two axes)

There are two orthogonal ideas that this codebase currently tangles together —
untangling them is the spine of this mission:

- **Axis A — connection ownership / world topology.** *What the consumer plugs
  in.* Three shapes:
  - **Standalone** — naia owns the connection handshake (`MainServer`) AND the
    replicated world. The classic `naia_server::Server<E>`.
  - **WorldProxied** — naia owns only the `WorldServer`; connections are proxied
    by an upstream service. (cyberlith's game cell sits behind a session server.)
  - **SimIntegration** — the world lives in the *consumer's* ECS; naia holds only
    the three pipeline handles, driven by the consumer's coordinator.

- **Axis B — drive shape.** *How naia runs the engine.* Two shapes:
  - **Resident** — the fused engine (`InternalWorldServer`) driven **synchronously**
    on the calling thread. No worker threads.
  - **Pipelined** — the same engine's three handles (coord/recv/send) split into
    park-slots with a worker runtime; the engine is **transiently reassembled**
    per control op. In production, recv/send run on worker threads overlapped
    with the consumer's compute; in deterministic builds the workers are parked
    and everything runs synchronously (the byte-exact "oracle").

The core type is `WorldServer<E>` (`server/src/server/world_server_enum.rs`), a
public enum wrapper over a **private** `WorldServerImpl::{Resident(InternalWorldServer),
Pipelined(PipelinedWorldServer)}`. Every public method matches on the variant and
dispatches. Keep the variant carrier private — consumers act only through the
dispatched surface.

### 0.4 Key internal concepts (glossary)

- **coord / recv / send handles** — the engine is split into three substates:
  `CoordHandle` (coordinator: rooms, users, entity registry, priority),
  `RecvHandle` (inbound decode), `SendHandle` (outbound serialization + per-user
  connections + scope maps). "coord-resident" state is reachable by `&self` in
  BOTH modes; "send-resident" state lives inside `SendHandle`, which in pipelined
  mode sits behind a `Mutex` slot.
- **`with_world_server`** (`server/src/pipeline_actors/sim_pipeline.rs:445`) — the
  pipelined "reassembly": lock all three slots, move the substates into a
  transient `InternalWorldServer`, run one closure, tear it back down, restore
  slots. Used by control/config ops. **Phase C's job is to make this nearly
  disappear.** Cost is cheap (moves + 6 uncontended `parking_lot` locks) and it
  is NEVER on the per-tick hot path — but it is the least elegant seam here.
- **D0–D9 drain contract** (`sim_pipeline.rs:1193` `drain_and_send`) — the single
  total order in which queued mutations + the send job apply each tick. This
  ordering **IS** the byte-exactness contract. Any new staged op MUST be slotted
  explicitly into this order by class, never appended ad hoc.
- **task #13 staging precedent** (`sim_pipeline.rs:1296` `publish_priority`) — the
  model Phase C generalizes: priority writes **stage on coord** and drain into
  send at the top of `drain_and_send`, instead of reassembling. Byte-identical to
  the resident direct write.
- **slot-lock read precedent** (`sim_pipeline.rs:990` `user_scope_has_entity_ref`
  + `server/src/server/world_server.rs:4649` `user_scope_has_entity_impl`) — the
  model Phase B follows: a pipelined `&self` read locks the send slot
  **read-only** (`send_slot.lock().as_ref()`, no `take`) and calls a shared
  `_impl` free function that the resident engine ALSO calls — one body, zero
  drift. This is the established way to read send-resident state without
  reassembly.
- **oracle** — the deterministic build's synchronously-driven Pipelined bracket.
  It, not Resident, is the byte-exact reference the moat validates.
- **`workers_active` cfg** — `server/build.rs` emits `workers_active = not(deterministic)`.
  Deterministic build → parked workers (synchronous oracle); production → active
  worker threads. `test_time` (advanceable clock) is orthogonal.

### 0.5 Invariants you must NOT violate

- **naia is dev-trunk.** Commit on `dev`. **Never** commit on `main`.
- **No wire-format changes. Ever.** Every item here is an internal-surface
  refactor. Byte output on the wire must be identical — that's what the moat
  checks.
- **Never loosen the moat, regenerate snapshots, or relax an assertion** to make
  a test pass. A moat failure means your change altered behavior — fix the change.
- **Root cause, not band-aid.** No skip/gate/sleep/flag workarounds.
- specops conventions: no env vars, no tokio (async-std/smol; thiserror/anyhow
  allowed), snake_case, no re-exports in the public surface beyond what exists.

### 0.6 How to build & test (verified commands)

**naia unit/integration tests** (run from `/home/connor/Work/specops/naia`):
```bash
# Full workspace (the correctness floor). CI enforces -D warnings.
cargo test --workspace --all-targets

# The crates this mission touches:
cargo test -p naia-server
cargo test -p naia-bevy-server          # bevy adapter; dev-deps enable `deterministic`
cargo test -p naia-bevy-server --test '*'   # just the adapters/bevy/server/tests/ suite
```
The bevy adapter's dev-dependency already turns on `deterministic` (parked
workers / byte-exact oracle), so adapter tests exercise the pipelined-oracle path
by default.

**namako specs (the PRIMARY surface — add specs here, not raw `#[test]`).**
Gherkin `.feature` files live in `test/specs/features/` (core) and
`test/bevy_specs/features/` (bevy); Rust step bindings in `test/tests/src/`; the
runner is the NPA binary in `test/npa/`. CI runs them via a `namako gate` over
`test/specs` with the NPA as adapter. **Confirm the exact local invocation before
relying on it** (check `_AGENTS/SYSTEM.md` and `.tesaki/config.toml`); the CI gate
is the source of truth. Each phase below says which spec to add.

**The cyberlith moat (the ultimate referee).** Run from
`/home/connor/Work/specops/cyberlith` after every phase. Suites live under
`test/game/harness/` (package `cyberlith_test_harness`):
```bash
cargo test -p cyberlith_test_harness --test simulation  --features desync_detection
cargo test -p cyberlith_test_harness --test integration --features desync_detection
cargo test -p cyberlith_test_harness --test e2e
# architectural firewalls (part of the moat job):
cargo test -p cyberlith_test_harness --test input_firewall        --features desync_detection
cargo test -p cyberlith_test_harness --test gi_determinism_firewall --features desync_detection
cargo test -p cyberlith_test_harness --test web_driver_firewall     --features desync_detection
```
`desync_detection` adds per-entity `SnapshotId` and asserts
`server[T] == confirmed[T] == predicted[T]` byte-for-byte each tick. The moat also
enforces a **test-count floor** (anti-false-green) — do not let the count drop.
The suites are referred to as sim / integration / e2e / B1-active; **confirm the
current expected counts and the B1-active invocation from cyberlith's
`.github/workflows/moat.yml` + `_AGENTS/` docs** before you start, and reproduce
them GREEN as your pre-change baseline. Authoritative moat docs: cyberlith
`AGENTS.md` (netcode contract), `.github/workflows/moat.yml`, `test/TESTING.md`,
`_AGENTS/NETWORKING_MODEL.md`.

### 0.7 Baseline gate (do this BEFORE writing any code)

1. `git -C /home/connor/Work/specops/naia branch --show-current` → must be `dev`
   (or branch from `dev`). Never `main`.
2. Run the naia workspace tests and the full cyberlith moat above. Record the
   GREEN baseline (counts included). If anything is RED before you start, STOP and
   surface it — do not build on a red baseline.

---

## PHASE A — `Server::new(mode)`: explicit required drive shape  *(Item 2)*  ✅ DONE

**Smallest, most isolated change — do it first to warm up the `ServerMode` surface.**

> **Landed 2026-06-30.** `Server::new`/`new_with_protocol_id` now take a required
> `ServerMode` first arg; `Server::new_pipelined` deleted (was zero-call-site);
> `with_mode` folded into `new_with_protocol_id`. Call sites ported (harness
> `scenario.rs`, `bench/criterion`, `demos/basic`+`demos/macroquad`, bevy adapter
> `plugin.rs` Full arm → `ServerMode::Resident`). `WorldServer::new`/`new_pipelined`
> left as-is (lower-level test surface; symmetry optional). Workspace builds clean;
> naia tests 30/30 binaries GREEN; cyberlith simulation moat **55/55 byte-exact**.

### What is
`naia_server::Server<E>` (`server/src/server/server.rs:54`) holds a
`WorldServer<E>` and offers two constructors: `Server::new` →
`with_mode(ServerMode::Resident, …)` (`:65`) and `Server::new_pipelined` →
`with_mode(ServerMode::Pipelined, …)` (`:93`). `ServerMode` (defined in
`world_server_enum.rs:46`) and the private `with_mode` (`server.rs:100`) hide the
choice behind two names. `WorldServer` itself already has the parallel
`WorldServer::new` / `WorldServer::new_pipelined` (`world_server_enum.rs:67,75`).

### Target
Drive shape becomes a **required, explicit** argument. Delete `new_pipelined`.
```rust
// naia_server (world_server_enum.rs): make ServerMode public if not already.
pub enum ServerMode { Resident, Pipelined }

// server.rs
impl<E> Server<E> {
    pub fn new<P: Into<Protocol>>(mode: ServerMode, server_config: ServerConfig, protocol: P) -> Self;
    // new_pipelined: DELETED. with_mode: fold into `new` or keep private.
}
```
Do the same shape for the `WorldServer` constructors if you want symmetry
(`WorldServer::new(mode, …)` replacing `new`/`new_pipelined`) — but that's
optional; the required deliverable is `Server`. Keep `WorldServer::from_pipelined`
(the adapter uses it, Phase D).

### Steps
1. Make `ServerMode` `pub` and re-exported from the crate root
   (`server/src/lib.rs`) if not already public.
2. Change `Server::new` to take `mode: ServerMode` as the first param; delete
   `Server::new_pipelined`; inline/keep `with_mode`.
3. Fix every call site (`rg 'Server::new\b|new_pipelined' server/ adapters/ test/`).
   The bevy adapter constructs `Server` internally (Phase D territory) — update it
   to pass the mode it currently implies.
4. `cargo test -p naia-server && cargo test -p naia-bevy-server`, then the moat.

### Verification / DoD
- No `Server::new_pipelined` remains; `Server::new` requires an explicit
  `ServerMode`. Workspace tests + moat GREEN. No wire change (this is
  construction-only).

### Risk: low, mechanical (a signature + enumerable call sites).

---

## PHASE B — Finish `interior_visibility` in pipelined mode  *(Item 4)*

**Self-contained. Follows the existing slot-lock read precedent — do NOT relocate
state to coord (that was considered and rejected; see "Why not coord-relocate").**

### What is (the current gap)
With the `interior_visibility` feature on, three read methods panic on the
pipelined arm:
- `WorldServer::local_entities` / `local_entity` / `local_entity_mut`
  (`world_server_enum.rs:1290-1332`) — `panic!("… unsupported in pipelined mode …")`.
- `EntityRef::local_entity` (`server/src/world/entity_ref.rs:100-115`) — same panic.

The data they need — the user→local-entity mapping — lives in **send-resident**
state: specifically each user's `Connection.base.world_manager.entity_converter()`
inside `SendHandle` (`send.state.send_user_connections`). The resident engine
reads it inline; the pipelined arm's only wired reach-in (`with_world_server`)
needs `&mut`, so the `&self` reads panic. No pipelined consumer currently enables
the feature, so it's harmless today — but it's **unfinished plumbing**, and the
fix is small and already-patterned.

### The resident bodies to share
In `server/src/server/world_server.rs`:
- `local_entities` (`:4556`) — reads `sim_handle.state.user_store` (coord) +
  `send.state.send_user_connections[addr].base.world_manager.local_entities()`
  (send). Returns an owned `Vec<LocalEntity>`.
- `local_to_world_entity` (`:4606`) and `world_to_local_entity` (`:4624`) — resolve
  an id by reading coord `user_store` + send `send_user_connections` +
  `shared.global_entity_map`. Return owned values.
- `local_entity` (`:4573`) / `local_entity_mut` (`:4593`) — resolve the world
  entity via `local_to_world_entity` (the send-resident read) and then defer to
  the ordinary `self.entity(...)` / `self.entity_mut(...)` (coord/world path). So
  the ONLY send-resident touch is the id resolution.

### The precedent to copy (already in the tree)
`PipelinedWorldServer::user_scope_has_entity_ref` (`sim_pipeline.rs:990`) is a
`&self` read that: takes `self.coord()`, does `self.send_slot.lock()` +
`.as_ref()` (read-only, NO take), and calls the shared free fn
`user_scope_has_entity_impl` (`world_server.rs:4649`) which the resident engine
also calls. One body, zero drift, no reassembly.

### Target
1. **Factor shared `_impl` free functions** (in `world_server.rs`, next to
   `user_scope_has_entity_impl`) for the send-resident reads:
   `local_entities_impl(coord_state, send_state, user_key) -> Vec<LocalEntity>` and
   `local_to_world_entity_impl` / `world_to_local_entity_impl` taking `&` refs to
   the coord + send substates + `shared`. Rewrite the resident
   `local_entities`/`local_to_world_entity`/`world_to_local_entity` to call them
   (proves the shared body is byte-identical for the resident path).
2. **Add pipelined `&self` slot-lock accessors** on `PipelinedWorldServer`
   (mirroring `user_scope_has_entity_ref`): lock `send_slot` read-only, call the
   `_impl`. Expose `local_entities`, and an id-resolver the enum's `local_entity`
   can use.
3. **Rewrite the `WorldServer` enum arms** (`world_server_enum.rs:1290-1332`) to
   dispatch to the pipelined accessors instead of panicking. For `local_entity` /
   `local_entity_mut`: resolve the world entity via the slot-lock read, then reuse
   the existing `self.entity(world, &we)` / `self.entity_mut(world, &we)` path
   (both modes share that tail).
4. **Fix `EntityRef::local_entity`** (`entity_ref.rs:100-115`): the `Pipelined`
   arm calls the pipelined `world_to_local_entity` resolver instead of panicking.
5. Delete every `"unsupported in pipelined mode"` panic string for this feature.

### Why not coord-relocate
The local-entity converter is the send connection's `world_manager` — it's
intrinsically send-side (it drives serialization). Moving it to coord would fight
the architecture and touch the hot send path. The slot-lock read precedent already
exists for exactly this class of send-resident `&self` read; use it.

### Verification / DoD — ✅ DONE
- ✅ Dual-mode test lands in `test/harness/contract_tests/integration_only/world_server_enum_shell.rs`
  (`world_server_enum_{resident,pipelined}_interior_visibility_reads`) — this
  suite has `interior_visibility` on unconditionally via its naia-server dep and
  already runs every case against BOTH engine shapes. The test registers a
  server-owned entity and drives all three reads (`EntityRef::local_entity` →
  `world_to_local_entity`; `WorldServer::local_entity`/`local_entity_mut` →
  `local_to_world_entity`) through both arms, asserting the pipelined slot-lock
  read **dispatches without panic** and returns results **identical to resident**.
  - The **populated** send-resident read is the very same shared `_impl`
    (`{local_to_world,world_to_local}_entity_impl`), already covered end-to-end
    with a real connected user by the resident harness
    (`client_events`/`server_events` → `server_ref.local_entity(&user_key)`), and
    the pipelined slot-lock wrapper is structurally identical to the
    connected-tested `user_scope_has_entity_ref` read. A connected-client
    pipelined path does not exist in any naia-side suite (namako/moat drive it,
    but cyberlith does not enable `interior_visibility`), so the unconnected
    dual-mode equivalence + shared-`_impl` argument is the proportionate guardrail.
- ✅ No `interior_visibility` panic arms remain (enum + `EntityRef` Pipelined arms
  now delegate to the slot-lock resolvers).
- ✅ `RUSTFLAGS="-Dwarnings" cargo build -p naia-server --features interior_visibility`
  clean; `cargo test -p naia-test-harness` GREEN (incl. 8/8 enum_shell); moat GREEN
  (sim 39/0, int 41/0, e2e 115/0).

### Landed
- naia-server: `world_server.rs` (`{local_entities,local_to_world_entity,world_to_local_entity}_impl`
  free fns + resident delegation), `mod.rs` (gated re-exports), `world_server_enum.rs`
  (Pipelined arms delegate, panics removed), `entity_ref.rs` (Pipelined arm delegates).
- sim_pipeline.rs: three `#[cfg(interior_visibility)]` `&self` slot-lock accessors on
  `PipelinedWorldServer`.
- Test: `world_server_enum_shell.rs` dual-mode reads.

### Risk: medium — reads send-resident state, but read-only and precedented. The
dual-mode test is the guardrail.

---

## PHASE C — Collapse `with_world_server` via generalized coord-side staging  *(Item 1)*

**The big one. Incremental: convert ONE op class per PR, moat GREEN between each.**

### What is
`with_world_server` (`sim_pipeline.rs:445`) backs every send-resident **mutation**
on the pipelined arm — the `ps.with_world_server(...)` arms across
`world_server_enum.rs` and the `&mut` cluster at `sim_pipeline.rs:1007`
(`user_scope_set_entity`, `user_scope_remove_user`, `room_broadcast_message`, and
via the enum: `send_message`, `broadcast_message`, `insert_resource`,
`remove_resource`, `enable_delegation`, `entity_take_authority`/`give`/`release`,
`configure_entity_replication`, historian ops, …). Each reassembles the whole
engine to run one method, inside the park window.

The hot per-tick path does NOT use it: `receive_into` (`:1097`) and `send`
(`:1163`) thread raw `(coord, recv, send)` handles through `apply_recv_to_world`
(`server/src/pipeline_actors/orchestration.rs:284`) and `drain_and_send` (`:1193`)
— zero reassembly. Phase C makes the *control* ops match that discipline.

### The precedent to generalize
task #13 priority: `publish_priority` (`sim_pipeline.rs:1296`) stages coord-side
writes and drains them into send at the TOP of `drain_and_send` (`:1203`), in the
D0–D9 order. Also see `PendingScopeAuthorityOps` in the bevy adapter
(`adapters/bevy/server/src/server.rs:59`, drained at `:1050`/`:1098`) — the same
"stage a queue, drain it in order" shape for authority/room ops.

### Target
Every send-resident mutation becomes a **coord-side staged op**, drained in an
explicit D0–D9 slot. When done, `with_world_server` survives ONLY for (a) one-time
`io_load` and (b) the deterministic oracle's whole-engine synchronous drives — no
per-gameplay-event reassembly remains.

### Steps
1. **Inventory & classify** every `with_world_server` call site into: **(a)**
   already coord-resident (shouldn't reassemble — if so, convert to a
   coord fast-path, it's a latent bug), **(b)** send-resident mutation → convert
   to a staged op, **(c)** genuinely monolithic → keep. Fill the appendix table.
2. For each **(b)**: add a coord-side pending queue (mirror `publish_priority` /
   `PendingScopeAuthorityOps`), and **slot its drain explicitly into D1–D7 by
   class** in `drain_and_send` — the doc comment there
   (`sim_pipeline.rs:1204-1213`) already reserves D1=entity-registration,
   D2=resource, D3=lifecycle, D4=authority/editor, D5=host-sync, D6=outbound
   messages, D7=scope-ledger. Place each op in its class; do not invent new order.
3. **Convert one op class per PR**, in dependency order (messages → scope →
   resource → authority → historian). Moat GREEN + a namako spec asserting
   resident≡pipelined-oracle **byte-equality** for that class before each merge.
4. Once the (b) set is empty, narrow `with_world_server`'s visibility/doc to the
   two surviving monolithic uses (consider renaming to signal "monolithic-only").

### Verification / DoD
- `with_world_server` used only by `io_load` + oracle drives. Per-class byte-
  equality namako specs GREEN. Moat GREEN ×10 (determinism under repetition).

### Risk: highest of the four — the D0–D9 total order IS byte-exactness. Mitigate:
one op class per PR, each with a byte-equality spec, moat ×10 before merge. Never
batch-convert.

---

## PHASE D — Unify the `Plugin` constructors: one explicit config  *(Item 3)*

**Reshape the outer surface LAST, against the finished internals.**

### What is
The bevy adapter `Plugin` (`adapters/bevy/server/src/plugin.rs`) exposes **7
constructors** (`new` :118, `pipelined` :128, `world_only` :150,
`world_only_pipeline` :160, `types_and_sets_only` :173, `sim_integration` :207,
`sim_integration_with_schedule` :221) funneling into `new_impl` (:237) over **5
entangled booleans** (`world_only`, `pipeline`, `state_external`,
`change_detection_schedule`, `full_pipelining`) plus two more flags
(`skip_host_sync_change_tracking`, `drive_bracket_in_update`, :108/:113). Problems:
- **Axis A and Axis B are conflated** across the constructor names.
- **Full + Pipelined is unexpressible** — the pipelined engine only appears via
  `full_pipelining` (WorldProxied) or caller-supplied handles (SimIntegration).
- `world_only_pipeline` is mislabeled — it builds a **Resident** engine with
  externally-driven recv/send, not a pipelined engine.

Mapping to Axis A/B today: `new`=Standalone/Resident; `world_only`=WorldProxied/
Resident; `world_only_pipeline`=WorldProxied/Resident+external-drive;
`pipelined`=WorldProxied/Pipelined (`plugin_full.rs:132`
`install_full_pipelining` → `WorldServer::from_pipelined`, :148);
`sim_integration[_with_schedule]` / `types_and_sets_only`=SimIntegration
(no `ServerImpl`; caller supplies handles).

### Target — a single `Plugin::new(config)` with illegal states unrepresentable
Prefer the **enum config** over more named constructors (re-explode on the next
axis) and over a free-form builder (permits nonsense like Resident+PipelineConfig).
The config **rides on the variant that needs it**:
```rust
// naia_bevy_server
pub struct ServerPluginConfig {
    pub server_config: ServerConfig,
    pub protocol:      Protocol,
    pub topology:      Topology,
}
pub enum Topology {
    Standalone(DriveShape),    // naia owns MainServer + world
    WorldProxied(DriveShape),  // naia owns only the WorldServer; connections proxied upstream
    SimIntegration(SimIntegrationConfig), // world in consumer's ECS; naia holds handles
}
pub enum DriveShape {
    Resident,
    Pipelined(PipelineConfig),  // PipelineConfig (plugin_full.rs:59) rides ONLY here
}
pub struct SimIntegrationConfig {
    pub change_detection_schedule: Option<InternedScheduleLabel>,
    pub skip_host_sync_change_tracking: bool,
    pub drive_bracket_in_update: bool,
    // (the SimIntegration/Pipelined-only tuning flags live here)
}
impl Plugin { pub fn new(config: ServerPluginConfig) -> Self; }
```
Full, legal-only matrix:

| Old constructor | New `Topology` |
|---|---|
| `Plugin::new` | `Standalone(Resident)` |
| *(was impossible)* | `Standalone(Pipelined(cfg))` ✅ newly expressible |
| `Plugin::world_only` | `WorldProxied(Resident)` |
| `Plugin::pipelined` | `WorldProxied(Pipelined(cfg))` |
| `Plugin::sim_integration[_with_schedule]` | `SimIntegration(cfg)` |
| `Plugin::types_and_sets_only` | `SimIntegration(cfg)` (same shape sim_integration became) |
| `Plugin::world_only_pipeline` | **DECISION NEEDED:** `WorldProxied(Resident)` + an explicit external-drive flag, or deprecate. Its name lied (Resident engine, externally-driven recv/send). Record the decision here when made. |

### Steps
1. Add `ServerPluginConfig` / `Topology` / `DriveShape` / `SimIntegrationConfig`
   and `Plugin::new(config)`; route the existing `new_impl` internals from the
   config (the 5 booleans become a pure function of the enum — encode that mapping
   once, centrally).
2. Turn the 7 old constructors into thin **`#[deprecated]` shims** that build a
   config and call `Plugin::new` — keeps naia + cyberlith compiling while they
   migrate in separate commits.
3. Migrate cyberlith call sites (in `/home/connor/Work/specops/cyberlith`):
   - `services/game/cell/src/init.rs:102` `NaiaServerPlugin::pipelined(...)` →
     `WorldProxied(Pipelined(cfg))`.
   - game cell Sim `sim_integration_with_schedule(SimMain)` (see
     `services/game/cell/src/sim_app.rs`) → `SimIntegration{ schedule: Some(SimMain), … }`.
   - `services/asset_editor/src/main.rs:93` & `src/server_local.rs:145`
     `ServerPlugin::new(...)` → `Standalone(Resident)`.
4. Delete the deprecated shims once cyberlith is fully moved. Moat GREEN.

### Verification / DoD
- One `Plugin::new(ServerPluginConfig)`; `Standalone(Pipelined)` expressible;
  illegal combos unrepresentable at the type level; old constructors gone after
  migration. naia tests + cyberlith moat GREEN.

### Risk: medium — touches the public plugin surface + cyberlith init. The
deprecated-shim bridge lets naia and cyberlith move independently, moat GREEN
throughout.

---

## Definition of done (whole mission)

- [x] **Phase A / Item 2:** `Server::new(mode, …)` required-arg; `new_pipelined`
      deleted; all call sites ported; workspace tests + moat GREEN. ✅ 2026-06-30
- [x] **Phase B / Item 4:** `interior_visibility` works in pipelined mode via the
      slot-lock read precedent (shared `_impl` + `&self` accessors); zero
      "unsupported in pipelined mode" panics; dual-mode (resident≡pipelined)
      test GREEN; moat GREEN. ✅ 2026-06-30
- [ ] **Phase C / Item 1:** `with_world_server` used ONLY by `io_load` + oracle
      drives; every send-resident mutation is a D0–D9 staged op; per-class
      byte-equality namako specs; moat GREEN ×10.
- [ ] **Phase D / Item 3:** single `Plugin::new(ServerPluginConfig)` with
      `Topology`/`DriveShape`/`SimIntegrationConfig`; Full+Pipelined expressible;
      old constructors deleted after cyberlith migration; illegal combos
      unrepresentable; moat GREEN.
- [ ] Across ALL phases: naia on `dev`; no wire-format change; moat never loosened,
      no snapshot regenerated, no assertion relaxed; each phase landed GREEN
      before the next began.

## Appendix — `with_world_server` call-site inventory (fill during Phase C step 1)

| Call site (file:line) | Class (a/b/c) | Target staged-op / D-slot / notes |
|---|---|---|
| _(to fill)_ | | |
