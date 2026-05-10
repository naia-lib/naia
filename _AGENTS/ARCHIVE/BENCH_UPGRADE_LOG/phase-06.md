# Phase 6 — Batched-spawn coalescing audit

**Date:** 2026-04-24
**Status:** ✅ COMPLETE — hypothesis (a) confirmed empirically; coalescing is correct.

---

## Question

Does the wire-message count for a `PaintRect`-style burst of N entities (each with K components) come out to:

- **(a)** N `SpawnWithComponents`, K kinds inline per entity — 0 stray `Spawn` / `InsertComponent` ops; or
- **(b)** N `Spawn` + N×K `InsertComponent` ops — coalescing silently broken on the burst path.

Outcome dictates whether Phase 6 needs a fix or just a firm validation of the spec choice.

## Method

1. **Counter** — added `cmd_emission_counters` (`shared/src/world/local/local_world_manager.rs`) behind `bench_instrumentation`. Increments per `EntityMessageType` exactly when `record_command_written` commits a command to a packet (i.e., `is_writing == true` in `WorldWriter::write_command`). Tracks `Spawn`, `SpawnWithComponents`, `Despawn`, `InsertComponent`, `RemoveComponent`, `Noop`, plus `payload_components` (sum of kinds inlined into all `SpawnWithComponents`).
2. **Audit harness** — `benches/examples/phase6_paint_rect_audit.rs`. Runs `BenchWorldBuilder::new().users(1).entities(0).build()` to steady-state, resets counters, calls `world.paint_rect_spawn_burst(N, K)`, then ticks until the client has received all N entities. Snapshots and asserts the gate.
3. **Cells** — N ∈ {1, 10, 100, 256, 1000}, K ∈ {1, 2}. The 256-K=2 cell mirrors a canonical 16×16 PaintRect with two component kinds (mutable + immutable). 1000-K=2 stress-tests the coalesce path past the canonical scale.

## Result

**Hypothesis (a) holds — coalescing is correct.**

Run via `cargo run --release --example phase6_paint_rect_audit -p naia-benches`:

```
    N   K |   spawn_wc      spawn  despawn   insert_c   remove_c     noop |    payload |    verdict
──────────┼─────────────────────────────────────────────────────────────────┼────────────┼─────────
    1   1 |          1          0        0          0          0        0 |          1 | OK
    1   2 |          1          0        0          0          0        0 |          2 | OK
   10   1 |         10          0        0          0          0        0 |         10 | OK
   10   2 |         10          0        0          0          0        0 |         20 | OK
  100   1 |        100          0        0          0          0        0 |        100 | OK
  100   2 |        100          0        0          0          0        0 |        200 | OK
  256   2 |        256          0        0          0          0        0 |        512 | OK
 1000   2 |       1000          0        0          0          0        0 |       2000 | OK
```

Every cell:
- `spawn_with_components == N` exactly,
- `payload_components == N × K` exactly,
- zero stray `Spawn` / `InsertComponent` / `Despawn` / `Noop` ops.

The `init_entity_send_host_commands` coalesce path (`shared/src/world/host/host_world_manager.rs:115–143`) is the load-bearing primitive: at scope-entry it harvests the entity's full component-kind list and emits one `SpawnWithComponents(entity, kinds)`. Spawn-then-insert sequencing within a single tick is covered because scope evaluation runs after the host-side mutations resolve, so the kind list is complete by the time the command is enqueued.

## Why `spawn/coalesced` looked flat

`benches/benches/spawn/coalesced.rs` calls `BenchWorldBuilder::new().entities(N).build()` (replicates everything during setup, **not** measured) and then times **one** subsequent `world.tick()`. That is steady-state idle cost — it's measuring "level is loaded, first idle game tick" — not the spawn coalesce itself. The Phase 1–4 wins push that cost to the floor (the `tick/idle_matrix` and `update/immutable` benches confirm it), so `spawn/coalesced` and `spawn/burst` look near-identical for the right reason: both are measuring post-replication idle, not the burst.

The new `benches/benches/spawn/paint_rect.rs` measures the burst itself — spawn N, then drive ticks until the client has seen all N — which is the wall-clock latency `PaintRect` actually exposes.

## `spawn/paint_rect` baseline (criterion, 2026-04-24)

```
spawn/paint_rect/entities/100   1.86 ms  →  53.6K elem/s
spawn/paint_rect/entities/1000  24.4 ms  →  41.0K elem/s
spawn/paint_rect/entities/5000   187 ms  →  26.7K elem/s
```

Throughput slope is sub-linear (entities/sec falls from 53.6K → 26.7K as N grows 50×). This is **expected** post-sidequest: the bandwidth accumulator caps each tick's outbound bytes, so a 5000-entity rect drains across many ticks. The headline is the wire-correctness — not the wall-clock — and that's tight.

The slope is a telemetry signal worth noting for Phase 7: any future regression in `paint_rect/5000` >20% should fail the assert-wins gate.

## What I did NOT do

- **No optimization landed.** Phase 6 is measurement-first per the plan; (a) means there's nothing to fix here. The expected win was conditional on (b) and is unrealized — a clean negative result.
- **No `outbound_messages_per_tick` panel** — `cmd_emission_counters` is a finer-grained replacement keyed by message type. The original Phase 1 omission (per `phase-01.md` §"What I did NOT do") is now closed by this counter for the spawn path. Update / message paths can add their own counter panels in a follow-up if a Phase 7 regression needs one.

## Files touched

- `shared/src/world/local/local_world_manager.rs` — `cmd_emission_counters` module + increment in `record_command_written`
- `shared/src/lib.rs` — re-export `cmd_emission_counters` under `bench_instrumentation`
- `benches/src/lib.rs` — `BenchWorld::paint_rect_spawn_burst(n, k)` helper
- `benches/examples/phase6_paint_rect_audit.rs` — audit harness (this is the canonical regression gate; PaintRect coalescing must keep returning N+0+0)
- `benches/benches/spawn/paint_rect.rs` — criterion burst-cost bench
- `benches/benches/main.rs` — register `spawn/paint_rect` group

## Implications for Phase 7

- **Audit harness is a permanent regression gate.** Any change that quietly drops a component from the `SpawnWithComponents` payload, splits a burst into per-component `InsertComponent` ops, or introduces stray `Noop` would surface in this matrix. Wire it into Phase 7's `--assert-wins` panel.
- **`spawn/paint_rect` slope** is the headline wall-clock metric for editor-style bursts. Pin its 1000-entity p50 ≤ 28 ms and 5000-entity p50 ≤ 220 ms in Phase 7's gate.
