# Naia Perf Upgrade — 2-Orders-of-Magnitude Plan

**Status:** in progress — Phases 0–4 complete. **Sidequest (Priority Accumulator): ✅ COMPLETE 2026-04-24** — full A+B+C close-out, all gates green. Phase 5 (spatial scope index) **removed from plan 2026-04-24** per Connor — not pursued. See `_AGENTS/BENCH_UPGRADE_LOG/sidequest-priority-accumulator.md` post-mortem.
**Ref commits:** `4d73ad41` (U×N idle matrix bench) · GDD `862dcab` (LEVEL_SPEC §10 canonical)
**Scope:** this document is the durable plan. Update it as phases land. Do not fork.

## Phase status

| Phase | Status | Landing commit | Log |
|---|---|---|---|
| 0 — Tooling & baseline | ✅ complete | `ed7b4012` | — |
| 1 — Instrument server tick | ✅ complete | `ed7b4012` | `phase-01.md` |
| 2 — Immutable matrix | ✅ complete | `ed7b4012` | `phase-02.md` |
| 3 — Kill O(U·N) idle | ✅ complete (189× at 16u_10000e) | `db1b706d` | `phase-03.md` |
| 4 — Immutable skip idle | ✅ complete (21× at 16u_10000e imm) | TBD | `phase-04.md` |
| Sidequest — Priority Accumulator | ✅ complete (A+B+C closed, all gates green) | `b710ca4e` + 2026-04-24 | `sidequest-priority-accumulator.md` |
| 4.5 — Mutable resend-window spike | ✅ closed by absorption (sidequest Phase A bandwidth cap) | `b710ca4e` | `phase-04.5.md` |
| 6 — Coalesce audit | ⏸️ pending | — | — |
| 7 — Regression gate + close-out | ⏸️ pending | — | — |

---

## 1. What we're optimizing for

Cyberlith's canonical model (GDD §10) is one **immutable Naia entity per tile**. Target:

- **Tile count per level:** up to 2^16 = 65,536 immutable entities.
- **Players per server:** far above 16. Plan for 64+ as the near-term capacity line; the protocol must not collapse at 128.
- **Tick rate:** 25 Hz → **40 ms / tick budget**, server-side.
- **Session shape:** steady state is mostly idle ticks (nothing changes) + sparse mutation bursts (unit moves, combat) + edit-session spawn/despawn flurries.

## 2. Where we are (measured, 2026-04-24)

All numbers from `cargo bench -p naia-benches --bench naia`. Medians.

### Idle-tick matrix — `tick/idle_matrix`

| U \ N | 100    | 1,000  | 10,000  |
|-------|--------|--------|---------|
| 1     | 142 µs | 1.84 ms| 26.6 ms |
| 4     | 500 µs | 6.25 ms| 84.9 ms |
| 16    | 1.96 ms| 23.7 ms| **299 ms** |

- **Scaling:** linear in `U`, linear in `N`. Idle is **O(U·N)**.
- **Per-cell cost:** 1.2–2.7 µs per user·entity; mid-range ≈ 1.5 µs.
- **Win-2 (O(1) idle) is NOT holding** on the server-side scan. This is the #1 optimization target.

### Immutable dispatch — `update_immutable`

- `mutable_idle @ N=10K`:   29.23 ms
- `immutable_idle @ N=10K`: 14.27 ms
- **2.05× faster**, but **still linear in N** — the save is on mutation dispatch, not the idle scan.

### Other measured costs

| Bench | Result | Implication |
|---|---|---|
| `tick/scope_enter` | 3.1 µs/entity (linear) | scope-entry at 65K tiles = ~200 ms per join |
| `tick/scope_exit` | 4.7 µs/entity (linear) | disconnect at 65K tiles = ~310 ms |
| `update/mutation/single` | 23.5 µs per (user·mutation·tick) | bandwidth-bound, scales as U·K |
| `spawn/coalesced vs burst @ N=1K` | ~10% win only | coalesce is weaker than expected |
| `wire_bandwidth 4u_1000m` | 22.5 ms | wire is fine; server tick is not |

