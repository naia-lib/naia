---
title: "MISSION — naia pipelined-sim consumer API + boundary restoration"
status: DESIGN — open questions in §3; Connor sign-off pending
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

## 2. Design decisions — OPEN (Connor sign-off pending)

### 2a. G1 tick-driver shape

**Option A — additive `PipelineDriver<E>` resource (non-breaking)**
`spawn_server_handles` installs a fourth resource alongside the existing three `*HandleRes`.
```rust
driver.tick(&mut world_proxy, |ctx| {
    ctx.host_sync();
    ctx.send_all_packets();
});
```
+ Additive; three handles still accessible during migration.
− Four resources long-term; callers can still grab handles separately.

**Option B — unified `SimPipeline<E>` replaces three separate handles (breaking)**
Three handles become internal. `spawn_server_handles` returns `SimPipeline<E>`.
```rust
pipeline.tick(&mut world_proxy, |ctx| { ... });
pipeline.mark_entity_as_static(&entity);   // G3 also on same type
```
+ Cleanest long-term API; handles are a true implementation detail.
− Breaking change; naia API + cyberlith migration must land atomically.

**Option C — `tick(...)` on `SimHandle<E>` taking recv+send as arguments**
No new types; three-arg call site.
− Awkward ergonomics; mixes coord + lifecycle concerns.

**Tycho recommendation: Option B** (single type, handles internal). Option A acceptable if incremental landing is preferred.

### 2b. G3 pub(crate) strategy

**Strategy 1 — thin named methods on `SimHandle<E>` / `SimPipeline<E>` (RECOMMENDED)**
One naia method per leaked op. Zero internal exposure. Bevy adapter `CommandsExt` becomes a shim.
```rust
sim.mark_entity_as_static(&entity);
sim.configure_entity(&entity, config);
sim.take_entity_authority(&entity, &user_key);
```
+ Named, tested, no internals exposed; D11 CellCommandsExt dies.

**Strategy 2 — `SimCoordCtx<'_, E>` accessor exposing `GlobalWorldManager` + `RoomStore`**
Scoped write access; callers call GWM/room ops directly.
− Leaks internal types as public API; callers can re-invent mechanism.

**Tycho recommendation: Strategy 1.**

### 2c. Cyberlith lane

The cyberlith-side migration (`server_access.rs` → policy-on-G1–G6) co-evolves with the naia API.
Options:
- **Tycho owns both worktrees** (per §7.5 of the audit spec): naia feature branch + cyberlith feature branch, repointed naia path-dep, land atomically.
- **Naia-only**: Tycho lands G1–G6 in naia; cyberlith session does the `server_access.rs` migration after.

Connor to decide.

## 3. Sequence (per audit spec §7)

G1 → G2 → G3 → G4 → G5 → G6. Each group = its own design sub-pass + Connor sign-off before impl.

G1/G2/G3 = trunk (driver + un-pub(crate)); G4/G5 collapse onto trunk; G6 (resources) last.

M2 sim-namako BDD specs are RESHAPED — write them against the new API surface (G1–G6 contract), not the leaked shape. M2 follows this mission, not before.

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
