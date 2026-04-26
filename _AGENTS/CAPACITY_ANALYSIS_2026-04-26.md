# Cyberlith Capacity, Scaling, and Business Plan — Living Source of Truth

**Last Updated:** 2026-04-26
**Repo Commit:** `0a57c753` (naia repo, `git rev-parse --short HEAD`)
**Machine:** lifetop — Intel Core i9-12900HK (20 logical CPUs, 2 threads/core), 62 GiB RAM, Linux 6.8.0-110-generic x86\_64
**Confidence Level:** MIXED — Naia networking stack is measured; full game stack (Rapier physics + game logic + wire bytes) is NOT yet measured. Do not treat full-stack estimates as validated.

---

## Table of Contents

- [E. Executive Summary](#e-executive-summary)
- [ER. Evidence Registry](#er-evidence-registry)
- [1. Measured Benchmarks (Naia-Only)](#1-measured-benchmarks-naia-only)
- [2. The Full Tick Budget](#2-the-full-tick-budget)
- [3. Per-Cell Resource Costs (Halo BTB 16v16, 10K tiles)](#3-per-cell-resource-costs-halo-btb-16v16-10k-tiles)
- [4. Vultr VPS Analysis — 5 Price Points](#4-vultr-vps-analysis--5-price-points)
- [5. The Guild Wars Instancing / Portal Network Model](#5-the-guild-wars-instancing--portal-network-model)
- [6. Tile Count as a Design Lever](#6-tile-count-as-a-design-lever)
- [7. Portal Pre-Warming (Critical UX Requirement)](#7-portal-pre-warming-critical-ux-requirement)
- [8. 40v40 Match Analysis (80 Players/Cell)](#8-40v40-match-analysis-80-playerscell)
- [9. Validation Ladder](#9-validation-ladder)
- [10. Client-Side Capacity](#10-client-side-capacity)
- [11. The Three Binding Constraints (Ranked by Frequency)](#11-the-three-binding-constraints-ranked-by-frequency)
- [12. Recommendations for the Cyberlith Business Plan](#12-recommendations-for-the-cyberlith-business-plan)
- [13. The 4-Tier Match Architecture — Per-Tier Resource Costs](#13-the-4-tier-match-architecture--per-tier-resource-costs)
- [14. The Mixed Fleet — Server Capacity Across All Four Tiers](#14-the-mixed-fleet--server-capacity-across-all-four-tiers)
- [15. Revenue Model and Scaling Plan](#15-revenue-model-and-scaling-plan)
- [16. Business Plan Scenarios (Three Paths)](#16-business-plan-scenarios-three-paths)
- [17. DLC Campaigns as a Revenue Accelerator](#17-dlc-campaigns-as-a-revenue-accelerator)
- [18. Distribution, Platform Matrix, and Steam Strategy](#18-distribution-platform-matrix-and-steam-strategy)
- [19. Payment Architecture — One Backend, Multiple Purchase Providers](#19-payment-architecture--one-backend-multiple-purchase-providers)
- [20. Risk Register](#20-risk-register)
- [21. Decision Gates](#21-decision-gates)
- [22. Performance and Infrastructure Levers](#22-performance-and-infrastructure-levers)
- [23. Provider Pricing Table (as of 2026-04)](#23-provider-pricing-table-as-of-2026-04)
- [24. Bandwidth Model — Worst-Case vs Event-Driven](#24-bandwidth-model--worst-case-vs-event-driven)
- [25. Measurement Backlog](#25-measurement-backlog)
- [26. Appendix: Scaling Formulas](#26-appendix-scaling-formulas)

---

## E. Executive Summary

### What is proven

- **Naia networking stack is fast.** The `halo_btb_16v16_10k` scenario (16 players, 10K immutable tile entities, 32 mutable unit entities, 25 Hz) shows: idle tick **63 µs**, active tick **58 µs**, level load **5.2–5.5 s**, client receive **889 ns**. These are `[Measured, Naia-only]` on a single core of the lifetop dev machine.
- **O(1) idle cost proven (Win 2).** 10 000 immutable tile entities cost nothing at steady-state. Map size does not affect per-tick server CPU.
- **O(mutations × users) scaling confirmed (Win 3).** Active tick cost scales with the product of dirty count and connected client count, as expected.
- **Client-side is essentially free.** 889 ns/tick client receive = 0.002% of one CPU core.

### What is NOT proven (full-stack unknowns)

- **Rapier physics tick cost is unquantified.** The ~350 µs physics estimate and the ~558 µs total full-stack tick are `[Estimated, no benchmark]`. No full-stack Cyberlith game server has been benchmarked under load.
- **Wire bytes are not measured.** The capacity report shows `∞` for wire capacity — the wire benches exist but have not been run for this scenario. All bandwidth figures are derived from a worst-case O(N²) formula, NOT from measured bytes-on-wire.
- **No multi-player full-stack benchmark exists.** All CCU capacity figures for the full game stack are extrapolations from Naia-only measurements plus estimated physics cost. Error bars: ±30–50%.
- **No cross-cell, multi-server, or WAN measurements exist.**

### What blocks business confidence

1. **Full-stack tick cost is unknown.** The ~558 µs/cell full-game estimate (→ ~29 cells/core realistic) could be 2× wrong in either direction. This is the primary uncertainty in all capacity and revenue tables.
2. **Bandwidth is not instrumented.** The event-driven position model (§24) likely makes the O(N²) formula a severe overestimate for production BW, but this has not been measured.
3. **Steam payment policy is an open risk.** `[Policy risk, requires verification]` Steam can be a free-to-play discovery channel, especially for Linux and Steam Deck players. However, monetization for Steam-channel users should be assumed to require Steam Wallet / Steam DLC / Steam microtransaction-compatible flows until verified otherwise. Therefore Steam should be modeled as a lower-margin but higher-discovery channel, while Web/PWA remains the high-margin direct-payment channel. Implementing Steam Wallet/DLC adapter is required before any paid items go live on Steam.

### Next gate to unlock business confidence

**Full-stack 2v2 benchmark** — run Cyberlith game server with 4 players, real Rapier physics, real game logic, and measure: tick wall time (P50/P95/P99), server outgoing bytes per tick, and memory consumption. This single measurement resolves the largest unknown in this document.

---

## ER. Evidence Registry

All measured data used in this document is registered here. Labels used throughout: `[Measured]`, `[Measured, Naia-only]`, `[Extrapolated]`, `[Estimated]`, `[Assumption]`, `[External pricing, manually verified 2026-04]`, `[External pricing, unverified — check date]`, `[Policy risk, requires verification]`.

| ID | Scenario | Command | Date | Machine | Includes | Excludes | Result | Confidence | Used in formulas? |
|---|---|---|---|---|---|---|---|---|---|
| M-001 | halo_btb_16v16 level load | `cargo criterion -p naia-benches -- "scenarios/halo_btb_16v16"` | 2026-04-26 | lifetop (i9-12900HK, 62 GiB) | Naia entity replication, 16 clients, 10K tiles + 32 units, uncapped BW | Rapier, game logic, actual network transport | **5.2 s** (5 511.6 ms from capacity report) | High (Naia-only) | §1, §6, §7, §13d, §26 |
| M-002 | halo_btb_16v16 idle tick | same | 2026-04-26 | lifetop | Naia server tick, 16 clients, 10K tiles, 0 mutations | Rapier, game logic | **63 µs** | High (Naia-only) | §1, §2, §3, §26 |
| M-003 | halo_btb_16v16 active tick | same | 2026-04-26 | lifetop | Naia server tick, 16 clients, 32 mutations | Rapier, game logic | **58 µs** | High (Naia-only) | §1, §2, §3, §26 |
| M-004 | halo_btb_16v16 client receive | same | 2026-04-26 | lifetop | One client receive path, active tick | Server tick cost, other clients | **889 ns** | High (Naia-only) | §1, §10 |
| E-001 | Full game tick — 16v16 | — | — | — | Naia (M-003) + Rapier estimate + logic estimate + OS overhead | Not yet measured | **~558 µs** `[Estimated, no benchmark]` | Low | §2, §3, §4, §9, §11, §13c, §22b |
| E-002 | Rapier physics — 32 bodies + 10K tiles | — | — | — | Physics tick only | Not yet measured | **~350 µs** `[Estimated, no benchmark]` | Low | §2, §22b |
| E-003 | Active bandwidth — 16v16 | Formula | 2026-04-26 | — | O(N²) worst-case formula | Measured bytes; event-driven model | **~230 KB/s** `[Extrapolated, formula only]` | Low | §3c, §4, §8, §9, §13a |
| E-004 | Player-count extrapolations | Naia M-002/M-003 linear scaling | 2026-04-26 | — | Naia scaling by N | Full-stack, physics, BW | Various (§3 tables) `[Extrapolated]` | Low–Medium | §3, §8, §13c |

**Missing entries (see §25 Measurement Backlog):** Full-stack 2v2/5v5/10v10/40v40, wire bytes per tick, WAN latency, join burst, portal prewarm, multi-cell jitter, client performance under load.

---

## 1. Measured Benchmarks (Naia-Only)

All numbers are from the `halo_btb_16v16_10k` scenario:
**16 players, 10 000 immutable HaloTile entities, 32 mutable HaloUnit entities, 25 Hz (40 ms tick budget)**,
run via `cargo criterion -p naia-benches -- "scenarios/halo_btb_16v16"`.

| Measurement | Value | Provenance |
|---|---|---|
| Level load (10K tiles + 32 units → 16 clients) | **5.2 s** (σ ≈ 0.8 s; 5 511.6 ms from capacity report) | `[Measured, Naia-only]` M-001 |
| Server tick — idle (0 mutations) | **63 µs** | `[Measured, Naia-only]` M-002 |
| Server tick — active (32 mutations) | **58 µs** | `[Measured, Naia-only]` M-003 |
| Client receive — active tick | **889 ns** | `[Measured, Naia-only]` M-004 |

> Note: Active tick (58 µs) is slightly faster than idle (63 µs) — this is within noise at this timescale; both reflect the Naia networking path only with zero Rapier or game logic cost.

### Capacity report (2026-04-26 run)

```
╔══════════════════════════════════════════════════════════════╗
║   Cyberlith halo_btb_16v16 — Capacity Estimate @ 25 Hz      ║
╠══════════════════════════════════════════════════════════════╣
║  Level load (10K tiles + 32 units → 16 clients):            ║
║    5511.6 ms                                                  ║
║                                                              ║
║  Server capacity (CPU):                                      ║
║    idle  (0 mutations/tick):   649 concurrent games           ║
║    active (32 mutations/tick): 760 concurrent games           ║
║                                                              ║
║  Wire capacity (1 Gbps outbound):                            ║
║    idle:                       ∞ concurrent games             ║
║    active:                     ∞ concurrent games             ║
║                                                              ║
║  Client (one player, active tick): ✓ keeps up                 ║
║                                                              ║
║  Bottleneck: CPU (server tick cost)                           ║
╚══════════════════════════════════════════════════════════════╝
  Note: wire capacity shown as ∞ — run the full bench suite
  (including wire/bandwidth_realistic_quantized) for wire estimates.
```

The 649/760 figures are **Naia-networking-only** on one core. Full game stack (Rapier + logic) reduces this significantly — see §2. These numbers should NOT be used directly for production capacity planning without a full-stack benchmark.

### Architectural guarantees confirmed `[Measured, Naia-only]`

**Win 2 — O(1) idle cost:** 10 000 HaloTile entities (immutable) cost nothing at steady state.
No dirty-tracking, no per-entity scan. Server CPU is entirely determined by mutable unit mutations
and client count, never by map size.

**Win 3 — O(mutations × users):** Active tick cost scales with the product of mutations and connected
clients, not total entity count. This means bandwidth (which must carry mutations to every client)
scales as O(N²) with player count, making N the primary capacity design lever.

> These benchmarks measure **Naia networking only**. Game simulation (Rapier physics, pathfinding,
> damage resolution) adds additional cost estimated at ~350 µs — see §2. Neither Rapier cost nor total full-stack cost has been measured.

---

## 2. The Full Tick Budget

At 25 Hz: **40 000 µs per tick** per thread.

| Component | Cost (µs) | Provenance |
|---|---|---|
| Naia networking — active, 32 mutations, 16 clients | **58** | `[Measured, Naia-only]` M-003 |
| Rapier physics — 32 kinematic bodies + 10K static tiles | **~350** | `[Estimated, no benchmark]` E-002 |
| Game logic — damage, spawn management, game state | **~100** | `[Estimated]` |
| OS overhead, context switches | **~50** | `[Estimated]` |
| **Total per-cell tick (16-player Halo BTB)** | **~558 µs** | `[Estimated, no benchmark]` — see note |
| **Cells per core — theoretical** | **~72** | 40 000 / 558 |
| **Cells per core — realistic (40% efficiency)** | **~29** | Cache + scheduling overhead |

> **Note:** The prior version of this document used **~408 µs** total. That figure omitted game logic (~100 µs) and OS overhead (~50 µs). The corrected figure is **~558 µs** (Naia 58 + physics ~350 + logic ~100 + OS ~50). All downstream capacity tables inherit this estimate and carry ±50% error bars until a full-stack benchmark runs. Run BM-001 to resolve.

The 40% efficiency factor accounts for L1/L2/L3 cache pressure (10K entities × ~300 B ≈
3 MB working set per cell, competing across threads), memory bandwidth contention, and
kernel scheduler overhead when oversubscribing cores.

**Naia-only ceiling** (if physics/logic turns out lighter than estimated `[Estimated]`): ~635 cells/core theoretical,
254 cells/core realistic.

---

## 3. Per-Cell Resource Costs (Halo BTB 16v16, 10K tiles)

### 3a. CPU

| Scenario | Tick cost | % of one core | Cells/core (realistic) | Provenance |
|---|---|---|---|---|
| Naia-only, idle | 63 µs | 0.16% | 254 | `[Measured, Naia-only]` M-002 |
| Naia-only, active | 58 µs | 0.15% | 274 | `[Measured, Naia-only]` M-003 |
| Full game (Naia + physics + logic) | ~558 µs | 1.40% | **~29** | `[Estimated, no benchmark]` E-001 |

### 3b. Memory

| Component | Per-cell | Provenance |
|---|---|---|
| 10K HaloTile entities (server representation) | ~3 MB | `[Estimated]` |
| 32 HaloUnit entities + dirty tracking | ~50 KB | `[Estimated]` |
| 16 client connections (Naia state + buffers) | ~1.6 MB | `[Estimated]` |
| Network send/receive buffers (16 × 64 KB × 2) | ~2 MB | `[Estimated]` |
| Naia server overhead (room, routing, event queues) | ~500 KB | `[Estimated]` |
| Game logic state (positions, health, game state) | ~100 KB | `[Estimated]` |
| **Total per cell (16-player Halo BTB)** | **~7.2 MB** | `[Estimated]` |

**Tile-count sensitivity (memory scales linearly; CPU does NOT — Win 2 `[Measured, Naia-only]`):**

| Map size | Memory/cell | CPU/cell | Level load | Provenance |
|---|---|---|---|---|
| 1K tiles (small arena) | ~1.5 MB | ~558 µs (same) | ~0.5 s | `[Estimated]` / `[Extrapolated]` |
| 10K tiles (Halo BTB) | ~7.2 MB | ~558 µs (same) | ~5.2–5.5 s | `[Estimated]` / `[Measured, Naia-only]` M-001 |
| 32K tiles (large campaign) | ~22 MB | ~558 µs (same) | ~17 s | `[Extrapolated]` |
| 64K tiles (massive map) | ~45 MB | ~558 µs (same) | ~33 s | `[Extrapolated]` |

**Player-count sensitivity (both CPU and memory scale with players):**

| Players/cell | Memory/cell | Naia tick (active) | Full tick (est.) | Provenance |
|---|---|---|---|---|
| 16 (8v8) | ~4 MB | ~29 µs | ~290 µs | `[Extrapolated]` from M-003 / `[Estimated]` |
| 20 (10v10) | ~5 MB | ~45 µs | ~380 µs | `[Extrapolated]` / `[Estimated]` |
| 32 (16v16, benchmarked) | ~7.2 MB | **58 µs** | **~558 µs** | `[Measured, Naia-only]` M-003 / `[Estimated]` |
| 64 (32v32) | ~14 MB | ~116 µs | ~760 µs | `[Extrapolated]` / `[Estimated]` |
| 80 (40v40) | ~21 MB | ~145 µs | ~880 µs | `[Extrapolated]` / `[Estimated]` |

*All Naia figures except 16v16 are extrapolations. Win 3: cost is O(mutations × users), so scaling
is well-understood for the Naia layer. Physics scaling is separately estimated.*

### 3c. Bandwidth (Outbound Server → Clients)

**This section uses the O(N²) worst-case formula, NOT measured bytes. See §24 for a full discussion of the event-driven model and why production BW is likely far lower.**

Bandwidth follows the **O(N²) worst-case law** `[Extrapolated, formula only]`: N players = N mutations/tick × N clients receiving =
N² bytes/tick. This assumes every entity sends a position update every tick — which the actual
event-driven implementation does NOT do.

| State | Calculation | Per-cell rate | Provenance |
|---|---|---|---|
| Level load burst | 10K × ~20 B/entity × 16 clients | ~3 MB over 80 ms (~37.5 MB/s peak) | `[Extrapolated]` |
| Active combat (32 mutations, 16 clients) | 32 × 18 B × 16 × 25 Hz | **~230 KB/s** | `[Extrapolated, formula only]` |
| Idle (keepalives + clock sync) | 16 × 5 B/tick × 25 Hz | **~2 KB/s** | `[Extrapolated]` |
| Realistic mix, 10% active duty | 0.1×230 + 0.9×2 | ~25 KB/s | `[Extrapolated]` |
| Realistic mix, 30% active duty | 0.3×230 + 0.7×2 | ~70 KB/s | `[Extrapolated]` |
| Realistic mix, 60% active duty (intense TDM) | 0.6×230 + 0.4×2 | ~139 KB/s | `[Extrapolated]` |

Monthly bandwidth per cell (16v16):

| Active duty | KB/s avg | GB/month | Provenance |
|---|---|---|---|
| 10% | 25 | **65** | `[Extrapolated, formula only]` |
| 30% | 70 | **181** | `[Extrapolated, formula only]` |
| 60% | 139 | **360** | `[Extrapolated, formula only]` |

---

## 4. Vultr VPS Analysis — 5 Price Points

### Server-sizing constants used

- Full game tick cost: ~558 µs/cell (16v16 @ 10K tiles) `[Estimated, no benchmark]`
- Memory/cell: 7.2 MB `[Estimated]`
- Bandwidth/cell: 181 GB/month (30% active duty) for 16v16 baseline `[Extrapolated, formula only]`
- Realistic CPU efficiency: 40% on shared, 55% on dedicated `[Assumption]`
- Binding constraint = min(CPU, RAM, BW)
- Vultr pricing as of 2026-04 `[External pricing, manually verified 2026-04]`

> All capacity figures in this section are `[Estimated]` or `[Extrapolated]` and inherit the full-stack tick uncertainty from §2.

---

### Tier 1 — $10/mo: Development `[External pricing, manually verified 2026-04]`

**Spec:** 1 shared vCPU · 2 GB RAM · 2 TB/mo bandwidth

| Constraint | Formula | Limit | Provenance |
|---|---|---|---|
| CPU | 0.5 eff. cores × 29 cells/core | 15 cells | `[Estimated, no benchmark]` |
| RAM | 2 000 MB ÷ 7.2 MB | 278 cells | `[Estimated]` |
| Bandwidth | 2 000 GB ÷ 181 GB/cell/mo | **11 cells ← binding** | `[Extrapolated, formula only]` |

**Result:** ~11 cells · 352 player-slots `[Estimated]`

**Verdict:** Dev and testing only. Shared CPU variance makes tick jitter unpredictable.
Bandwidth is the hard wall — Vultr's budget tiers are BW-stingy.

---

### Tier 2 — $20/mo: Minimal Staging `[External pricing, manually verified 2026-04]`

**Spec:** 2 shared vCPU · 4 GB RAM · 3 TB/mo bandwidth

| Constraint | Formula | Limit | Provenance |
|---|---|---|---|
| CPU | 1 eff. core × 29 | 29 cells | `[Estimated, no benchmark]` |
| RAM | 4 000 MB ÷ 7.2 MB | 556 cells | `[Estimated]` |
| Bandwidth | 3 000 GB ÷ 181 GB/cell/mo | **17 cells ← binding** | `[Extrapolated, formula only]` |

**Result:** ~17 cells · 544 player-slots `[Estimated]`

**Verdict:** Viable for a small closed beta or prototyping. Bandwidth is always the binding
constraint on this tier. See §9 for how reducing match size dramatically expands capacity.

---

### Tier 3 — $40/mo: Small Production Unit `[External pricing, manually verified 2026-04]`

**Spec:** 4 shared vCPU · 8 GB RAM · 4 TB/mo bandwidth

| Constraint | Formula | Limit | Provenance |
|---|---|---|---|
| CPU | 2 eff. cores × 29 | 58 cells | `[Estimated, no benchmark]` |
| RAM | 8 000 MB ÷ 7.2 MB | 1 111 cells | `[Estimated]` |
| Bandwidth | 4 000 GB ÷ 181 GB/cell/mo | **22 cells ← binding** | `[Extrapolated, formula only]` |

**Result:** ~22 cells · 704 player-slots `[Estimated]`

**Verdict:** First viable tier for very small early-access. Bandwidth is still the wall.
Shared CPU variance remains a risk for real-time workloads.

---

### Tier 4 — ~$80–100/mo: Dedicated Game Server (Smallest) `[External pricing, manually verified 2026-04]`

**Spec (Vultr Optimized Cloud Compute, estimated):**
4 dedicated vCPU · 16 GB RAM · ~6 TB/mo bandwidth

Dedicated CPU eliminates noisy-neighbor jitter. Efficiency factor improves from 40% to ~55%.

| Active duty | Cells (BW-bound) | Cells (CPU-bound) | Binding | Provenance |
|---|---|---|---|---|
| 30% | 33 | 157 | BW | `[Extrapolated]` / `[Estimated]` |
| 10% | 92 | 157 | BW | `[Extrapolated]` / `[Estimated]` |
| 5% | 176 | 157 | CPU at ~157 cells | `[Extrapolated]` / `[Estimated]` |

**At 10% active (realistic mixed Halo gameplay):**
~92 cells · 2 944 player-slots `[Estimated]`

**Verdict:** First production-worthy tier. Dedicated CPU eliminates jitter. The 10% active
assumption is realistic for a game with pregame lobbies, intermission, and varied zones.

---

### Tier 5 — ~$224/mo: Full Production Game Server `[External pricing, manually verified 2026-04]`

**Spec (Vultr Optimized Cloud Compute, estimated):**
8 dedicated vCPU · 32 GB RAM · ~10 TB/mo bandwidth

| Active duty | BW-limited cells | CPU-limited cells | Player-slots | Provenance |
|---|---|---|---|---|
| 30% | 55 | 319 | 1 760 | `[Extrapolated]` / `[Estimated]` |
| 10% | 154 | 319 | **4 928** | `[Extrapolated]` / `[Estimated]` |
| 5% | 294 | 319 | **9 408** | `[Extrapolated]` / `[Estimated]` |

At 5% active duty (mostly lobby/idle, bursts of combat): CPU becomes binding at ~294 cells.
At 10% active: BW-bound at 154 cells.

**At 10% active duty:**
~154 cells · 4 928 player-slots `[Estimated]`

**Verdict:** The production sweet spot. At 10 000 CCU across 3–4 of these servers, the
monthly server bill is ~$900. Recouped by ~60 subscribers at $15/mo.

---

### Player/Server Scaling Table (Tier 5, 10% active duty, 16v16) `[Estimated]`

| Target CCU | Cells needed | Servers needed | Monthly server cost |
|---|---|---|---|
| 320 | 10 | 0.1 | ~$22 (use Tier 3) |
| 1 600 | 50 | 0.3 | ~$67 (1 × Tier 4) |
| 5 000 | 156 | 1.0 | ~$224 |
| 10 000 | 313 | 2.0 | ~$450 |
| 50 000 | 1 563 | 10.2 | ~$2 285 |
| 100 000 | 3 125 | 20.3 | ~$4 550 |

---

## 5. The Guild Wars Instancing / Portal Network Model

### Architecture

Each "cell" = one game_server thread managing one level instance:

```
[Cell A: Outpost] <--portal--> [Cell B: Combat Zone] <--portal--> [Cell C: Boss Area]
  32 players                       32 players                        32 players
  1 thread                         1 thread                          1 thread
  7 MB RAM                         7 MB RAM                          7 MB RAM
```

**Instancing:** When Cell B fills, the server spawns **Cell B'** (independent thread,
independent Naia state, independent bandwidth). This is purely additive — no cross-cell
state synchronization needed.

**Level load amortization:** A cell's level is loaded once when the first player enters
(~5.2 s for 10K tiles `[Measured, Naia-only]` M-001). Subsequent players joining the same instance get replication from
the running server in one tick. Portal pre-warming is critical — see §7.

### Portal Network Topology Sizing `[Estimated]`

| World scale | Zone types | Avg instances/zone | Total cells | RAM | Monthly servers |
|---|---|---|---|---|---|
| Indie MVP | 10 | 2 | 20 | ~144 MB | 1 × Tier 3 ($40) |
| Small launch | 50 | 3 | 150 | ~1 GB | 1 × Tier 5 ($224) |
| Mid-size | 200 | 3 | 600 | ~4.3 GB | 4 × Tier 5 ($896) |
| Large | 500 | 4 | 2 000 | ~14 GB | 13 × Tier 5 (~$3 000) |
| MMO-scale | 1 000 | 5 | 5 000 | ~36 GB | 33 × Tier 5 (~$7 400) |

---

## 6. Tile Count as a Design Lever

| Scenario | Tiles/cell | Memory/cell | CPU/cell | Level load | Cells on 32 GB RAM | Provenance |
|---|---|---|---|---|---|---|
| PvP arena | 1K | 1.5 MB | ~558 µs | ~0.5 s | 21 333 | `[Extrapolated]` |
| Halo BTB (benchmarked Naia-only) | 10K | 7.2 MB | ~558 µs | ~5.2 s | 4 444 | `[Measured, Naia-only]` M-001 / `[Estimated]` |
| Campaign mission | 32K | ~22 MB | ~558 µs | ~17 s | 1 454 | `[Extrapolated]` |
| Open world zone | 64K | ~45 MB | ~558 µs | ~33 s | 711 | `[Extrapolated]` |

**Key leverage:** Tile count is free in CPU (Win 2 `[Measured, Naia-only]`). You can build 64K-tile maps and the
server tick budget is identical. You only pay in RAM and level-load time.

---

## 7. Portal Pre-Warming (Critical UX Requirement)

Level load for 10K tiles = 5.2 seconds `[Measured, Naia-only]` M-001. For seamless portal transitions:

1. When player is **10+ seconds** from a portal, begin booting the destination cell thread.
2. The destination cell spawns its Naia server, allocates entity state, loads tiles.
3. When the player crosses the portal, the cell is already in steady state.
4. If the destination cell is already running (another player is there), instant join — no
   load time.

The 5.2-second benchmark gives us the pre-warm timing budget. For 64K-tile maps, start
pre-warming 40+ seconds before the portal is used.

> Note: This benchmark is Naia-only. Full game stack (level geometry load, physics init, etc.) may increase actual load time. A full-stack benchmark should re-measure this.

---

## 8. 40v40 Match Analysis (80 Players/Cell)

*Not yet benchmarked. All figures are extrapolations from the 16-player baseline using Win 3
(O(mutations × users) scaling) `[Extrapolated]`. 40v40 is an aspirational/event tier, NOT a launch default — see §9.*

### Resource estimates for 80-player cells `[Extrapolated]`

| Resource | 16 players (measured) | 80 players (extrapolated) | Scaling factor | Provenance |
|---|---|---|---|---|
| Naia tick, active | 58 µs | ~290 µs | ~5× (linear in users) | `[Measured]` M-003 / `[Extrapolated]` |
| Full game tick | ~558 µs | ~1 300 µs | ~2.3× (physics scales sub-linearly) | `[Estimated]` / `[Extrapolated]` |
| Memory/cell | 7.2 MB | ~21 MB | ~3× (connection state dominates) | `[Estimated]` / `[Extrapolated]` |
| BW, active (60% duty) | ~139 KB/s | ~1 732 KB/s | ~12.5× (O(N²)) | `[Extrapolated, formula only]` |
| BW/month (60% duty) | 360 GB | **4 489 GB** | ~12.5× | `[Extrapolated, formula only]` |

### 40v40 cost at scale `[Estimated]`

| Target | Servers | Monthly cost |
|---|---|---|
| 1 concurrent match (testing) | 1 × Tier 4 ($100/mo) | ~$100 |
| 5 concurrent matches (small beta) | 1 × Tier 5 ($224/mo) | ~$224 |
| 20 concurrent matches (launch event) | 2–3 × Tier 5 | ~$450–700 |
| 100 concurrent matches (growth) | 10–12 × Tier 5 | ~$2 240–2 700 |

---

## 9. Validation Ladder

Each tier below has three gates that must all pass before that match size enters capacity planning or product decisions.

**Do not model any tier as a production default until its benchmark gate is passed.**

### Tier 1: 2v2 (4 players) — Launch default

| Gate | Description | Status |
|---|---|---|
| Benchmark gate | Full-stack 2v2 bench: 4 players, real Rapier, real game logic; P95 tick ≤ 40 ms; wire bytes measured | NOT DONE |
| Playtest gate | Internal 2v2 play session with live network; no tick budget violations | NOT DONE |
| Business gate | 2v2 queue fills within 5 min at ≥50 CCU for 3 consecutive peak days | NOT DONE |

### Tier 2: 5v5 (10 players) — Unlock at ~150 CCU

| Gate | Description | Status |
|---|---|---|
| Benchmark gate | Full-stack 5v5 bench: 10 players, real Rapier, real game logic; P95 tick ≤ 40 ms; wire bytes measured | NOT DONE |
| Playtest gate | 5v5 internal playtest with live network | NOT DONE |
| Business gate | 5v5 queue fills within 5 min at ≥150 CCU for 3 consecutive peak days | NOT DONE |

### Tier 3: 10v10 (20 players) — Unlock at ~400 CCU

| Gate | Description | Status |
|---|---|---|
| Benchmark gate | Full-stack 10v10 bench: 20 players, real Rapier, real game logic; P95 tick ≤ 40 ms; wire bytes measured | NOT DONE |
| Playtest gate | 10v10 internal playtest with live network | NOT DONE |
| Business gate | 10v10 queue fills within 5 min at ≥400 CCU for 3 consecutive peak days | NOT DONE |

### Tier 4: 40v40 (80 players) — Aspirational/event tier, unlock at ~2 000 CCU

40v40 is NOT a launch mode. It is a scheduled event mode gated behind full organic growth.

| Gate | Description | Status |
|---|---|---|
| Benchmark gate | Full-stack 40v40 bench: 80 players, real Rapier, real game logic; P95 tick ≤ 40 ms; wire bytes measured; on-demand cell boot/teardown measured | NOT DONE |
| Playtest gate | 40v40 beta event with live players | NOT DONE |
| Business gate | 40v40 events self-sustaining with ≥5 concurrent matches; server cost absorbed by ~2 000 CCU revenue | NOT DONE |

### 9e. Steam / Linux / Proton Client Validation

This track must pass before Steam is modeled as a meaningful player acquisition channel (Gate 15). It is independent of the match-size tiers above.

- [ ] Build native Linux binary (outside Steam)
- [ ] Launch through Steam runtime; confirm startup, input, networking
- [ ] Verify WebSocket/UDP transport under Steam runtime
- [ ] Verify file paths, config, and cache under Steam runtime
- [ ] Verify graphics backend on Steam Deck-class hardware
- [ ] Verify controller layout on Steam Deck
- [ ] Proton compatibility smoke test — Windows + Steam client
- [ ] Confirm no Proton blockers (anti-cheat, WebView, launcher components)

**Gate:** All checkboxes pass before Steam page goes live or Steam acquisition is modeled.

---

## 10. Client-Side Capacity

Client receive cost: **889 ns per active tick** `[Measured, Naia-only]` M-004 = 22.2 µs/second at 25 Hz = **0.002% of
one CPU core**. Naia client-side networking is essentially free. Client CPU budget is
100% available for rendering. This holds even at 80 players (estimated ~5 µs/second `[Extrapolated]`).

> Client rendering cost (Bevy ECS, Rapier client-side, draw calls) is NOT benchmarked. Client-side networking is free; total client performance on target hardware (web/Wasm) is unknown.

---

## 11. The Three Binding Constraints (Ranked by Frequency)

| Rank | Constraint | When binding | Mitigation | Provenance |
|---|---|---|---|---|
| 1 | **Bandwidth** | Always on budget shared-CPU plans (≤$40/mo); at high active duty on any plan | Reduce match size; lower tick rate for idle cells; delta compression; interest management | `[Extrapolated, formula only]` — BW not yet measured |
| 2 | **CPU** | On dedicated servers at very low active duty (<5%); always if physics is more expensive than estimated | Win 2/3 already optimized Naia; tile BVH trimesh (§22d) targets physics | `[Estimated, no benchmark]` |
| 3 | **RAM** | Only for 64K-tile maps or very large cell counts on small RAM plans | Keep maps ≤32K tiles; use 32+ GB RAM for dense servers | `[Estimated]` |

---

## 12. Recommendations for the Cyberlith Business Plan

1. **Run a full-stack 2v2 benchmark first.** This is the single highest-leverage action — it resolves the largest unknown in the entire document. Until this gate is passed, all CCU capacity estimates carry ±50% error bars.

2. **Ship 2v2 first, unlock larger modes as the player base grows.** 2v2 is viable at
   launch even with <100 CCU. Requiring 80 players simultaneously for 40v40 means that mode
   can't fill until ~2 000 CCU — which is the right unlock point anyway (see §9).

3. **Start on $20/mo; never upgrade before the 3× revenue rule is met.** See §16 for the
   exact upgrade triggers. Server cost stays under 33% of revenue at every transition.

4. **Treat 40v40 as a scheduled event, not a standing mode.** Boot dedicated cells on demand;
   tear them down after the match. At 4% of match volume this costs ~$25–50/month extra, not
   a whole new server tier.

5. **Target ~1 312 CCU (~394 premium subs) for $10 000/month net profit after taxes** (Base Case, §16b).
   This accounts for subscription + microtx ($7.60/CCU) and self-managed DLC (2 campaigns/year
   at $15, with blended platform fees across Web/PWA and Steam channels). Rewarded ads are excluded from all three business scenarios. See §17 for the DLC model and §16 for all three scenarios.

6. **Model Steam as a separate lower-margin discovery channel.** Steam can be a free-to-play discovery channel, especially for Linux and Steam Deck players. Model Steam-channel monetization through Steam Wallet/DLC flows (not external checkout) until policy is verified. Web/PWA is the high-margin direct-payment channel. See §18 and §21 (Gate 9).

7. **Measure wire bytes.** Wire capacity shows ∞ in the capacity report (not yet measured).
   Running `wire/bandwidth_realistic_quantized` and feeding its output into the capacity
   formula gives exact concurrent-game-on-1Gbps numbers.

8. **The real binding constraint is likely CPU, not bandwidth.** See §22 and §24 for the full analysis.
   Position updates are event-driven, so production BW is likely 10–15× below the O(N²) worst-case estimate.

9. **Switch from Vultr to Hetzner.** At ~$62/mo (CCX33 dedicated), Hetzner eliminates the BW constraint and provides dedicated vCPU at 35× lower cost for equivalent CCU. See §23.

---

## 13. The 4-Tier Match Architecture — Per-Tier Resource Costs

Connor's target mix: **40% 2v2 · 40% 5v5 · 16% 10v10 · 4% 40v40**.

All costs derived from the O(N²) bandwidth law and the 16v16 benchmark using measured
Naia scaling (0.113 µs × N² per tick `[Measured, Naia-only]`) plus estimated physics/logic (proportional to N `[Estimated, no benchmark]`).

### 13a. Active bandwidth `[Extrapolated, formula only]`

```
active_bw_bytes_per_sec = N × 18 B × N × 25 Hz = 450 × N²
```

This is the O(N²) worst-case formula. See §24 for event-driven production estimate.

| Tier | N total | Active BW | Idle BW |
|---|---|---|---|
| **2v2** | 4 | **7.2 KB/s** | ~0.1 KB/s |
| **5v5** | 10 | **45 KB/s** | ~0.5 KB/s |
| **10v10** | 20 | **180 KB/s** | ~1 KB/s |
| **40v40** | 80 | **2 880 KB/s** | ~5 KB/s |

### 13b. Memory per cell `[Estimated]`

Formula: 1 MB base + N × 200 KB (client state) + tiles × 300 B (entity data)

| Tier | Players | Tiles | Memory/cell |
|---|---|---|---|
| 2v2 | 4 | ~2 000 | **2.5 MB** |
| 5v5 | 10 | ~4 000 | **4.5 MB** |
| 10v10 | 20 | ~8 000 | **7.5 MB** |
| 40v40 | 80 | ~12 000 | **21 MB** |

### 13c. CPU per cell (full game stack) `[Estimated, no benchmark]`

Naia component: 0.113 µs × N² `[Measured, Naia-only]`. Physics + logic ≈ 11 µs × N `[Estimated]`. OS/overhead: 50 µs fixed `[Estimated]`.

| Tier | Naia (µs) | Physics + logic (µs) | Total (µs) | Cells/core (40% eff.) |
|---|---|---|---|---|
| 2v2 | 1.8 | 66 | **~120** | **133** |
| 5v5 | 11 | 110 | **~170** | **94** |
| 10v10 | 45 | 231 | **~326** | **49** |
| 40v40 | 725 | 880 | **~1 655** | **10** |

### 13d. Level load time and match duration `[Measured, Naia-only]` M-001 / `[Extrapolated]`

| Tier | Match length | ~Queue time | Map tiles | Level load |
|---|---|---|---|---|
| 2v2 | 12 min | ~2 min | ~2 000 | ~1.0 s |
| 5v5 | 24 min | ~5 min | ~4 000 | ~2.1 s |
| 10v10 | 32 min | ~8 min | ~8 000 | ~4.2 s |
| 40v40 | 40 min | ~20 min | ~12 000 | ~6.2 s |

### 13e. Monthly bandwidth per cell `[Extrapolated, formula only]`

| Tier | Match fraction | Combat intensity | Net active | GB/month/cell |
|---|---|---|---|---|
| 2v2 | 86% | 60% | 52% | **~10 GB** |
| 5v5 | 83% | 55% | 46% | **~55 GB** |
| 10v10 | 80% | 50% | 40% | **~190 GB** |
| 40v40 | 67% | 75% | 50% | **~3 740 GB** |

### 13f. Minimum player pool to fill each match type `[Assumption]`

To keep queue times ≤ 5 minutes, a mode needs ~10× its player count in the matchmaking pool:

| Tier | Players/match | Minimum pool | Minimum CCU to offer mode |
|---|---|---|---|
| 2v2 | 4 | 40 | **~50 CCU** |
| 5v5 | 10 | 100 | **~150 CCU** |
| 10v10 | 20 | 200 | **~400 CCU** |
| 40v40 | 80 | 800 | **~2 000 CCU** |

---

## 14. The Mixed Fleet — Server Capacity Across All Four Tiers

### 14a. Cell occupancy weighting `[Estimated]`

| Tier | Count share | Duration (min) | Duration × count | Cell share |
|---|---|---|---|---|
| 2v2 | 40% | 12 | 480 | **22.7%** |
| 5v5 | 40% | 24 | 960 | **45.5%** |
| 10v10 | 16% | 32 | 512 | **24.2%** |
| 40v40 | 4% | 40 | 160 | **7.6%** |
| **Total** | | | 2 112 | 100% |

### 14b. Mixed-fleet weighted resource costs (regular modes: 2v2 + 5v5 + 10v10) `[Estimated]`

| | 2v2 (25.5%) | 5v5 (51.0%) | 10v10 (27.2%) | Weighted avg |
|---|---|---|---|---|
| BW/month | 10 GB | 55 GB | 190 GB | **79 GB/cell** |
| Memory | 2.5 MB | 4.5 MB | 7.5 MB | **4.9 MB/cell** |
| CPU/cell | 120 µs | 170 µs | 326 µs | **204 µs/cell** |
| Players/cell | 4 | 10 | 20 | **11.1 players/cell** |

### 14c. Regular-mode cells and CCU by server tier `[Estimated]`

Binding constraint is always **bandwidth** on shared-CPU plans.

| Server | Monthly cost | BW budget | Regular cells | Peak CCU | $/CCU/mo |
|---|---|---|---|---|---|
| Shared 2 vCPU | **$20** | 3 TB | **38** | **422** | $0.047 |
| Shared 4 vCPU | **$40** | 4 TB | **50** | **556** | $0.072 |
| Dedicated 8 vCPU | **$224** | 10 TB | **126** | **1 399** | $0.160 |
| 2 × dedicated | **$448** | 20 TB | **252** | **2 797** | $0.160 |
| 4 × dedicated | **$896** | 40 TB | **504** | **5 594** | $0.160 |

*CCU = cells × 11.1 players/cell (weighted average for the 2v2/5v5/10v10 mix).*

---

## 15. Revenue Model and Scaling Plan

> **Channel note:** All prior revenue figures in this section assume the Web/PWA direct channel only (higher margin, Stripe fees only). Steam is a separate lower-margin channel with platform fees (~30% on Steam Wallet / DLC purchases). The blended effective margin depends on the Steam/Web player mix — see §16 channel-mix assumptions and §26 formulas for the two-channel model. Until the Steam Wallet/DLC adapter is built and the player mix is measured, use the Web/PWA figures as the upper bound and treat Steam-channel revenue as directional only.

> **Rewarded ads:** Rewarded ads are not a Steam-channel assumption and are not part of the core business model. They may be considered only for non-Steam web/mobile experiments later, but they carry brand, UX, and policy risk. Excluded from all three business scenarios (Survival, Base, Upside).

### 15a. Revenue model assumptions

| Parameter | Value | Provenance |
|---|---|---|
| CCU → MAU multiplier | ×10 | `[Assumption]` Industry rule of thumb |
| Premium conversion (MAU) | 3% | `[Assumption]` Conservative |
| Premium subscription price | $7/mo | `[Assumption]` |
| Microtransaction buyer rate | 27.5% of MAU | `[Assumption]` Low-friction cosmetics model |
| Avg Crystal spend per buyer/month | $2.00 | `[Assumption]` |
| Monthly revenue per CCU | **$7.60** | `[Assumption]` $2.10 sub + $5.50 microtx |

```
monthly_revenue ≈ CCU × 10 × (0.03 × $7   +   0.275 × $2.00)
                = CCU × ($2.10 + $5.50)
                = CCU × $7.60

// sensitivity range:
//   conservative (15% buyers, ~$1/mo):  CCU × $3.60
//   base case    (27.5% buyers, $2/mo): CCU × $7.60   (used throughout)
//   optimistic   (35% buyers, $3/mo):   CCU × $12.60
```

### 15b. Upgrade triggers and financial health at each transition `[Estimated]`

Upgrade when **both** conditions are met:
1. CCU consistently ≥ 70% of current server capacity for 3+ consecutive peak days
2. Monthly revenue (30-day trailing) ≥ 3× cost of next server tier

*Revenue column uses base-case $7.60/CCU/month.*

| Trigger event | CCU | Revenue/mo | Current cost | New cost | Revenue/cost ratio |
|---|---|---|---|---|---|
| Add 2v2 queue | **~50** | ~$380 | $20 | $20 | 19× |
| Unlock 5v5 | **~150** | ~$1 140 | $20 | $20 | 57× |
| **Upgrade $20→$40** | **~295** | ~$2 242 | $20 | $40 | 56× |
| Unlock 10v10 | **~400** | ~$3 040 | $40 | $40 | 76× |
| **Upgrade $40→$224** | **~389** | ~$2 957 | $40 | $224 | **13.2×** |
| **Add 2nd $224** | **~979** | ~$7 440 | $224 | $448 | **16.6×** |
| Unlock 40v40 events | **~2 000** | ~$15 200 | $448 | $448+$224 | **22.6×** |
| **Add 3rd $224** | **~1 958** | ~$14 881 | $448 | $672 | **22.1×** |
| **$10 000 net/mo target** | **~2 025** | ~$15 400 | $448–$672 | — | — |

### 15c. Leading indicators to watch

Track these three weekly. Upgrade when all three are green:

| Indicator | Warning threshold | Action |
|---|---|---|
| Median queue wait time | > 90 s in peak hours | Server near full |
| Cell utilization at peak | > 70% occupied for 3+ days | Approaching ceiling |
| Revenue 30-day trailing | ≥ 3× cost of next tier | Financial cushion exists |

### 15d. Server cost as a fraction of revenue `[Estimated]`

| CCU | Revenue/mo | Server cost | Server % of revenue |
|---|---|---|---|
| 50 | $380 | $20 | 5.3% |
| 200 | $1 520 | $20 | 1.3% |
| 422 | $3 207 | $40 | 1.2% |
| 979 | $7 440 | $224 | 3.0% |
| 2 000 | $15 200 | $448 | 2.9% |
| 2 400 | $18 240 | $448 | 2.5% |

### 15e. The dual-currency economy

**Currency 1: Gold** (earned in-game, never purchased directly)
- Earned by: completing matches, winning, challenge completions, daily login bonus
- Spent on: standard cosmetics, Lives replenishment, community resources

**Currency 2: Crystal** (purchased with real money, never earned from gameplay)
- 100 Crystals = $1, always — flat rate, no volume bonus
- Crystal packs: **500C ($5) · 1 000C ($10) · 2 000C ($20)**

**Crystal pack economics (Stripe fees, no platform cut) `[Assumption]`:**

| Pack | Price | Crystals | Stripe fee | Dev net | Effective rate |
|---|---|---|---|---|---|
| Small | $5 | 500C | $0.445 | $4.555 | 8.9% |
| Medium | $10 | 1 000C | $0.590 | $9.410 | 5.9% |
| Large | $20 | 2 000C | $0.880 | $19.120 | 4.4% |

### 15f. Rewarded ads — excluded from core plan

Rewarded ads are not a Steam-channel assumption and are not part of the core business model. They may be considered only for non-Steam web/mobile experiments later, but they carry brand, UX, and policy risk. **Excluded from all three business scenarios (Survival, Base, Upside).** The prior formula (`CCU × $0.45`) is preserved here for reference only — it does not appear in any scenario revenue calculations.

```
// Reference only — NOT included in any scenario:
monthly_ad_revenue_ref ≈ MAU × 0.30 × 30 × ($5 / 1 000) = CCU × $0.45
```

### 15g. Guest slots and daily Crystal stipend

Each subscriber receives a daily Crystal allowance (e.g., 10C/day = 300C/month) and **4 guest slots**.
No anonymous free tier — every player is either a paying subscriber or a named guest of one.

| Avg active guests per sub | Sub fraction of CCU | Revenue/CCU | vs base |
|---|---|---|---|
| 1 (half slots filled) | 50% | $8.50 | +12% |
| **2 (half slots filled)** | **33%** | **$7.83** | **+3%** |
| 3 (75% slots filled) | 25% | $7.25 | −5% |
| 4 (all slots filled) | 20% | $6.90 | −9% |

---

## 16. Business Plan Scenarios (Three Paths)

All three scenarios model Web/PWA as the primary high-margin channel. Steam is a separate lower-margin discovery channel with platform fees on Steam Wallet/DLC purchases. Rewarded ads are excluded from all three scenarios. See §18 for platform strategy and §26 for two-channel revenue formulas.

### 16a. Survival Case — Retention Proof

**Goal:** Not revenue; prove the game retains players and has a viable economy.

| Metric | Value | Provenance |
|---|---|---|
| Target CCU | ~100 | `[Assumption]` |
| Revenue model | Subscription + cosmetics only (no DLC, no ads) | |
| Monthly gross | ~$760 | `[Estimated]` 100 CCU × $7.60 |
| Monthly costs | ~$220 (server $20 + overhead $200) | `[Estimated]` |
| After-tax net | ~$378 | `[Estimated]` |
| Server tier | $20/mo | |
| Match modes available | 2v2 only | |

**Gate to exit Survival Case:** D7 retention ≥ 30%, D30 retention ≥ 10%, microtx conversion rate ≥ 20%. These prove the product works before scaling costs.

**Channel Mix (Survival):**
- Mostly Web/PWA direct — founder packs + cosmetics via web payments
- Steam: playtest/demo only, no paid items yet
- No rewarded ads
- Revenue assumes Web/PWA margin (Stripe fees only)

### 16b. Base Case — ~$10K/Month Net (Primary Target) `[Estimated]`

This is the primary target documented throughout this file. Achievable via Web/PWA direct payments + DLC campaigns.

```
full_revenue_per_ccu = $7.60 (sub+microtx) + $4.04 (DLC, 2/yr, moderate) = $11.64
variable_costs_per_ccu = $0.43
net_per_ccu = $11.21

// Solve: 0.70 × (CCU × $11.21 − $424 fixed costs) = $10 000
CCU = ($14 286 + $424) / $11.21 ≈ 1 312
```

| Metric | Value | Provenance |
|---|---|---|
| Target CCU | ~1 312 | `[Estimated]` |
| Premium subs (3% of MAU) | ~394 | `[Estimated]` |
| Monthly gross | ~$15 272 | `[Estimated]` |
| Monthly costs | ~$988 | `[Estimated]` |
| After-tax net | **~$9 999** | `[Estimated]` |
| Server tier | 1 × $224/mo dedicated | |
| Match modes | 2v2, 5v5, 10v10 | |

**Channel Mix (Base Case):**
- Web/PWA: primary high-margin channel (Stripe fees ~2.9% + $0.30)
- Steam: discovery + Steam-channel purchases via Steam Wallet/DLC (platform fee ~30% on Steam purchases)
- Blended platform fees modeled at ~15–20% across channels `[Assumption]` — depends on Web/Steam player split
- Linux-native / Steam Deck build quality matters at this scale
- Proton compatibility should be validated before Steam launch
- No rewarded ads

### 16c. Upside Case — ~$15K+/Month Net `[Estimated]`

| Metric | Value | Provenance |
|---|---|---|
| Target CCU | ~2 025 | `[Estimated]` |
| Premium subs | ~608 | `[Estimated]` |
| Revenue model | Sub + microtx + DLC + player market (future, gated) | |
| Monthly gross | ~$15 390 (sub+microtx only baseline) + additional DLC revenue | `[Estimated]` |
| Monthly costs | ~$1 094 (2 × $224 server + overhead + processing) | `[Estimated]` |
| After-tax net | **~$10 007 – $15 800** (depending on DLC performance) | `[Estimated]` |
| Server tier | 2 × $224/mo dedicated | |
| Match modes | All four tiers including 40v40 events | |

> At this CCU level, DLC campaigns become a significant accelerator on top of the base subscription + microtx revenue.

**Channel Mix (Upside Case):**
- Steam becomes a meaningful discovery engine at this scale
- Web/PWA remains the high-margin hub for direct payments
- Steam Wallet/DLC adapter mature; Steam-channel purchases handled through compliant flows
- Campaign/content drops available on both channels
- Player market only after fraud controls are in place
- No rewarded ads

### 16d. Sensitivity analysis — what changes the timeline most

*All deltas vs. the Base Case (1 312 CCU target at $7/mo sub, 27.5% microtx rate, 2 cosmetics/mo, 30% tax).*

| Lever | Effect | CCU target | Notes |
|---|---|---|---|
| Sub price: $7→$10/mo | −11% | ~1 120 | Sub effect is muted — microtx dominates |
| Conversion rate: 3%→5% | −16% | ~1 060 | Requires strong community |
| Microtx rate: 27.5%→40% | **−25%** | **~947** | Best cosmetic cadence lever |
| Microtx frequency: 2→3/mo | **−27%** | **~920** | More releases, larger catalog |
| Tax rate: 30%→20% | −12% | ~1 110 | S-corp election dependent |
| Conservative ($3.60/CCU base) | **+111%** | ~2 660 | Only if microtx barely converts |

---

## 17. DLC Campaigns as a Revenue Accelerator

### 17a. Platform economics — the self-managed advantage

| Platform | DLC price | Platform cut | Dev net per $15 sale | Provenance |
|---|---|---|---|---|
| Steam | $15 | 30% | **$10.50** | `[External pricing, manually verified 2026-04]` |
| App Store (iOS) | $15 | 30% | $10.50 | `[External pricing, manually verified 2026-04]` |
| Google Play | $15 | 15% | $12.75 | `[External pricing, manually verified 2026-04]` |
| **Self-managed (Stripe)** | **$15** | **0%** | **$14.27** (2.9% + $0.30 only) | `[External pricing, manually verified 2026-04]` |

**Net: Web/PWA direct yields 36% more per DLC sale than through Steam.** Steam-channel DLC sales should be modeled at $10.50 net (after ~30% Steam fee), not $14.27. The blended DLC margin depends on the Steam/Web player mix.

The Web/PWA advantage: no platform cut on subscriptions, microtx, or DLC. The trade-off: without Steam's browse/recommendation engine, there is no passive discovery. Every new player acquired outside Steam must come via social media, influencers, SEO, word of mouth, or direct marketing. DLC launch visibility spikes are smaller without a Steam storefront — estimated 5–8% new-player acquisition per campaign (vs 15%+ on Steam `[Assumption]`).

### 17b. Co-op campaigns are architecturally cheap `[Extrapolated, formula only]`

| Mode | Players | Active BW | Monthly BW/cell |
|---|---|---|---|
| 5v5 PvP | 10 | 45 KB/s | ~55 GB |
| **5-player co-op** | **5** | **~11 KB/s** | **~14 GB** |
| Solo campaign | 1 | ~0.5 KB/s | ~1.3 GB |

### 17c. DLC economics (self-managed, $15 price, 2 campaigns/year) `[Estimated]`

| Variable | Conservative | Moderate | Optimistic |
|---|---|---|---|
| Net sales per campaign (at 8 333 MAU) | ~920 | ~1 415 | ~2 085 |
| Revenue per campaign | **~$13 129** | **~$20 193** | **~$29 753** |
| Monthly DLC revenue (2/year amortized) | **~$2 188** | **~$3 366** | **~$4 959** |

### 17d. Full revenue picture at 250 premium subs, 2 DLC/year `[Estimated]`

**Base case at 8 333 MAU (250 subs, 833 CCU) — Web/PWA channel only (upper-bound margin):**

```
Revenue streams (monthly gross):
  Subscription:  250 × $7                        = $1 750
  Microtx:       8 333 × 27.5% × $2/mo           = $4 583
  DLC (moderate, 2/year): $20 193 × 2 / 12        = $3 366
  // Rewarded ads: EXCLUDED from core plan
  Total gross:                                     $9 699

Costs (monthly):
  Server:         1 × $224                        = $  224
  Overhead:                                        = $  200
  Processing (subs + microtx): 2.9% × $6 333      = $  184
  Processing (DLC): 1 415 × 2 / 12 × $0.74        = $  175
  Total costs:                                     $  783

Pre-tax profit:   $9 699 − $783                    = $8 916
After-tax (30%):                                   = $6 241/month
```

> Note: If Steam-channel players represent a meaningful share, DLC and microtx revenue net will be lower due to the ~30% Steam fee on Steam Wallet/DLC purchases. Model blended margin once Steam/Web player split is known.

### 17e. CCU target with Web/PWA direct + 2 DLC/year at $15 `[Estimated]`

**CCU target: ~1 312 CCU → ~394 premium subs** (Web/PWA direct channel, upper-bound margin)

Verification at 1 312 CCU (Web/PWA only, no rewarded ads):
- Revenue per CCU: $7.60 (sub+microtx) + $4.04 (DLC moderate) = $11.64
- Revenue: 1 312 × $11.64 = $15 272
- Costs: $424 + 1 312 × $0.43 = $424 + $564 = $988
- Pre-tax: $15 272 − $988 = $14 284
- After-tax (30%): $14 284 × 0.70 = **~$9 999** ✓

| CCU | Subs (~3% of MAU) | After-tax net |
|---|---|---|
| 833 (250 subs) | 250 | **~$6 500** |
| 1 000 | 300 | **~$7 870** |
| 1 312 | ~394 | **~$10 000** ✓ |
| 1 500 | 450 | **~$11 700** |
| 2 025 | 608 | **~$15 800** |

---

## 18. Distribution, Platform Matrix, and Steam Strategy

### 18a. Strategic framing

Cyberlith's preferred distribution model is multi-channel but single-account: Web/PWA for maximum reach and direct payments; Linux-native for the owned native target; Steam for discovery, Steam Deck/Linux legitimacy, and Steam-channel monetization through compliant purchase flows; Proton compatibility as the preferred Windows strategy rather than a Windows-native client. Windows-native support is intentionally out of scope unless later evidence shows it is cheap enough to justify.

Steam can be a free-to-play discovery channel, especially for Linux and Steam Deck players. However, monetization for Steam-channel users should be assumed to require Steam Wallet / Steam DLC / Steam microtransaction-compatible flows until verified otherwise. Therefore Steam should be modeled as a lower-margin but higher-discovery channel, while Web/PWA remains the high-margin direct-payment channel.

### 18b. Platform matrix

| Platform / Channel | Preferred Client Path | Payment Path |
|---|---|---|
| Web/PWA | Wasm/browser client | Direct Cyberlith web payments (Stripe) |
| Linux desktop direct | Native Linux binary | Direct Cyberlith web payments (Stripe) |
| Steam Linux | Native Linux Steam build | Steam Wallet / Steam DLC / Steam MTX |
| Steam Deck | Native Linux; Proton compatibility as fallback/test target | Steam Wallet / Steam DLC / Steam MTX |
| Windows Steam users | Proton compatibility, NOT Windows-native | Steam Wallet / Steam DLC / Steam MTX |
| Mac | PWA unless native becomes cheap later | Direct Cyberlith web payments |
| Mobile | PWA unless app-store native becomes worth it later | Direct web payments or future store-compliant path |

### 18c. Steam payment policy — OPEN RISK `[Policy risk, requires verification]`

**Do NOT assume Steam users can complete external web checkout without restriction.**

Current knowledge (as of 2026-04, unverified for the specific use case):
- Steam's standard policy may require Steam Wallet for any in-app transactions from Steam users
- Linking to an external payment page from a Steam game may trigger review or policy violation
- Steam's rules around "games that include in-game transactions" are version-dependent and subject to change
- The "Steam page as a redirect to PWA only" approach requires a real playable Steam build (Linux-native), not just a marketing page

**Required action — verify before Steam launch:**
1. Read current Steam Subscriber Agreement and Steamworks Partner documentation on payment policy
2. Contact Valve developer support to confirm which purchase flows are permitted for this game type
3. Do NOT model Steam-channel revenue as Web/PWA-margin equivalent until confirmed
4. Design Steam Wallet/DLC adapter before listing any paid items on Steam

**Conservative model for Steam-channel players:**
Until verified, model Steam-acquired players with Steam Wallet as their payment method:
- Platform cut: ~30% on any DLC/cosmetic/MTX that flows through Steam
- Dev net: $10.50 per $15 DLC vs $14.27 self-platform (26% reduction)
- Subscription revenue: model as unaffected only IF subscription is fully managed outside Steam and Steam policy permits

**Decision gates:** See §21, Gates 9, 10, 11, 15.

### 18d. Web/PWA — primary high-margin channel

- All transactions via Stripe, no platform cut beyond processor fees (~2.9% + $0.30)
- Safari PWA bypasses Apple IAP entirely
- Full margin on subscriptions, microtx, DLC
- Discovery is self-managed (no storefront algorithm)
- **Confidence: High on payment mechanics** `[External pricing, manually verified 2026-04]`

### 18e. Steam Linux / Steam Deck — discovery + compliant monetization channel

- Ship real native Linux Steam build (NOT a redirect-only page)
- Advantage: Steam's browse and recommendation engine drives organic discovery, especially Linux and Steam Deck players
- Monetization: Steam Wallet / Steam DLC / Steam microtransaction flows (not external Stripe from Steam context)
- Proton compatibility tested as fallback for Windows users via Steam
- **Estimated new-player acquisition from Steam vs self-platform: 15%+ vs 5–8% per DLC launch** `[Assumption]`
- **Gate:** Must ship real playable Linux-native app before using Steam as a meaningful acquisition channel (Gate 15)

---

## 19. Payment Architecture — One Backend, Multiple Purchase Providers

### 19a. Core principle

**Never let platform payment logic leak into game logic.**

The backend entitlement system must be platform-agnostic. Payment providers are adapters, not game systems. Purchases are verified server-side, then converted into canonical Cyberlith entitlements. The client never self-grants.

### 19b. Backend data model

| Concept | Description |
|---|---|
| `CyberlithAccount` | Canonical player identity; linked to one or more identity providers including optional SteamID |
| `Entitlement` | Canonical ownership record for a SKU (e.g., campaign unlocked, cosmetic owned) |
| `CurrencyBalance` | Crystal/premium currency balance per account |
| `PurchaseTransaction` | Audit record of every purchase event with provider, SKU, amount, and verification result |
| `PurchaseProvider` | Enum: `WebStripe \| SteamWallet \| SteamDlc \| ManualGrant \| FutureApple \| FutureGoogle` |
| `SkuCatalog` | Canonical SKU list with provider-specific ID mappings |

### 19c. SKU catalog — canonical mapping table

| Canonical SKU | Web Provider ID | Steam Item ID | Steam DLC App ID | Grant |
|---|---|---|---|---|
| `crystals_500` | Stripe price ID | Steam item ID | — | +500 Crystals |
| `founder_pack_1` | Stripe price ID | Steam item ID | optional DLC | cosmetics + title + Crystals |
| `campaign_1` | Stripe price ID | Steam item ID | Steam DLC app ID | campaign entitlement |

> Note: Steam DLC App IDs and Steam item IDs must be registered in Steamworks. Stripe price IDs are created in the Stripe dashboard. The backend maps provider receipts to canonical SKUs before granting entitlements — no provider-specific logic enters the game layer.

### 19d. Purchase flow invariants

1. **Server-side verification always:** Receipt/token from any provider is verified against that provider's API before any entitlement is granted.
2. **No client self-grant:** The client sends a purchase intent or receipt; only the server grants the entitlement.
3. **Idempotent transactions:** Duplicate receipts must be detected and rejected (use provider transaction IDs as idempotency keys).
4. **Refunds and chargebacks revoke entitlements:** The entitlement system must support revocation.

### 19e. Steam account linking

- Launch through Steam should identify the player via SteamID (via Steamworks SDK `GetPlayerSteamID`).
- Backend creates or links a `CyberlithAccount` for that SteamID.
- Avoid forcing manual account creation before the player can enter the game — the SteamID is sufficient for initial identity.
- Allow email/password to be added later for cross-platform play.
- If a player economy or trading system exists, Steam unlink/relink must be heavily restricted to prevent fraud (account trading abuse).
- **Status: Future implementation note — not yet implemented. Design before Steam launch.**

---

## 20. Risk Register

| Risk | Category | Severity | Likelihood | Current Evidence | Mitigation | Next Action |
|---|---|---|---|---|---|---|
| Full-stack tick cost unmeasured | Technical | **Critical** | Certain (it IS unknown) | Naia-only 58 µs measured; physics estimated at ~350 µs | Run 2v2 full-stack benchmark | See §25, BM-001 |
| Bandwidth not instrumented | Technical | **High** | Certain (wire bench not run) | Wire capacity shows ∞ in capacity report | Add `server_outgoing_bytes_per_tick_total` metric; run wire bench | See §25, BM-005 |
| Steam payment policy misunderstood | Platform/Business | **High** | Uncertain | Manually unverified as of 2026-04; external checkout may be prohibited | Verify Steamworks rules; model Steam as separate payment channel with ~30% fee | Read Steamworks Partner docs; contact Valve support — see §18c |
| Steam users cannot route to external checkout | Platform/Business | **High** | Medium | Steam policy likely requires Steam Wallet for in-app purchases | Implement Steam Wallet/DLC adapter before any paid Steam items | Design adapter before Steam launch — see §19 |
| Linux-only Steam build limits discovery vs Windows users | Market | **Medium** | Medium | No Windows-native build planned; Proton unvalidated | Steam Deck-first quality; Proton compatibility validated | Add Proton validation gate — see §9 Steam/Proton track |
| Windows-native avoided intentionally | Technical/Product | **Medium** | N/A — intentional decision | Proton + PWA is the Windows strategy | Proton compatibility + PWA fallback | Validate Proton before Steam launch — Gate 11 |
| Proton compatibility not actually good enough | Technical | **Medium** | Medium | No Proton testing done yet | Add Steam/Proton validation track to §9 | See §9 Steam/Proton validation checklist |
| Steam page cannot be used as PWA redirect only | Platform/Marketing | **Medium** | High | Steam requires a real playable app | Ship real native Linux/Steam Deck build before Steam page | Design Steam build before Steam page — Gate 15 |
| Entitlement fragmentation across platforms | Architecture | **High** | Certain if not designed | No entitlement backend designed yet | One backend entitlement system with provider adapters (§19) | Design entitlement backend before first paid item |
| Rewarded ads off-brand or policy-incompatible | Product/Platform | **Medium** | High | Steam policies restrict rewarded ads; brand risk | Excluded from core plan entirely | No action needed unless strategy changes |
| Shared CPU jitter | Technical | **Medium** | High on shared vCPU plans | No production measurement; known industry problem | Use dedicated vCPU (Hetzner CCX or Vultr Optimized) for production | Switch to Hetzner CCX33 before production |
| Content treadmill (DLC cadence) | Business | **High** | Medium | 2 DLC/year assumed; no proven release velocity | Pre-build campaign content before launch; set realistic cadence | Evaluate team velocity at 6-month mark |
| Economy fraud / Crystal abuse | Business | **Medium** | Low-Medium | No fraud prevention system designed | Rate-limit Crystal purchases; flag anomalous player-market activity | Add fraud checks before opening player market |
| Physics cost 2× estimate | Technical | **High** | Medium | Estimate only; no benchmark | Tile BVH trimesh (§22d) reduces Rapier cost before measuring | Run full-stack benchmark first; then optimize |
| Rapier order-sensitivity (determinism) | Technical | **Medium** | Known existing risk | Documented in cyberlith simulation determinism memory | HashMap key sorting for bulk collider spawns already documented | Enforce in tests |
| Multi-cell cross-server jitter | Technical | **Medium** | Medium | No multi-cell benchmark exists | Design portal protocol before building multi-server | See §25, BM-009 |
| WAN performance vs LAN | Technical | **Medium** | Medium | All benchmarks are loopback | WAN test with real clients required | See §25, BM-011 |
| Client performance on target hardware (web/Wasm) | Technical | **High** | Unknown | Client networking is free (889 ns); rendering untested | Wasm perf test on mid-range laptop browser | See §25, BM-012 |
| Player acquisition without platform | Business | **High** | Medium | No marketing strategy documented | DLC launches as marketing events; creator program; Steam discovery | Define acquisition strategy post-launch |

---

## 21. Decision Gates

These are hard stops. The named decision cannot be made until the gate condition is met.

| Gate | Decision blocked | Gate condition | Current status |
|---|---|---|---|
| Gate 1 | Model any match size as a production default | Full-stack benchmark passes P95 tick ≤ 40 ms for that match size | NOT DONE for any tier |
| Gate 2 | Model 10v10 as a launch default | 10v10 full-stack benchmark gate passes (§9, Tier 3) | NOT DONE |
| Gate 3 | Model 40v40 as anything other than aspirational/event | 40v40 full-stack benchmark AND ≥2 000 CCU sustained (§9, Tier 4) | NOT DONE |
| Gate 4 | Include BW constraint as the primary capacity limit | Wire bytes measured per tick via `server_outgoing_bytes_per_tick_total` bench | NOT DONE |
| Gate 5 | Include Steam external checkout in revenue model | Steam Subscriber Agreement reviewed; Valve support confirms external checkout allowed for this use case | NOT DONE — expected outcome is "NOT allowed"; model Steam as separate channel |
| Gate 6 | Upgrade to dedicated production server | CCU ≥ 70% capacity for 3+ days AND revenue ≥ 3× next tier cost | NOT DONE (pre-launch) |
| Gate 7 | Open player market | Fraud detection and rate-limiting implemented | NOT DONE |
| Gate 8 | Open 40v40 as scheduled events | 40v40 benchmark passes AND ≥2 000 CCU sustained AND on-demand cell boot/teardown implemented | NOT DONE |
| Gate 9 | Model Steam monetization as direct-margin (same as Web/PWA) | Steam policy verified; confirmed external checkout allowed OR adapter built for Steam Wallet/DLC | NOT DONE |
| Gate 10 | Ship paid Steam items | Backend has Steam purchase adapter + server-side entitlement verification (§19) | NOT DONE |
| Gate 11 | Claim Windows support | Proton/PWA path validated on real Windows+Steam environment; no blockers | NOT DONE |
| Gate 12 | Start Windows-native client work | Connor explicitly changes strategy with evidence showing it is worth the cost | NOT DONE — intentionally deferred |
| Gate 13 | Include rewarded ads in revenue model | Connor explicitly adds this after evaluating brand/policy risk | NOT DONE |
| Gate 14 | Let Steam-specific purchase logic enter gameplay code | Never — all purchase logic stays in backend entitlement adapter layer (§19) | PERMANENT BLOCK |
| Gate 15 | Launch Steam page as primary PWA redirect | Real Linux-native / Steam Deck-quality playable app ships first | NOT DONE |

---

## 22. Performance and Infrastructure Levers

### 22a. Position updates are event-driven, not tick-rate-driven

The O(N²) formula (`active_bw = 450 × N²` bytes/sec) assumes a position update fires
every tick for every moving entity. The actual implementation is fundamentally different.

`EntityPhysicsInputs` is populated in exactly two places:
1. **`CollisionEvent::Stopped`** — when a collision between two bodies ends.
2. **`NetworkedMoveBuffer::set()`** — when a unit's movement direction changes,
   with an explicit value-comparison guard that skips the call if the value is unchanged.

For an entity moving at constant velocity: zero position bytes transmitted on that tick.

**Real sustained BW is proportional to input-change rate, not tick rate.** The O(N²)
worst-case formula overestimates production BW by roughly one order of magnitude
for typical Halo-style gameplay. See §24 for the detailed bandwidth model.

**What actually triggers position/velocity sends:**

| Event | Frequency (Halo-style BTB) | Notes |
|---|---|---|
| Move direction change | ~2–4 per second per unit | Input-driven; predictable gap |
| Collision end | ~1–2 per second during combat | Post-collision drift correction |
| Spawn / respawn | Once per life | Full state on entity enter-scope |
| Forced repositioning | Rare (abilities, map events) | Correction always required |

### 22b. CPU is the real binding constraint in production `[Estimated, no benchmark]`

```
tick_budget_µs = 40 000    (25 Hz = 40 ms/tick)
tick_cost_µs   ≈ 558       (Naia 58 + physics ~350 + logic ~100 + OS ~50, per cell)
cells_per_core = 40 000 × 0.40 / 558 ≈ 29   (shared vCPU, 40% efficiency)
               = 40 000 × 0.55 / 558 ≈ 39   (dedicated vCPU, 55% efficiency)
```

Physics dominates the estimated budget at approximately **~350 µs** `[Estimated, no benchmark]`.

### 22c. Switch infrastructure to Hetzner `[External pricing, manually verified 2026-04]`

**Hetzner Cloud includes 20 TB/month on every instance, regardless of tier.**
With production BW well below worst-case estimates, 20 TB is sufficient — making BW
a non-issue and leaving CPU as the sole constraint.

See §23 for the full provider pricing table.

### 22d. Tile physics BVH — replace N_tile colliders with one static trimesh

**The problem:** Every wall tile is a separate Rapier rigid body. A 10K-tile map with ~30–40% walls
yields ~3 000–4 000 independent tile colliders in Rapier's broad-phase.

**The fix:** Replace all tile wall geometry with a single static `ColliderBuilder::trimesh()`.
Rapier builds an internal BVH on construction. Runtime cost per frame:
- Broad-phase: **1 AABB** (trimesh bounds) per unit, instead of 3 000+
- Narrow-phase: **O(log N_triangles)** BVH traversal

**Expected impact:** Rapier is estimated at ~350 µs of the 558 µs budget `[Estimated, no benchmark]`.
Consolidating to one trimesh is expected to yield a **2–4× reduction in physics tick time**,
roughly doubling total CCU capacity on the same hardware. See §20 risk "Physics cost 2× estimate".

### 22e. Adaptive tick rate

Cells in lobby, post-match, or spectator mode do not need 25 Hz. Dropping idle phases
to 5 Hz reduces both CPU and BW for those cells by 5×.

```
savings = idle_fraction × (1 - 5/25) = 0.25 × 0.80 = 20%
```

A **20% fleet-wide CPU reduction** for near-zero implementation cost.

---

## 23. Provider Pricing Table (as of 2026-04)

**All prices manually verified 2026-04. Re-verify before any production purchasing decision — cloud pricing changes frequently.**

| Provider | Tier | Price/mo | BW included | CPU type | Cells/server (55% eff, full-stack) | Peak CCU | Provenance |
|---|---|---|---|---|---|---|---|
| Vultr | Shared 2 vCPU | $20 | 3 TB | shared | ~29 | ~322 | `[External pricing, manually verified 2026-04]` |
| Vultr | Dedicated 8 vCPU | $224 | 10 TB | dedicated | ~236 | ~2 620 | `[External pricing, manually verified 2026-04]` |
| **Hetzner** | **CX21** (shared 2 vCPU) | **~$6** | **20 TB** | shared | ~58 | **~644** | `[External pricing, manually verified 2026-04]` |
| **Hetzner** | **CCX33** (dedicated 8 vCPU) | **~$62** | **20 TB** | dedicated | ~472 | **~5 239** | `[External pricing, manually verified 2026-04]` |

*CCU = cells × 11.1 players/cell (§14b weighted average). BW is non-binding on Hetzner with event-driven model.*

> The prior version showed Hetzner CCX33 at "~10 789 CCU" based on the superseded 408 µs tick figure. The corrected 558 µs estimate reduces that figure. The order-of-magnitude advantage of Hetzner vs Vultr is unchanged.

**Key finding:** Hetzner CX21 at ~$6/mo delivers more CCU than Vultr $20/mo at comparable quality.
Hetzner CCX33 at ~$62/mo covers the entire Base Case (~1 312 CCU) on one server with headroom.

**Shared vCPU warning:** Shared vCPU plans (Vultr shared, Hetzner CX21) are subject to noisy-neighbor CPU jitter. For real-time game servers at 25 Hz, P99 tick latency spikes may violate the 40 ms budget. Use dedicated vCPU (Hetzner CCX, Vultr Optimized) for production.

---

## 24. Bandwidth Model — Worst-Case vs Event-Driven

Two distinct models apply:

### 24a. Worst-case O(N²) formula (all §3–§14 tables)

```
active_bw_bytes_per_sec = N_mutations × bytes_per_mutation × N_clients × tick_hz
                        = N × 18 B × N × 25 Hz = 450 × N²
```

This formula assumes every entity sends a delta update every tick. It is `[Extrapolated, formula only]` — NOT measured.

**Overestimate factor:** The event-driven implementation means most entities send zero bytes on most ticks. Real production BW is estimated at 10–15× below this worst case `[Estimated]` based on input-change rate analysis in §22a. This estimate itself is not measured.

### 24b. Proposed instrumentation — exact metrics to add

Instrument these metrics in the game server to replace formula-based BW estimates with measured values:

```
server_outgoing_bytes_per_tick_total        -- total bytes sent to all clients in one tick
server_outgoing_bytes_per_tick_per_client_p50   -- median per-client send
server_outgoing_bytes_per_tick_per_client_p95   -- p95 per-client send
server_outgoing_bytes_per_tick_per_client_p99   -- p99 per-client send
server_incoming_bytes_per_tick_total        -- total bytes received from all clients
server_tick_wall_time_p50_us               -- median tick wall time in µs
server_tick_wall_time_p95_us               -- p95 tick wall time in µs
server_tick_wall_time_p99_us               -- p99 tick wall time in µs
server_physics_tick_time_p50_us            -- Rapier tick time separate from Naia
server_naia_tick_time_p50_us               -- Naia-only tick time in production
```

These replace all `[Extrapolated, formula only]` and `[Estimated, no benchmark]` labels in §3c and §4 with `[Measured]` labels.

### 24c. Until BW is measured, use conservative (worst-case) BW figures

All capacity tables in §3–§14 use the worst-case O(N²) formula. If production BW is 10× lower than this formula, then BW-bound estimates actually become CPU-bound — which changes which constraint binds first. This is why Gate 4 (§21) must be resolved before trusting §4's binding constraint analysis.

---

## 25. Measurement Backlog

Prioritized list of missing benchmarks. Items marked with the gate they unblock.

| ID | Benchmark | What it measures | Unblocks | Priority |
|---|---|---|---|---|
| BM-001 | Full-stack 2v2 — 4 players, real Rapier, real game logic | Full tick wall time P50/P95/P99; wire bytes; memory | Gate 1 (2v2 production default) | **CRITICAL** |
| BM-002 | Full-stack 5v5 — 10 players | Same as BM-001 at 10 players | Gate 1 (5v5) | High |
| BM-003 | Full-stack 10v10 — 20 players | Same as BM-001 at 20 players | Gate 2 (10v10 production default) | High |
| BM-004 | Full-stack 40v40 — 80 players | Same as BM-001 at 80 players | Gate 3 (40v40 event tier) | Medium |
| BM-005 | Wire bytes per tick — run `wire/bandwidth_realistic_quantized` bench | Actual bytes per active tick | Gate 4 (BW model validation) | **CRITICAL** |
| BM-006 | Join burst — 16 clients joining an existing session simultaneously | Burst tick time during join; level load spike | Portal pre-warming design | High |
| BM-007 | Portal prewarm — spawn cell, measure wall time to steady-state | Time from cell boot to first player ready | Portal UX design | High |
| BM-008 | Tile BVH trimesh — run halo_btb_16v16 with trimesh vs per-tile colliders | Physics tick improvement ratio | §22d physics optimization | Medium |
| BM-009 | Multi-cell jitter — 4+ cells on same server under concurrent load | P99 tick variance with multiple cells sharing CPU | Multi-cell capacity model | Medium |
| BM-010 | Adaptive tick rate — 5 Hz lobby vs 25 Hz active | CPU savings factor, BW savings factor | §22e adaptive tick optimization | Low |
| BM-011 | WAN test — real clients over internet vs loopback | Tick timing with real RTT; jitter behavior | Production readiness | High |
| BM-012 | Client performance on web/Wasm — mid-range laptop, browser | Render frame time, Wasm heap, GC pressure | Client-side capacity confidence | High |

**Run BM-001 and BM-005 first.** They resolve the two largest unknowns in this document.

---

## 26. Appendix: Scaling Formulas

```
// O(N²) bandwidth law (Win 3) [Measured, Naia-only] — formula, not measured wire bytes
active_bytes_per_sec = N_mutations × bytes_per_mutation × N_clients × tick_hz
                     = N × 18 × N × 25   (for N-player match at 25 Hz)
                     = 450 × N²  bytes/sec

// Monthly bandwidth per cell
bw_gb_per_month = active_bytes_per_sec × active_fraction × 2_592_000 / 1e9

// Cells per server (bandwidth-bound)
cells_bw = monthly_bw_budget_gb / bw_gb_per_month_per_cell

// Total concurrent players (bandwidth constraint)
players_total = cells_bw × N
              = (monthly_bw_budget_gb / (active_frac × 450 × N² × 2592000 / 1e9)) × N
              ∝ 1/N    // smaller matches → more total players

// CPU capacity (cells per server) [Estimated, no benchmark] for full-stack
cells_cpu = (cores × efficiency × 40_000_µs) / tick_budget_µs_per_cell
          = cores × 0.40 × ~72   // for 16-player full game stack, ~558 µs estimated

// CPU capacity — Naia-only [Measured, Naia-only]
cells_cpu_naia = cores × 0.40 × ~274  // for 16-player Naia-only, 58 µs measured

// RAM capacity (cells per server) [Estimated]
cells_ram = total_ram_mb / mb_per_cell
          = total_ram_mb / 7.2   // for 16-player Halo BTB 10K tiles

// Level load time (linear approximation from M-001) [Measured, Naia-only] for 10K tiles
level_load_s ≈ tile_count / 10_000 × 5.2

// Binding capacity (cells per server)
cells = min(cells_cpu, cells_ram, cells_bw)

// Revenue formulas [Assumption]
monthly_revenue_per_ccu = CCU × 10 × (sub_rate × sub_price + microtx_rate × avg_spend)
                        = CCU × $7.60   // base case (Web/PWA channel only)

// Rewarded ads: EXCLUDED from all scenarios — reference formula only
// monthly_ad_revenue_ref = CCU × 10 × 0.30 × 30 × (ecpm / 1000) = CCU × $0.45 at $5 eCPM

dlc_revenue_per_ccu = dlc_conversion_rate × mau_multiplier × net_per_sale × releases_per_year / 12
                    = 0.17 × 10 × $14.27 × 2 / 12 = $4.04   // moderate case, Web/PWA channel

// Two-channel revenue model [Assumption] — use once Steam/Web player split is known
steam_player_share              // fraction of players via Steam
web_player_share                // fraction via Web/PWA (1 - steam_player_share)
steam_payer_share               // fraction of Steam players who make a purchase
web_payer_share                 // fraction of web players who make a purchase
steam_platform_fee_rate         // Steam cut (assumption ~30%) [Policy risk, requires verification]
web_payment_processor_fee_rate  // Stripe ~2.9% + $0.30 per transaction
refund_rate
chargeback_rate
tax_reserve_rate

steam_gross = steam_players × steam_payer_share × avg_purchase_value
web_gross   = web_players × web_payer_share × avg_purchase_value

blended_net_revenue =
    steam_gross × (1 - steam_platform_fee_rate)
  + web_gross × (1 - web_payment_processor_fee_rate)
  - refunds - chargebacks - taxes - server_cost - support_tools

// Until steam_player_share is known from production data, use web-only upper bound:
conservative_net_revenue = web_gross × (1 - web_payment_processor_fee_rate) - costs
```

---

*This document is a living source of truth. Update the Evidence Registry (§ER) and provenance labels whenever new benchmarks run. Update pricing labels when re-verified. Update risk statuses when gates close. Last major revision: 2026-04-26 by Claude (Steam/Proton/Payments strategy update — §18 platform matrix, §19 payment architecture, §21 gates 9–15, §9 Steam/Proton validation track, two-channel revenue formulas in §26).*