### Capacity gap (immutable-tile assumption, ~0.91 µs/cell)

| U | N=10K | N=65K |
|---|---|---|
| 16 | 146 ms (**3.6×** budget) | **954 ms (24×)** |
| 64 | 582 ms (15×) | 3.8 s (95×) |
| 128 | 1.16 s (29×) | 7.6 s (**191×**) |

## 3. Target

**Bring `tick/idle_matrix` down by ≥100× in the dominant cells.**

Concrete numeric gates (all at 25 Hz / 40 ms budget):

| Cell | Today | Target | Ratio |
|---|---|---|---|
| `16u_10000e` idle (mutable) | 299 ms | ≤ 3 ms | 100× |
| `16u_10000e` idle (immutable) | ~146 ms* | ≤ 1.5 ms | 100× |
| `1u_10000e` idle (mutable) | 26.6 ms | ≤ 0.3 ms | 100× |
| `16u_65536e` idle (immutable) | ~954 ms* | ≤ 10 ms | 100× |
| `tick/scope_enter @ 10K` | 31.2 ms | ≤ 3 ms | 10× (scope is less hot but still linear) |

*(\*) extrapolated; to be re-measured after the immutable variant of the matrix ships (Phase 1.b)*.

A 100× reduction makes 64-player × 65K-tile sessions fit the tick budget with headroom. Anything less still leaves Cyberlith's canonical shape unreachable.

## 4. Guiding principles

1. **Measure first, change second.** Every phase opens with a profile capture (`samply` / `cargo flamegraph`), not a guess. No PR ships without a criterion before/after for the affected cell.
2. **No valgrind dependency.** iai suite stays in the tree and is maintained, but perf work must be unblocked without callgrind. Use `samply`, `cargo flamegraph`, `perf stat`, and criterion comparative mode as the full toolkit. When a home-machine iai run becomes available, it becomes a cross-check, not a blocker.
3. **The existing test suite is the contract.** Any optimization that breaks `cargo test --workspace` or one of the BDD contracts in `resolved_plan.json` is rolled back. Regression protection lives in `test/bench_report --assert-wins` *and* the behavioral test suite. Both must stay green.
4. **Optimize the observable, not the theoretical.** `tick/idle_matrix` is the scoreboard. If a change improves a microbench but doesn't move the matrix, it doesn't count.
5. **One phase at a time.** Each phase is a separate PR with its own before/after criterion diff committed into `_AGENTS/BENCH_UPGRADE_LOG/phase-NN.md`. This gives us reversibility and a durable audit trail.

## 5. Phase plan

### Phase 0 — Tooling & baseline (no code changes to Naia runtime) ✅ COMPLETE

**Goal:** make valgrind-free profiling trivial, and freeze a baseline we can diff against for the rest of the project.

Tasks:

- [ ] Install + document `samply` (`cargo install samply`) and `cargo-flamegraph` (`cargo install flamegraph`) in `_AGENTS/PROFILING.md`. Add one-line "how to profile bench X" recipe.
- [ ] Freeze baseline: `cargo bench -p naia-benches --bench naia -- --save-baseline perf_v0` (criterion stores these under `target/criterion/*/perf_v0/`). All subsequent phases diff against `perf_v0` with `--baseline perf_v0`.
- [ ] Add `cargo bench -p naia-benches --bench naia -- --save-baseline perf_vN` step to each phase's completion checklist.
- [ ] Extend `test/bench_report --assert-wins` with concrete thresholds derived from the measured baseline (today it warns without hard numbers for Win-1). Fail-loud if any phase regresses *another* cell while improving its target.

Success: `samply record` on an idle-tick bench produces a flamegraph; `naia-bench-report --assert-wins --baseline perf_v0` runs green.

---

### Phase 1 — Instrument the server tick loop ✅ COMPLETE

**Goal:** make the *cause* of O(U·N) idle visible, without changing behavior.

Hypothesis: the server's `send_all_packets` loop iterates every `user_connection` × every entity-in-scope every tick, regardless of dirtiness. Flamegraph will confirm or refute.

Tasks:

- [ ] Run `samply` against the `tick/idle_matrix/u_x_n/16u_10000e` bench. Save SVG flamegraph to `_AGENTS/BENCH_UPGRADE_LOG/phase-01-flamegraph-before.svg`.
- [ ] Identify the hottest functions. Expected culprits (to be confirmed, not assumed): `update_entity_scopes`, `WorldChannel::process_updates`, `MutReceiver::collect_component_updates`, or the `UserConnection` tick walk.
- [ ] Add a `Debug`-only `PerTickCounters` struct to `Server`:
  - `touched_entities_per_tick`
  - `scope_checks_per_tick`
  - `outbound_messages_per_tick` (already partially there via `outgoing_bytes_last_tick`)
  - `idle_users_per_tick` (users with zero outbound messages this tick)
- [ ] Expose via `Server::last_tick_counters()` → read from `BenchWorld`. Assert `touched_entities_per_tick == 0` for a fully-idle tick in the matrix (this will **fail today** — that's the diagnostic).
- [ ] **Test safety:** counters are behind a compile-time cfg flag (`cfg(feature = "bench_instrumentation")`), default off, so the existing test suite is untouched. Gate the `BenchWorld` accessor on the same feature.

Success: flamegraph + counters localize the O(U·N) cost to a specific function. We know *what* to fix before writing the fix.

**Deliverable:** `_AGENTS/BENCH_UPGRADE_LOG/phase-01.md` — flamegraph, counter readouts at each matrix cell, one-paragraph diagnosis naming the function(s) responsible.

---

### Phase 2 — Add immutable-tile matrix coverage ✅ COMPLETE

**Goal:** the existing matrix is all-mutable. Tiles are immutable. Measure the *actual* target surface.

Tasks:

- [ ] Add `.immutable()` variant to `tick/idle_matrix` — 9 more cells (U ∈ {1,4,16} × N ∈ {100,1000,10000}, all immutable entities).
- [ ] Confirm the 2.05× Win-5 discount holds across the whole matrix (today it's only measured at U=1, N=10K).
- [ ] Save `perf_v2` baseline.

Success: the capacity table in §2 is re-grounded in measured numbers, not extrapolation.

**Deliverable:** update §2 of this doc with actual immutable cells.

---

### Phase 3 — Kill O(U·N) idle (the main course) ✅ COMPLETE (2026-04-24, `db1b706d`)

Gate met at 189× (302ms → 1.60ms). See `_AGENTS/BENCH_UPGRADE_LOG/phase-03.md` for attribution between dirty-push (real ~20×) and bench-methodology fix (swap-artifact ~10×).

**Goal:** server idle-tick becomes O(dirty ∩ scope), not O(users × scope).

Hypothesis: the per-tick work decomposes into:

- Per-entity: `MutReceiver::collect_component_updates` walks every component of every entity.
- Per-user: `UserConnection` re-checks every entity in scope for newly-dirty components.

Both should be replaced by a **push-based dirty set**: when a component is mutated, it pushes a change record into a tick-scoped dirty queue; the tick consumer drains the queue once, then dispatches per-user based on scope intersection. Zero mutations → zero queue entries → O(U) tick cost (just "are you still alive?" heartbeats per user, plus any RTT/keepalive work), not O(U·N).

Tasks:

- [ ] From Phase 1's flamegraph, confirm the hotspot matches the hypothesis. If it doesn't, **stop and re-plan** — the rest of this phase is hypothesis-conditioned.
- [ ] Introduce `server::world_server::TickDirtyQueue` (or similar) — a per-tick SmallVec of `(EntityHandle, ComponentKind)` pushed by mutation paths and drained once at `send_all_packets` start.
- [ ] Refactor `WorldChannel::process_updates` to consume this queue instead of scanning.
- [ ] Scope-intersection: for each dirty (entity, component), iterate only the users whose scope contains that entity. Requires a reverse index `entity → set<UserKey>` which is cheap to maintain at scope-enter / scope-exit (incremental).
- [ ] Heartbeat/keepalive logic stays O(U) but should NOT touch the entity set.
- [ ] **Test safety:** run `cargo test --workspace` + `cargo test -p naia-benches` after every functional change. The BDD contract suite in `test/` is the backstop — all 15 contracts must stay passing (`b465c32f` is the last green reference). Any red contract blocks the merge.
- [ ] **Regression gate:** `cargo bench -p naia-benches --bench naia -- tick --baseline perf_v0` must show ≥10× speedup on the 16u_10000e cell to proceed. Anything less means the refactor is wrong, not that we need to keep going.

Expected win: **50–100×** on idle cells at N≥1K. This phase alone is most of the budget.

**Deliverable:** `_AGENTS/BENCH_UPGRADE_LOG/phase-03.md` — criterion diff vs `perf_v0`, flamegraph after, before/after matrix table, list of touched files.

---

### Phase 4 — Immutable-bypass extends to idle scan ✅ COMPLETE (2026-04-24)

Gate met at **21×** on 16u_10000e_imm (1.05 ms → 49 µs; per-receiver idle cost ≈ 0.3 ns, effectively noise). See `_AGENTS/BENCH_UPGRADE_LOG/phase-04.md`.

The landing fix was not an immutable partition but a **ReliableSender fast-path** (`collect_messages` short-circuits when nothing is due for resend via cached `min_last_sent` + `has_unsent`). Attacking that hotspot made the immutable path contribute zero work to idle — achieving the phase goal in spirit. The mutable pipeline, however, revealed a latent periodic spike — tracked as Phase 4.5 below.

**Goal:** immutable entities contribute exactly zero work to idle ticks.

Hypothesis: today's immutable-component path skips `MutChannel`/`MutReceiver` allocation (Win-5 in the benchmark series), but the server's idle scan still *visits* immutable entities to check "nothing changed." It shouldn't — they can't change.

Tasks:

- [ ] Partition `WorldChannel`'s entity set into `mutable_entities` and `immutable_entities`. Idle sweep touches only `mutable_entities`.
- [ ] Spawn-time classification: `SpawnWithComponents` with all immutable components → entity lands in `immutable_entities` and never enters the dirty-checking path.
- [ ] Despawn/spawn still work in O(1) per event — they push into the dirty queue from Phase 3.
- [ ] **Test safety:** add a bench-only test that mutates an immutable entity via the client path and asserts the mutation is rejected (it should already be rejected at the component level — this pins the invariant in bench_instrumentation mode).
- [ ] Criterion gate: immutable cells of the matrix (Phase 2) must show **≥3× speedup on top of Phase 3**, specifically for the per-entity idle-scan coefficient.

Expected win: **another 2–3×** on immutable-heavy workloads (i.e., all tile-dominant Cyberlith sessions).

**Deliverable:** `_AGENTS/BENCH_UPGRADE_LOG/phase-04.md`. ✅ landed.

---

### Sidequest — Priority Accumulator (interrupts the main track)

**Opened:** 2026-04-24, after the Phase 4.5 spike surfaced on `idle_distribution.rs`.

Glenn Fiedler's **priority accumulator** is a long-standing backlog item and (per Connor) an absolute necessity for Naia to be production-ready. It is a **sender-side** pacing layer (applies symmetrically to server-outbound and client-outbound traffic, since Naia supports client-authoritative messages / requests / responses / entities) that (a) accumulates a priority score per replicated item every tick, (b) selects items up to a per-tick bandwidth budget, and (c) resets accumulators for sent items — producing self-paced outbound traffic that survives bursts.

It is believed to be the natural fix for the Phase 4.5 mutable resend-window spike (10K-item resend burst becomes N ticks of steady-state load at budget `B`). Research will verify or refute; if verified, Phase 4.5 folds into this sidequest.

See `_AGENTS/PRIORITY_ACCUMULATOR_SIDEQUEST.md` for scope, research questions, deliverables, and scope discipline. The sidequest produces two artifacts (`PRIORITY_ACCUMULATOR_RESEARCH.md`, `PRIORITY_ACCUMULATOR_PLAN.md`), both approved before any code lands.

Scope surfaces under survey:

- Component update messages (original target)
- `UnorderedReliable` / `OrderedReliable` entity commands (spawn-burst case)
- Plain Naia messages (`ChannelSender`)
- Request / response (built on messages)

**Blocks:** Phase 4.5. Phase 6 and 7 are independent and can be resequenced if useful.

---

### Phase 4.5 — Mutable resend-window spike ✅ CLOSED BY ABSORPTION (2026-04-24)

**Outcome:** Closed by Priority Accumulator Sidequest Phase A (`b710ca4e`). Post-sidequest `idle_distribution` shows every mutable cell `OK` (max/p50 ≤ 6.6×, down from 2741–4033×). See `_AGENTS/BENCH_UPGRADE_LOG/phase-04.5.md` for the full before/after matrix and attribution.

---

### Phase 4.5 (original goal — retained for history)

**Goal:** eliminate the periodic ~17-tick latency spike on mutable idle ticks. No cell of the matrix may exceed `p99 × 10` (i.e., `idle_distribution` reports no `SPIKE`).

**Status:** discovered during Phase 4 via the new `idle_distribution.rs` harness. Scope is strictly this pre-existing bug (not a new optimization) — per Connor's rigor mandate, no hand-waving past real bugs.

**Evidence (2026-04-24, `cargo run --release --example idle_distribution`):**

| cell              | p50     | max        | max/p50  | flag      |
|-------------------|---------|------------|----------|-----------|
| 1u_10000e_mut     | 3.5 µs  | 10.5 ms    | 3007×    | **SPIKE** |
| 4u_10000e_mut     | 8.4 µs  | 34.0 ms    | 4033×    | **SPIKE** |
| 16u_10000e_mut    | 31.6 µs | 86.5 ms    | 2741×    | **SPIKE** |

Spike cadence is cyclic: ticks +11, +12, +13, +28, +29, +30, …, every ~17 ticks ≈ 850 ms ≈ `1.5 × 567 ms` (default rtt, `rtt_resend_factor = 1.5`). Immutable cells are clean (`max/p50 ≤ 6×`). Aligns with the reliable-sender **resend window** cadence.

**Narrowed hypothesis (to prove or refute):** Phase 4's `ReliableSender` fast-path neutralized the sender-side scan, so the remaining mutable-only periodic work lives elsewhere on the resend boundary — most likely `handle_dropped_update_packets` (`shared/src/world/update/entity_update_manager.rs:~126`), which iterates the `sent_updates` HashMap per-tick and re-queues update work for dropped packets. At 10K mutable entities this could be the O(N) spike.

**Tasks:**

- [ ] Extend `phase4_tick_internals.rs` (or new probe) to instrument the update-manager dropped-packet path + scope-check + outbound-packet-assembly sub-phases during the spike tick. Capture ns per sub-phase.
- [ ] Confirm or refute the `handle_dropped_update_packets` hypothesis with hard data before writing any fix. If not there, follow the probe to the real hotspot.
- [ ] Fix the root cause (likely: a dirty-state cache analogous to Phase 3/4 — bookkeeping should be O(dropped), not O(sent)).
- [ ] **Re-run `idle_distribution` until every mut cell reports `OK`** (`max/p50 ≤ 10×`). No hand-waving, no "close enough" — spike gates to zero.
- [ ] **Regression gate:** immutable cells must not regress. `16u_10000e_imm` p50 stays ≤ 60 µs. Idle_distribution output is committed alongside the fix.
- [ ] Test safety: `cargo test --workspace`, namako integration tests, and `namako gate --specs-dir test/specs` must all stay green.

**Expected win:** surfaces as cleaner tail at any mutable-cell; headline p50s shouldn't change much (already excellent post-Phase-3/4), but p99/max and mean collapse to p90 territory.

**Deliverable:** `_AGENTS/BENCH_UPGRADE_LOG/phase-04.5.md` — sub-phase probe readout on a spike tick, root-cause narrative, before/after `idle_distribution` matrix with all cells `OK`, files touched.

---

### Phase 6 — Batched-spawn coalescing audit

**Goal:** `PaintRect` → one `SpawnWithComponents` per tile, not per component, and not re-sent as diffs.

Hypothesis: today's `spawn/coalesced @ N=1K` is only 10% faster than `spawn/burst`. For a bulk-edit path this is suspiciously small. Either:

(a) the coalescing is fine but the bench measures steady-state not the coalesce itself (documented limitation in `iai/benches/spawn_coalesced.rs`); or

(b) the coalescing *is* silently missing a batch path that `PaintRect` would hit.

Tasks:

- [ ] Add a bench that mimics `PaintRect`: issue N spawns in a single server tick, no ticks between them, measure first outbound tick.
- [ ] Instrument `outbound_messages_per_tick` from Phase 1 — `PaintRect` of 256 tiles should produce **256 messages** (one SpawnWithComponents each), not 256×K for K components.
- [ ] If the number is worse, trace where.

Expected win: depends on the finding; if (b), potentially 10× on edit paths. If (a), zero runtime win but validates the spec choice firmly.

**Deliverable:** `_AGENTS/BENCH_UPGRADE_LOG/phase-06.md`.

---

### Phase 7 — Continuous regression gate + final measurement

**Goal:** the 100× gain is permanent, protected by the test suite, and reproducible from a clean clone.

Tasks:

- [ ] Harden `test/bench_report --assert-wins` with all the thresholds from this doc baked in. Any PR that regresses a matrix cell by >20% fails the assert.
- [ ] Run the full bench suite vs `perf_v0`. Publish the final comparative table in this doc's §2.
- [ ] Update the LEVEL_SPEC §10.3 open-performance-questions section to state the realized capacity envelope.
- [ ] When home-machine iai is available: re-run iai benches and confirm instruction counts moved in the same direction as criterion wall-clock. This is a correlation check, not a gate.

Success criteria (all must hold):

- `tick/idle_matrix/16u_10000e` ≤ 3 ms.
- `tick/idle_matrix/16u_10000e/immutable` ≤ 1.5 ms (once Phase 2 lands immutable cells).
- `cargo test --workspace` green.
- All 15 BDD contracts green (`b465c32f` baseline or later).
- `naia-bench-report --assert-wins --baseline perf_v0` green.
- `_AGENTS/BENCH_UPGRADE_LOG/phase-0{1,2,3,4,4.5,6}.md` present with before/after artifacts.

---

## 6. Risk register

| Risk | Mitigation |
|---|---|
| Phase 3 refactor breaks a subtle ordering guarantee in `process_updates` | Snapshot pre-phase behavior via a scope-replay test. Any byte-diff in the outbound stream at same inputs blocks merge. |
| Instrumentation (Phase 1) leaks into release builds | All counters behind `cfg(feature = "bench_instrumentation")`, default off. CI verifies release builds exclude the feature. |
| 100× goal is unreachable even with all phases | Phase 1 flamegraph will tell us this *before* we refactor. If the budget doesn't fit, we escalate to protocol-level changes (delta batching across ticks, protocol v2) — out of scope for this plan but a known fallback. |
| iai coverage stays behind until home-machine | Acceptable: criterion is the source of truth for wall-clock. iai becomes a cross-check when available. Plan does not block on it. |

## 7. Non-goals for this plan

- **New transport layer.** UDP/QUIC stays as-is. Wire-bandwidth benches show wire isn't the bottleneck.
- **Protocol version bump.** All changes are server-internal and preserve wire format. If Phase 3 or 5 requires a wire-format change, that's a separate out-of-scope plan.
- **Client-side optimizations.** Idle-tick matrix is server-side; clients are faster per-entity than servers today. Revisit only if a later profile shows client-side bottlenecks.
- **Immutable-component API expansion.** The existing immutable kind is sufficient for tiles; no new trait surface needed here.

## 8. How to resume this plan mid-stream

1. Read `_AGENTS/BENCH_UPGRADE_LOG/` — each phase file says what shipped and what measured.
2. Find the highest-numbered phase with a shipped `.md`. The next phase is the current work.
3. Before writing any code, re-read §4 principles. Flamegraph first.
4. Never delete completed phase files. They're the record.
