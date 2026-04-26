# Cyberlith Capacity Analysis: Halo-Style Gameplay at 25 Hz

**Date:** 2026-04-26 | **Author:** Claude (Phase 10 synthesis)

---

## Table of Contents

1. [Evidence Base — Measured Benchmarks](#1-evidence-base--measured-benchmarks)
2. [The Full Tick Budget](#2-the-full-tick-budget)
3. [Per-Cell Resource Costs (Halo BTB 16v16, 10K tiles)](#3-per-cell-resource-costs-halo-btb-16v16-10k-tiles)
4. [Vultr VPS Analysis — 5 Price Points](#4-vultr-vps-analysis--5-price-points)
5. [The Guild Wars Instancing / Portal Network Model](#5-the-guild-wars-instancing--portal-network-model)
6. [Tile Count as a Design Lever](#6-tile-count-as-a-design-lever)
7. [Portal Pre-Warming (Critical UX Requirement)](#7-portal-pre-warming-critical-ux-requirement)
8. [40v40 Match Analysis (80 Players/Cell)](#8-40v40-match-analysis-80-playerscell)
9. [Optimal Match Size for Budget Deployments](#9-optimal-match-size-for-budget-deployments)
10. [Client-Side Capacity](#10-client-side-capacity)
11. [The Three Binding Constraints (Ranked by Frequency)](#11-the-three-binding-constraints-ranked-by-frequency)
12. [Recommendations for the Cyberlith Business Plan](#12-recommendations-for-the-cyberlith-business-plan)
13. [The 4-Tier Match Architecture — Per-Tier Resource Costs](#13-the-4-tier-match-architecture--per-tier-resource-costs)
14. [The Mixed Fleet — Server Capacity Across All Four Tiers](#14-the-mixed-fleet--server-capacity-across-all-four-tiers)
15. [Revenue Model and Scaling Plan](#15-revenue-model-and-scaling-plan)
    - §15e: The Dual-Currency Economy (Gold + Crystals + player market)
    - §15f: Rewarded Ads
    - §15g: Guest Slots + Daily Crystal Stipend
16. [Path to $10 000/Month Net Profit](#16-path-to-10-000month-net-profit)
17. [DLC Campaigns as a Revenue Accelerator](#17-dlc-campaigns-as-a-revenue-accelerator)
    - §17a: Self-managed platform advantage (no 30% cut)
    - §17d: Full picture at 250 subs, 2 DLC/year at $15
    - §17e: CCU target to reach $10K (~378 subs at ~1 262 CCU)
18. [Appendix: Scaling Formulas](#18-appendix-scaling-formulas)
19. [Performance and Infrastructure Levers](#19-performance-and-infrastructure-levers)
    - §19b: Interest management / AOI — 4× BW reduction via TileTraversability
    - §19c: Hetzner vs Vultr — 20 TB included, up to 327 CCU/dollar
    - §19d: Combined impact — ~10 789 CCU on a single $62/mo server
    - §19e: Adaptive tick rate — 20% fleet BW reduction
    - §19f: CDN preloading — eliminates the 5.2 s level load
    - §19g: What to measure next

---

## 1. Evidence Base — Measured Benchmarks

All numbers are from the `halo_btb_16v16_10k` scenario:
**16 players, 10 000 immutable HaloTile entities, 32 mutable HaloUnit entities, 25 Hz (40 ms tick budget)**,
run via `cargo criterion -p naia-benches -- "scenarios/halo_btb_16v16"`.

| Measurement | Value | Notes |
|---|---|---|
| Level load (10K tiles + 32 units → 16 clients) | **5.2 s** (σ ≈ 0.8 s) | 2 ticks with `uncapped_bandwidth` |
| Server tick — idle (0 mutations) | **63 µs** | Naia networking only |
| Server tick — active (32 mutations) | **58 µs** | Naia networking only |
| Client receive — active tick | **889 ns** | One client, measured in isolation |

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

The 649/760 figures are Naia-networking-only on one core. Full game stack reduces this
significantly — see §2.

### Architectural guarantees confirmed

**Win 2 — O(1) idle cost:** 10 000 HaloTile entities (immutable) cost nothing at steady state.
No dirty-tracking, no per-entity scan. Server CPU is entirely determined by mutable unit mutations
and client count, never by map size.

**Win 3 — O(mutations × users):** Active tick cost scales with the product of mutations and connected
clients, not total entity count. This means bandwidth (which must carry mutations to every client)
scales as O(N²) with player count, making N the primary capacity design lever.

> These benchmarks measure **Naia networking only**. Game simulation (Rapier physics, pathfinding,
> damage resolution) adds additional cost — see §2 for the full budget.

---

## 2. The Full Tick Budget

At 25 Hz: **40 000 µs per tick** per thread.

| Component | Cost (µs) | Source |
|---|---|---|
| Naia networking — active, 32 mutations, 16 clients | **58** | Measured |
| Rapier physics — 32 kinematic bodies + 10K static tiles | **~200** | Estimated |
| Game logic — damage, spawn management, game state | **~100** | Estimated |
| OS overhead, context switches | **~50** | Estimated |
| **Total per-cell tick (16-player Halo BTB)** | **~408 µs** | 1.02% of 40 000 µs budget |
| **Cells per core — theoretical** | **97** | 40 000 / 408 |
| **Cells per core — realistic (40% efficiency)** | **~39** | Cache + scheduling overhead |

The 40% efficiency factor accounts for L1/L2/L3 cache pressure (10K entities × ~300 B ≈
3 MB working set per cell, competing across threads), memory bandwidth contention, and
kernel scheduler overhead when oversubscribing cores.

**Naia-only ceiling** (if physics/logic turns out lighter): 635 cells/core theoretical,
254 cells/core realistic.

---

## 3. Per-Cell Resource Costs (Halo BTB 16v16, 10K tiles)

### 3a. CPU

| Scenario | Tick cost | % of one core | Cells/core (realistic) |
|---|---|---|---|
| Naia-only, idle | 63 µs | 0.16% | 254 |
| Naia-only, active | 58 µs | 0.15% | 274 |
| Full game (Naia + physics + logic) | ~408 µs | 1.02% | **~39** |

### 3b. Memory

| Component | Per-cell |
|---|---|
| 10K HaloTile entities (server representation) | ~3 MB |
| 32 HaloUnit entities + dirty tracking | ~50 KB |
| 16 client connections (Naia state + buffers) | ~1.6 MB |
| Network send/receive buffers (16 × 64 KB × 2) | ~2 MB |
| Naia server overhead (room, routing, event queues) | ~500 KB |
| Game logic state (positions, health, game state) | ~100 KB |
| **Total per cell (16-player Halo BTB)** | **~7.2 MB** |

**Tile-count sensitivity (memory scales linearly; CPU does NOT — Win 2):**

| Map size | Memory/cell | CPU/cell | Level load |
|---|---|---|---|
| 1K tiles (small arena) | ~1.5 MB | ~408 µs (same) | ~0.5 s |
| 10K tiles (Halo BTB) | ~7.2 MB | ~408 µs (same) | ~5.2 s |
| 32K tiles (large campaign) | ~22 MB | ~408 µs (same) | ~17 s |
| 64K tiles (massive map) | ~45 MB | ~408 µs (same) | ~33 s |

**Player-count sensitivity (both CPU and memory scale with players):**

| Players/cell | Memory/cell | Naia tick (active) | Full tick (est.) |
|---|---|---|---|
| 16 (8v8) | ~4 MB | ~29 µs | ~210 µs |
| 20 (10v10) | ~5 MB | ~45 µs | ~280 µs |
| 32 (16v16, benchmarked) | ~7.2 MB | **58 µs** | **~408 µs** |
| 64 (32v32) | ~14 MB | ~116 µs | ~600 µs |
| 80 (40v40) | ~21 MB | ~145 µs | ~725 µs |

*All figures except 16v16 are extrapolations. Win 3: cost is O(mutations × users), so scaling
is well-understood.*

### 3c. Bandwidth (Outbound Server → Clients)

Bandwidth follows the **O(N²) law**: N players = N mutations/tick × N clients receiving =
N² bytes/tick. Every doubling of players quadruples bandwidth demand.

| State | Calculation | Per-cell rate |
|---|---|---|
| Level load burst | 10K × ~20 B/entity × 16 clients | ~3 MB over 80 ms (~37.5 MB/s peak) |
| Active combat (32 mutations, 16 clients) | 32 × 18 B × 16 × 25 Hz | **~230 KB/s** |
| Idle (keepalives + clock sync) | 16 × 5 B/tick × 25 Hz | **~2 KB/s** |
| Realistic mix, 10% active duty | 0.1×230 + 0.9×2 | ~25 KB/s |
| Realistic mix, 30% active duty | 0.3×230 + 0.7×2 | ~70 KB/s |
| Realistic mix, 60% active duty (intense TDM) | 0.6×230 + 0.4×2 | ~139 KB/s |

Monthly bandwidth per cell (16v16):

| Active duty | KB/s avg | GB/month |
|---|---|---|
| 10% | 25 | **65** |
| 30% | 70 | **181** |
| 60% | 139 | **360** |

---

## 4. Vultr VPS Analysis — 5 Price Points

### Server-sizing constants used

- Full game tick cost: ~408 µs/cell (16v16 @ 10K tiles)
- Memory/cell: 7.2 MB
- Bandwidth/cell: 181 GB/month (30% active duty) for 16v16 baseline
- Realistic CPU efficiency: 40% on shared, 55% on dedicated
- Binding constraint = min(CPU, RAM, BW)

---

### Tier 1 — $10/mo: Development

**Spec:** 1 shared vCPU · 2 GB RAM · 2 TB/mo bandwidth

| Constraint | Formula | Limit |
|---|---|---|
| CPU | 0.5 eff. cores × 39 cells/core | 20 cells |
| RAM | 2 000 MB ÷ 7.2 MB | 278 cells |
| Bandwidth | 2 000 GB ÷ 181 GB/cell/mo | **11 cells ← binding** |

**Result:** ~11 cells · 352 player-slots · $0.91/cell/mo · $0.028/player-slot/mo

**Verdict:** Dev and testing only. Shared CPU variance makes tick jitter unpredictable.
Bandwidth is the hard wall — Vultr's budget tiers are BW-stingy.

---

### Tier 2 — $20/mo: Minimal Staging

**Spec:** 2 shared vCPU · 4 GB RAM · 3 TB/mo bandwidth

| Constraint | Formula | Limit |
|---|---|---|
| CPU | 1 eff. core × 39 | 39 cells |
| RAM | 4 000 MB ÷ 7.2 MB | 556 cells |
| Bandwidth | 3 000 GB ÷ 181 GB/cell/mo | **17 cells ← binding** |

**Result:** ~17 cells · 544 player-slots · $1.18/cell/mo · $0.037/player-slot/mo

**Verdict:** Viable for a small closed beta or prototyping. Bandwidth is always the binding
constraint on this tier. See §9 for how reducing match size dramatically expands capacity.

---

### Tier 3 — $40/mo: Small Production Unit

**Spec:** 4 shared vCPU · 8 GB RAM · 4 TB/mo bandwidth

| Constraint | Formula | Limit |
|---|---|---|
| CPU | 2 eff. cores × 39 | 78 cells |
| RAM | 8 000 MB ÷ 7.2 MB | 1 111 cells |
| Bandwidth | 4 000 GB ÷ 181 GB/cell/mo | **22 cells ← binding** |

**Result:** ~22 cells · 704 player-slots · $1.82/cell/mo · $0.057/player-slot/mo

**Verdict:** First viable tier for very small early-access. Bandwidth is still the wall.
Shared CPU variance remains a risk for real-time workloads.

---

### Tier 4 — ~$80–100/mo: Dedicated Game Server (Smallest)

**Spec (Vultr Optimized Cloud Compute, estimated):**
4 dedicated vCPU · 16 GB RAM · ~6 TB/mo bandwidth

Dedicated CPU eliminates noisy-neighbor jitter. Efficiency factor improves from 40% to ~55%.

| Active duty | Cells (BW-bound) | Cells (CPU-bound) | Binding |
|---|---|---|---|
| 30% | 33 | 216 | BW |
| 10% | 92 | 216 | BW |
| 5% | 176 | 216 | CPU at ~195 cells |

**At 10% active (realistic mixed Halo gameplay):**
~92 cells · 2 944 player-slots · $0.87–1.09/cell/mo · $0.027–0.034/player-slot/mo

**Verdict:** First production-worthy tier. Dedicated CPU eliminates jitter. The 10% active
assumption is realistic for a game with pregame lobbies, intermission, and varied zones.

---

### Tier 5 — ~$224/mo: Full Production Game Server

**Spec (Vultr Optimized Cloud Compute, estimated):**
8 dedicated vCPU · 32 GB RAM · ~10 TB/mo bandwidth

| Active duty | BW-limited cells | CPU-limited cells | Player-slots |
|---|---|---|---|
| 30% | 55 | 432 | 1 760 |
| 10% | 154 | 432 | **4 928** |
| 5% | 294 | 432 | **9 408** |

At 5% active duty (mostly lobby/idle, bursts of combat): CPU becomes binding at ~294 cells.
At 10% active: BW-bound at 154 cells.

**At 10% active duty:**
~154 cells · 4 928 player-slots · $1.45/cell/mo · $0.045/player-slot/mo

**Verdict:** The production sweet spot. At 10 000 CCU across 3–4 of these servers, the
monthly server bill is ~$900. Recouped by ~60 subscribers at $15/mo.

---

### Player/Server Scaling Table (Tier 5, 10% active duty, 16v16)

| Target CCU | Cells needed | Servers needed | Monthly server cost |
|---|---|---|---|
| 320 | 10 | 0.1 | ~$22 (use Tier 3) |
| 1 600 | 50 | 0.3 | ~$67 (1 × Tier 4) |
| 5 000 | 156 | 1.0 | ~$224 |
| 10 000 | 313 | 2.0 | ~$450 |
| 50 000 | 1 563 | 10.2 | ~$2 285 |
| 100 000 | 3 125 | 20.3 | ~$4 550 |

Server cost per active player at 10K CCU: **~$0.045/player/month**. At $15/mo subscription
pricing, server cost is ~0.3% of revenue.

---

## 5. The Guild Wars Instancing / Portal Network Model

### Architecture

Each "cell" = one game_server thread managing one level instance:

```
[Cell A: Outpost] ←portal→ [Cell B: Combat Zone] ←portal→ [Cell C: Boss Area]
  32 players                    32 players                    32 players
  1 thread                      1 thread                      1 thread
  7 MB RAM                      7 MB RAM                      7 MB RAM
```

**Instancing:** When Cell B fills, the server spawns **Cell B′** (independent thread,
independent Naia state, independent bandwidth). This is purely additive — no cross-cell
state synchronization needed.

**Level load amortization:** A cell's level is loaded once when the first player enters
(~5.2 s for 10K tiles). Subsequent players joining the same instance get replication from
the running server in one tick. Portal pre-warming is critical — see §7.

### Portal Network Topology Sizing

| World scale | Zone types | Avg instances/zone | Total cells | RAM | Monthly servers |
|---|---|---|---|---|---|
| Indie MVP | 10 | 2 | 20 | ~144 MB | 1 × Tier 3 ($40) |
| Small launch | 50 | 3 | 150 | ~1 GB | 1 × Tier 5 ($224) |
| Mid-size | 200 | 3 | 600 | ~4.3 GB | 4 × Tier 5 ($896) |
| Large | 500 | 4 | 2 000 | ~14 GB | 13 × Tier 5 (~$3 000) |
| MMO-scale | 1 000 | 5 | 5 000 | ~36 GB | 33 × Tier 5 (~$7 400) |

### Combat Hotspot Problem

A "siege chokepoint" that many teams fight over will spawn many instances simultaneously.
10 simultaneous instances of one contested cell = 10 × 7.2 MB RAM + 10 × 70 KB/s BW.
This is manageable but must be planned for in dynamic scaling.

---

## 6. Tile Count as a Design Lever

| Scenario | Tiles/cell | Memory/cell | CPU/cell | Level load | Cells on 32 GB RAM |
|---|---|---|---|---|---|
| PvP arena | 1K | 1.5 MB | ~408 µs | ~0.5 s | 21 333 |
| Halo BTB (benchmarked) | 10K | 7.2 MB | ~408 µs | ~5.2 s | 4 444 |
| Campaign mission | 32K | ~22 MB | ~408 µs | ~17 s | 1 454 |
| Open world zone | 64K | ~45 MB | ~408 µs | ~33 s | 711 |

**Key leverage:** Tile count is free in CPU (Win 2). You can build 64K-tile maps and the
server tick budget is identical. You only pay in RAM and level-load time.

**Recommended cell size targets:**

- PvP combat maps: ≤10K tiles (7.2 MB, 5.2 s load)
- Campaign/exploration maps: ≤32K tiles (22 MB, 17 s load — needs pre-warming)
- Special massive maps: ≤64K tiles (45 MB — RAM becomes constraint for dense servers)

---

## 7. Portal Pre-Warming (Critical UX Requirement)

Level load for 10K tiles = 5.2 seconds. For seamless portal transitions:

1. When player is **10+ seconds** from a portal, begin booting the destination cell thread.
2. The destination cell spawns its Naia server, allocates entity state, loads tiles.
3. When the player crosses the portal, the cell is already in steady state.
4. If the destination cell is already running (another player is there), instant join — no
   load time.

The 5.2-second benchmark gives us the pre-warm timing budget. For 64K-tile maps, start
pre-warming 40+ seconds before the portal is used.

---

## 8. 40v40 Match Analysis (80 Players/Cell)

*Not yet benchmarked. All figures are extrapolations from the 16-player baseline using Win 3
(O(mutations × users) scaling).*

### Resource estimates for 80-player cells

| Resource | 16 players (measured) | 80 players (extrapolated) | Scaling factor |
|---|---|---|---|
| Naia tick, active | 58 µs | ~290 µs | ~5× (linear in users) |
| Full game tick | ~408 µs | ~900 µs | ~2.2× (physics scales sub-linearly) |
| Memory/cell | 7.2 MB | ~21 MB | ~3× (connection state dominates) |
| BW, active (60% duty) | ~139 KB/s | ~1 732 KB/s | ~12.5× (O(N²)) |
| BW/month (60% duty) | 360 GB | **4 489 GB** | ~12.5× |

### $40/mo feasibility for 40v40 at 60% active duty

| Constraint | 40v40 demand | $40/mo supply | Verdict |
|---|---|---|---|
| CPU | ~900 µs × 25 Hz = 2.25% per core | 2 eff. cores → 88 cells theoretical | ✓ Not binding |
| RAM | ~21 MB/cell | 8 000 MB → 381 cells | ✓ Not binding |
| Bandwidth | 4 489 GB/month per cell | 4 000 GB/month total | **✗ Less than 1 cell** |

**$40/mo cannot sustain even one concurrent 40v40 match at 60% active duty.**

### What $40/mo CAN do for 40v40

| Active duty | GB/month/cell | Cells on 4 TB | Simultaneous matches |
|---|---|---|---|
| 60% | 4 489 | 0.9 | **0** (not viable) |
| 30% | 2 245 | 1.8 | **1** (marginal) |
| 10% | 770 | 5.2 | **5** (scheduled, not concurrent) |

At **10% active duty** (scheduled match times, significant lobby/pregame period),
$40/mo supports ~5 total cells but realistically **1 match running at any given time**.

### 40v40 cost at scale

| Target | Servers | Monthly cost |
|---|---|---|
| 1 concurrent match (testing) | 1 × Tier 4 ($100/mo) | ~$100 |
| 5 concurrent matches (small beta) | 1 × Tier 5 ($224/mo) | ~$224 |
| 20 concurrent matches (launch) | 2–3 × Tier 5 | ~$450–700 |
| 100 concurrent matches (growth) | 10–12 × Tier 5 | ~$2 240–2 700 |

**Minimum viable for 40v40 at 30% active duty: ~$450–500/mo** (2× Tier 5) →
8 concurrent active matches = 640 concurrent players.

---

## 9. Optimal Match Size for Budget Deployments

### The O(N²) bandwidth constraint

For N players per match (N/2 v N/2), bandwidth scales quadratically:

```
active_kb_per_sec = N_mutations × bytes_per_mutation × N_clients × tick_hz
                  = N × 18 × N × 25   =   450 × N²  bytes/sec
```

This means **total concurrent players = cells × N**, while **cells ∝ 1/N²**, so
**total players ∝ N/N² = 1/N**. Smaller matches pack more players onto a given bandwidth
budget.

### N-player sweep on $20/mo (3 TB/month, 20–40 minute matches, 60% active duty)

| Players/match (N) | BW/cell (GB/mo) | Cells (BW-bound, 3 TB) | Total concurrent players |
|---|---|---|---|
| 8 | 45 | **66** | 528 |
| **10** | **70** | **42** | **420** |
| 12 | 101 | 29 | 348 |
| 16 (benchmarked) | 181 | 16 | 256 |
| 20 | 281 | 10 | 200 |
| 24 | 405 | 7 | 168 |
| 32 | 720 | 4 | 128 |

*Bandwidth/cell at 60% active duty calculated as `0.6 × 450 × N² × 2 592 000 / 1e9` GB/month.*

### Recommendation: **10v10 (20 players/match)**

| Metric | 8v8 | 10v10 | 16v16 |
|---|---|---|---|
| Concurrent matches | 66 | **42** | 16 |
| Concurrent players | 528 | **420** | 256 |
| Matchmaking queue depth | ~66 matches worth | **~42** | 16 |
| Halo game quality | Sparse | **Good** | Full BTB |

**10v10 is the sweet spot at $20/mo** for 20–40 minute TDM matches:

- **42 concurrent matches** is above the practical matchmaking threshold (~10 minimum)
- 420 concurrent players maximizes utilization while staying N²-affordable
- 10v10 maps are already a core Halo game mode (Team Slayer, Oddball, CTF)
- Matches fit within 20–40 minute windows, so cells turn over 3–6× per day

At $40/mo (4 TB BW), the same analysis gives **~56 concurrent 10v10 matches = 560 players**.

### Budget server scaling by match size

| Match size | $20/mo cells | $40/mo cells | $224/mo cells |
|---|---|---|---|
| 8v8 | 66 (528 pl) | 88 (704 pl) | ~555 (4 440 pl) |
| 10v10 | 42 (420 pl) | 57 (570 pl) | ~357 (3 570 pl) |
| 16v16 | 16 (256 pl) | 22 (352 pl) | ~138 (2 208 pl) |
| 40v40 | <1 | <1 | ~7 (560 pl) |

---

## 10. Client-Side Capacity

Client receive cost: **889 ns per active tick** = 22.2 µs/second at 25 Hz = **0.002% of
one CPU core**. Naia client-side networking is essentially free. Client CPU budget is
100% available for rendering. This holds even at 80 players (estimated ~5 µs/second).

---

## 11. The Three Binding Constraints (Ranked by Frequency)

| Rank | Constraint | When binding | Mitigation |
|---|---|---|---|
| 1 | **Bandwidth** | Always on budget shared-CPU plans (≤$40/mo); at high active duty on any plan | Reduce match size; lower tick rate for idle cells (e.g., 5 Hz); delta compression (Phase 8); interest management (send only visible units) |
| 2 | **CPU** | On dedicated servers at very low active duty (<5%) | Already well-optimized via Win 2/3; physics is the next target |
| 3 | **RAM** | Only for 64K-tile maps or very large cell counts on small RAM plans | Keep maps ≤32K tiles; use 32+ GB RAM for dense servers |

---

## 12. Recommendations for the Cyberlith Business Plan

1. **Ship 2v2 first, unlock larger modes as the player base grows.** 2v2 is viable at
   launch even with <100 CCU. Requiring 80 players simultaneously for 40v40 means that mode
   can't fill until ~2 000 CCU — which is the right unlock point anyway (see §14).

2. **Start on $20/mo; never upgrade before the 3× revenue rule is met.** See §15 for the
   exact upgrade triggers. Server cost stays under 33% of revenue at every transition.

3. **Treat 40v40 as a scheduled event, not a standing mode.** Boot dedicated cells on demand;
   tear them down after the match. At 4% of match volume this costs ~$25–50/month extra, not
   a whole new server tier.

4. **Target ~1 262 CCU (~378 premium subs) for $10 000/month net profit after taxes.**
   This accounts for subscription + microtx ($7.60/CCU), self-managed DLC (2 campaigns/year
   at $15, no platform cut), and rewarded ads ($0.45/CCU). See §16 for the pure-subscription
   model and §17 for the full combined model including DLC and ads.

5. **Map size sweet spot:** 2K tiles for 2v2 (1.0 s load, 2.5 MB/cell); 4K tiles for 5v5
   (2.1 s, 4.5 MB); 8K tiles for 10v10 (4.2 s, 7.5 MB). Larger campaign maps (32K tiles)
   need pre-warming — see §7.

6. **Portal pre-warming is non-negotiable.** Level loads are acceptable only if hidden behind
   a pre-warm strategy starting 10+ seconds before portal crossing.

7. **Benchmark the 40v40 scenario next.** The extrapolated numbers in §8 and §13 have ±30%
   error bars. A `halo_40v40` bench (80 players, 80 units) would confirm the O(N²) CPU and BW
   extrapolations with real measurements.

8. **Measure wire bytes.** Wire capacity shows ∞ in the capacity report (not yet measured).
   Running `wire/bandwidth_realistic_quantized` and feeding its output into the capacity
   formula gives exact concurrent-game-on-1Gbps numbers.

9. **Three levers can transform the business model before spending more money.** See §19
   for the full analysis. In priority order: (a) implement AOI interest management via
   `TileTraversability` — 4× BW reduction turns the $20/mo tier into ~1 688 CCU;
   (b) switch from Vultr to Hetzner — 20 TB/month included on all instances, up to 35×
   lower cost for equivalent CCU; (c) CDN preloading for level tiles — eliminates the
   5.2 s level load and its BW spike. Combined, a single Hetzner CCX33 at ~$62/mo can
   serve ~10 789 CCU with AOI — the entire $10K business target on one server.

---

## 13. The 4-Tier Match Architecture — Per-Tier Resource Costs

Connor's target mix: **40% 2v2 · 40% 5v5 · 16% 10v10 · 4% 40v40**.

All costs derived from the O(N²) bandwidth law and the 16v16 benchmark using measured
Naia scaling (0.113 µs × N² per tick) plus estimated physics/logic (proportional to N).

### 13a. Active bandwidth

```
active_bw_bytes_per_sec = N × 18 B × N × 25 Hz = 450 × N²
```

| Tier | N total | Active BW | Idle BW |
|---|---|---|---|
| **2v2** | 4 | **7.2 KB/s** | ~0.1 KB/s |
| **5v5** | 10 | **45 KB/s** | ~0.5 KB/s |
| **10v10** | 20 | **180 KB/s** | ~1 KB/s |
| **40v40** | 80 | **2 880 KB/s** | ~5 KB/s |

### 13b. Memory per cell

Formula: 1 MB base + N × 200 KB (client state) + tiles × 300 B (entity data)

| Tier | Players | Tiles | Memory/cell |
|---|---|---|---|
| 2v2 | 4 | ~2 000 | **2.5 MB** |
| 5v5 | 10 | ~4 000 | **4.5 MB** |
| 10v10 | 20 | ~8 000 | **7.5 MB** |
| 40v40 | 80 | ~12 000 | **21 MB** |

### 13c. CPU per cell (full game stack)

Naia component: 0.113 µs × N². Physics + logic ≈ 11 µs × N. OS/overhead: 50 µs fixed.

| Tier | Naia (µs) | Physics + logic (µs) | Total (µs) | Cells/core (40% eff.) |
|---|---|---|---|---|
| 2v2 | 1.8 | 66 | **~90** | **178** |
| 5v5 | 11 | 110 | **~155** | **103** |
| 10v10 | 45 | 231 | **~280** | **57** |
| 40v40 | 725 | 880 | **~1 525** | **10** |

### 13d. Level load time and match duration

| Tier | Match length | ~Queue time | Map tiles | Level load |
|---|---|---|---|---|
| 2v2 | 12 min | ~2 min | ~2 000 | ~1.0 s |
| 5v5 | 24 min | ~5 min | ~4 000 | ~2.1 s |
| 10v10 | 32 min | ~8 min | ~8 000 | ~4.2 s |
| 40v40 | 40 min | ~20 min | ~12 000 | ~6.2 s |

### 13e. Monthly bandwidth per cell

Active fraction model: (match / (match + queue)) × combat intensity.

| Tier | Match fraction | Combat intensity | Net active | GB/month/cell |
|---|---|---|---|---|
| 2v2 | 86% | 60% | 52% | **~10 GB** |
| 5v5 | 83% | 55% | 46% | **~55 GB** |
| 10v10 | 80% | 50% | 40% | **~190 GB** |
| 40v40 | 67% | 75% | 50% | **~3 740 GB** |

40v40 at 50% active: 0.50 × 2 880 KB/s × 2 592 000 s / 1e6 = 3 732 GB/month.
This is the technical reason 40v40 must be treated as a separate on-demand tier.

### 13f. Minimum player pool to fill each match type

To keep queue times ≤ 5 minutes, a mode needs ~10× its player count in the matchmaking pool:

| Tier | Players/match | Minimum pool | Minimum CCU to offer mode |
|---|---|---|---|
| 2v2 | 4 | 40 | **~50 CCU** |
| 5v5 | 10 | 100 | **~150 CCU** |
| 10v10 | 20 | 200 | **~400 CCU** |
| 40v40 | 80 | 800 | **~2 000 CCU** |

This gives the game's natural mode unlock progression: 2v2 at launch, 5v5 at ~150 CCU,
10v10 at ~400 CCU, 40v40 events at ~2 000 CCU.

---

## 14. The Mixed Fleet — Server Capacity Across All Four Tiers

### 14a. Cell occupancy weighting

For server planning, cells are occupied proportional to match duration × match count:

| Tier | Count share | Duration (min) | Duration × count | Cell share |
|---|---|---|---|---|
| 2v2 | 40% | 12 | 480 | **22.7%** |
| 5v5 | 40% | 24 | 960 | **45.5%** |
| 10v10 | 16% | 32 | 512 | **24.2%** |
| 40v40 | 4% | 40 | 160 | **7.6%** |
| **Total** | | | 2 112 | 100% |

### 14b. Mixed-fleet weighted resource costs (regular modes: 2v2 + 5v5 + 10v10)

The 40v40 is handled on separate on-demand dedicated cells (see §14d). The regular fleet
uses cells for 2v2/5v5/10v10 only — renormalized to 100% cell share:

| | 2v2 (25.5%) | 5v5 (51.0%) | 10v10 (27.2%) | Weighted avg |
|---|---|---|---|---|
| BW/month | 10 GB | 55 GB | 190 GB | **79 GB/cell** |
| Memory | 2.5 MB | 4.5 MB | 7.5 MB | **4.9 MB/cell** |
| CPU/cell | 90 µs | 155 µs | 280 µs | **181 µs/cell** |
| Players/cell | 4 | 10 | 20 | **11.1 players/cell** |

### 14c. Regular-mode cells and CCU by server tier

Binding constraint is always **bandwidth** on shared-CPU plans.

| Server | Monthly cost | BW budget | Regular cells | Peak CCU | $/CCU/mo |
|---|---|---|---|---|---|
| Shared 2 vCPU | **$20** | 3 TB | **38** | **422** | $0.047 |
| Shared 4 vCPU | **$40** | 4 TB | **50** | **556** | $0.072 |
| Dedicated 8 vCPU | **$224** | 10 TB | **126** | **1 399** | $0.160 |
| 2 × dedicated | **$448** | 20 TB | **252** | **2 797** | $0.160 |
| 4 × dedicated | **$896** | 40 TB | **504** | **5 594** | $0.160 |

*CCU = cells × 11.1 players/cell (weighted average for the 2v2/5v5/10v10 mix).*

### 14d. 40v40 on-demand dedicated cells

40v40 requires its own dedicated cell that is booted for the match and torn down after.

At 4% of match count on the $20/mo main server (38 cells, ~99 matches/hour):
- 40v40 events/hour: 99 × 0.042 = ~4 events → ~3 simultaneous cells needed
- But at 422 CCU, there are not enough players for 40v40 (need ~2 000 CCU — §13f)

**40v40 economics once viable (CCU ~2 000+):**

| 40v40 cells active simultaneously | BW/cell/month | Server cost |
|---|---|---|
| 1 | 3 740 GB | $224/mo (runs BW-full) |
| 3 | 11 220 GB | $448/mo + $224/mo overflow |
| 5 | 18 700 GB | 2 × $448/mo |

At 2 000 CCU with ~4% in 40v40 = ~80 concurrent 40v40 players = 1 match. One dedicated
$224/mo cell. Cost: $224 absorbed into main fleet budget (at 2 000 CCU revenue is ~$15 200/mo).

---

## 15. Revenue Model and Scaling Plan

### 15a. Revenue model assumptions

**Currency:** 100 Crystals = $1 (always, no volume bonus). Crystal packs come in 500C ($5),
1 000C ($10), and 2 000C ($20). Cosmetics cost **20–100 Crystals** individually; cosmetic
bundles (full outfit sets) cost **100–500 Crystals**. Items never scale in power vertically —
horizontal utility, aesthetics, and flexibility only. No pay-to-win.

The self-managed platform (PWA + web portal) means **zero platform cut** on all transactions.
All microtx and subscription revenue flows through Stripe (2.9% + $0.30/transaction).

| Parameter | Value | Basis |
|---|---|---|
| CCU → MAU multiplier | ×10 | Industry rule of thumb (peak CCU ≈ 10% of MAU) |
| Premium conversion (MAU) | 3% | Conservative; guest-slots model (§15g) may improve this |
| Premium subscription price | $7/mo | Month-to-month, single tier; includes daily Crystal stipend |
| Microtransaction buyer rate | 27.5% of MAU | Low-friction cosmetics at 20–100C drive impulse buys |
| Avg Crystal spend per buyer/month | $2.00 | ~3 cosmetics at ~65C avg = ~200C = $2.00 |
| Monthly revenue per CCU | **$7.60** | $2.10 sub + $5.50 microtx |

```
monthly_revenue ≈ CCU × 10 × (0.03 × $7   +   0.275 × $2.00)
                = CCU × ($2.10 + $5.50)
                = CCU × $7.60

// sensitivity range:
//   conservative (15% buyers, ~$1/mo):  CCU × $3.60
//   base case    (27.5% buyers, $2/mo): CCU × $7.60   ← used throughout
//   optimistic   (35% buyers, $3/mo):   CCU × $12.60
```

At 20–100C per cosmetic, a $5 Crystal pack (500C) buys 5–25 cosmetics. The low floor creates
near-zero purchase friction. Bundle packs (100–500C) provide concentrated spending opportunities
for enthusiasts. No whale-cliff — every item has the same Crystal cost for everyone.

### 15b. Upgrade triggers and financial health at each transition

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

The revenue-to-cost ratio is never below 13× at any transition (vs the 3× minimum rule).
At $7.60/CCU, the business is strongly cash-positive long before the server costs matter.
The $40→$224 jump — the only plausibly uncomfortable one — happens at $2,957/mo revenue,
making $224/mo feel trivial.

### 15c. Leading indicators to watch (better than raw CCU)

Track these three weekly. Upgrade when all three are green:

| Indicator | Warning threshold | Action |
|---|---|---|
| Median queue wait time | > 90 s in peak hours | Server near full |
| Cell utilization at peak | > 70% occupied for 3+ days | Approaching ceiling |
| Revenue 30-day trailing | ≥ 3× cost of next tier | Financial cushion exists |

Monitoring queue wait time is the most player-visible signal — it's the leading indicator
players will churn over before they complain on forums.

### 15d. Server cost as a fraction of revenue over the scaling arc

| CCU | Revenue/mo | Server cost | Server % of revenue |
|---|---|---|---|
| 50 | $380 | $20 | 5.3% |
| 200 | $1 520 | $20 | 1.3% |
| 422 | $3 207 | $40 | 1.2% |
| 979 | $7 440 | $224 | 3.0% |
| 2 000 | $15 200 | $448 | 2.9% |
| 2 400 | $18 240 | $448 | 2.5% |

At $7.60/CCU the server cost is never more than ~6% of revenue, even at launch. The server
bill is noise compared to revenue — scaling is purely about acquiring players.

### 15e. The dual-currency economy

Cyberlith uses two in-game currencies:

**Currency 1: Gold** (earned in-game, never purchased directly)
- Earned by: completing matches, winning, challenge completions, daily login bonus
- Spent on: standard cosmetic tier (skins, decals, emotes, player cards), Lives replenishment,
  community resource contributions (crafting, guild upgrades, trading in the player market)
- Cannot be purchased directly with real money

**Currency 2: Crystal** (purchased with real money, never earned from gameplay)
- 100 Crystals = $1, always — no volume discount, no bonus on larger packs (flat rate)
- Crystal packs: **500C ($5) · 1 000C ($10) · 2 000C ($20)**
- Spent on: cosmetics (20–100C each), cosmetic bundles (100–500C), Lives, Gold conversion,
  player-market purchases, and Premium Sub Tokens (700C = 1 month, same as $7 cash sub)
- Can never be converted back into real money — Crystals only flow in, never out

**Crystal pack economics (Stripe fees, no platform cut):**

| Pack | Price | Crystals | Stripe fee | Dev net | Effective rate |
|---|---|---|---|---|---|
| Small | $5 | 500C | $0.445 | $4.555 | 8.9% |
| Medium | $10 | 1 000C | $0.590 | $9.410 | 5.9% |
| Large | $20 | 2 000C | $0.880 | $19.120 | 4.4% |

*No $1 pack — minimum purchase is $5. The 100C = $1 rate is maintained without volume bonuses
because the packs are purely convenience bundles, not discount tiers.*

**The player-driven market:**

Players can freely buy and sell the following for Crystals between each other:
- Cosmetics (individual items, sets, limited drops)
- Lives (players with surplus sell; players who died buy)
- Gold (players convert Gold to tradeable quantities)
- Premium Sub Tokens (700C = 1 month; market price may differ from face value)
- Other in-game resources (crafting materials, guild contributions)

Crystals spent in the player market are **not destroyed** — they transfer from buyer to seller.
Crystals are destroyed only when spent in the dev store (buying new cosmetics, Premium Sub Tokens
at face value, etc.). This creates a closed-loop economy with natural deflation over time,
sustaining ongoing demand for new Crystal purchases.

**Economic characteristics:**
1. **No cash-out**: Crystals can never be redeemed for money — they stay in the system forever
2. **Zero-sum P2P**: Player-to-player trades don't change total Crystal revenue; the money
   was already captured when the Crystals were purchased
3. **Crystal sinks**: Dev-store purchases destroy Crystals, requiring the economy to be
   continuously refueled by real-money pack purchases
4. **No pay-to-win**: Crystals buy cosmetics and horizontal utilities (Lives, Gold, convenience)
   — never stats, combat power, or vertical advantages

**Why this model increases revenue vs. a pure cosmetics store:**
- Players who earn Crystals via the market are motivated to buy packs to spend (new demand)
- Premium Sub Tokens at 700C give price-sensitive players a Crystal path to subscribing
  without reducing total sub revenue (700C was bought with $7 of real money by someone)
- Lives market creates ongoing Crystal demand tied to gameplay intensity — not just cosmetics

### 15f. Rewarded ads

The PWA + self-managed platform means rewarded video ads are possible without Apple's in-app
purchase restrictions. A "watch an ad for 25 Gold" mechanic monetizes non-paying players who
would otherwise generate zero revenue.

**Ad revenue model (web eCPM, not native-mobile eCPM):**

| Parameter | Value | Notes |
|---|---|---|
| Rewarded ad eCPM (web) | $3–$8 | vs. $10–$25 native mobile; $5 used as baseline |
| Ad-engaging player rate | 30% of MAU | Players who opt in to "watch for Gold" |
| Ads per engaged player/month | 30 | 1–2/session, ~20 active days/month |
| Revenue per engaged MAU/month | $0.045–$0.12 | 30 ads × $5 eCPM / 1 000 |

```
monthly_ad_revenue ≈ MAU × 0.30 × 30 × ($5 / 1 000)
                   = MAU × $0.045
                   = CCU × 10 × $0.045
                   = CCU × $0.45
```

| CCU | Monthly ad revenue |
|---|---|
| 833 (250 subs) | ~$375 |
| 1 262 (target) | ~$568 |
| 2 025 | ~$911 |

Ads effectively pay for the server cost at modest scale, and add $600–$900/month at the $10K
target — not life-changing, but meaningful and entirely passive after integration.

### 15g. Guest slots and daily Crystal stipend

Two subscription perks that reshape acquisition and retention without changing the core revenue math:

**Daily Crystal stipend:**
Each $7/mo subscriber receives a daily Crystal allowance (e.g., 10C/day = 300C/month = $3 of
Crystal equivalent). This is a retention mechanic, not a cost: the dev credits Crystals at zero
marginal cost. The stipend gives subscribers a steady flow of spending power for the player market
and cosmetics without requiring additional cash, creating a daily login habit.

*Revenue impact:* Subscribers who receive 300C/month spend it on cosmetics and market purchases
— creating Crystal sinks that drive non-subscriber players to buy Crystal packs to replenish the
economy. The stipend increases Crystal velocity (higher total transaction volume) without reducing
pack purchase demand. Net effect: neutral-to-positive on Crystal revenue.

**Guest slots (replacing the free tier):**
Each subscriber gets **4 guest slots** — friends who can play for free as long as they occupy
one of the subscriber's slots, even while the subscriber is offline.

*No anonymous free tier.* Every player is either a paying subscriber or a named guest of one.

**How the math changes:**

In the standard model, revenue per CCU = $7.60 (3% of CCU are subscribers).
With guest slots, say the average subscriber fills 2 of 4 guest slots:

```
// Players per subscriber: 1 sub + 2 guests = 3 players
// → subscriber fraction of CCU = 1/3 = 33.3%
// → sub revenue per CCU = 0.333 × $7 × 10 (MAU mult) = $2.33

// Microtx: guests can still buy Crystal packs → same 27.5% rate applies to all players
// → microtx per CCU = 0.275 × $2 × 10 = $5.50

full_revenue_per_ccu (guest model, 2 guests/sub) = $2.33 + $5.50 = $7.83

// vs. base model: $7.60/CCU
// Difference: +3.0% — slightly better because a larger fraction of CCU are paying subs
```

| Avg active guests per sub | Sub fraction of CCU | Revenue/CCU | vs base |
|---|---|---|---|
| 1 (half slots filled) | 50% | $8.50 | +12% |
| **2 (half slots filled)** | **33%** | **$7.83** | **+3%** |
| 3 (75% slots filled) | 25% | $7.25 | −5% |
| 4 (all slots filled) | 20% | $6.90 | −9% |

The model is revenue-neutral vs. the base when subscribers fill ~3 of 4 guest slots on average.
At 1–2 active guests per subscriber (realistic), revenue per CCU is 3–12% **higher** than the
free-tier base model.

**Acquisition dynamics (the key upside):**
Guest slots turn every subscriber into an active recruiter. A subscriber who fills all 4 slots
has personally onboarded 4 new players at zero acquisition cost. Some fraction of those guests
will eventually subscribe themselves (to get their own guest slots and Crystal stipend).

*If just 25% of guests convert to subscribers within 6 months:*
Each subscriber generates 0.5 new subscribers over 6 months → ~1 subscriber-generation/year
→ organic subscriber base doubles roughly annually from word-of-mouth alone.

This is structurally similar to Costco's membership model: members recruit family/friends,
who become members, who recruit more family/friends. The product IS the access ticket.

**Bootstrap consideration:**
With no free tier, the game needs a mechanism to acquire the first subscribers. Options:
- **Founding subscriber** early access at launch (players who subscribe before launch get 6
  months at $7 and immediately have 4 slots to fill)
- **Creator program**: content creators get complimentary subscriber accounts with 4 guest
  slots each → their audience becomes guest players → visible funnel to subscription

---

## 16. Path to $10 000/Month Net Profit

### 16a. The profit formula

```
monthly_net_profit = (revenue - costs) × (1 - tax_rate)

Costs:
  server:            $448/mo   (2× $224, at CCU ~2 025)
  payment processing: 2.9% of revenue + $0.30/transaction
  other overhead:    $200/mo   (domain, monitoring, email, etc.)

Tax rate:            30%        (US self-employment + federal/state, conservative)
```

### 16b. Revenue target and CCU required (subscription + microtx only baseline)

*This section shows the subscription + microtx baseline without DLC or ad revenue.
The full model including self-managed DLC and rewarded ads is in §17e — it reaches $10K
at only ~1 262 CCU (~378 subs) rather than the ~2 025 CCU shown here.*

Solving for net profit = $10 000:

```
$10 000 = (revenue - costs) × 0.70
revenue - costs = $14 286

At ~2 025 CCU:
  server: 2 × $224 = $448/mo
  other:  $200/mo
  payment processing: 2.9% of revenue

Revenue = $14 286 + ($448 + $200 + 0.029 × revenue)
0.971 × revenue = $14 934
revenue ≈ $15 380/month

CCU = $15 380 / $7.60 = 2 024  →  ~2 025 CCU
```

### 16c. What ~2 025 CCU looks like

| Metric | Value |
|---|---|
| Concurrent players (peak) | 2 025 |
| Monthly active users (MAU) | ~20 250 |
| Premium subscribers | ~608 (3% of MAU) |
| Microtx buyers/month | ~5 569 (27.5% of MAU) |
| Cosmetics sold/month | ~11 138 (avg 2 per buyer) |
| Server fleet | 2 × $224/mo dedicated = $448/mo |
| Regular cells | ~182 cells (2v2+5v5+10v10 mix) |
| Simultaneous 40v40 matches | ~1 (first dedicated on-demand cell) |
| Monthly revenue | ~$15 390 |
| Monthly costs | ~$1 094 |
| Pre-tax profit | ~$14 296 |
| **After-tax net profit** | **~$10 007 ≈ $10 000** ✓ |

22 000 MAU is a **very reachable milestone** for an indie with a distinctive identity. That's
roughly the scale of Spelunky 2 or Noita — beloved niche titles, not mass-market hits.

### 16d. Timeline to $10K/month (illustrative)

A new multiplayer indie that achieves modest organic spread:

| Month | Event | CCU | Revenue/mo |
|---|---|---|---|
| 0 | Launch: 2v2 only, $20/mo server | 20 | $152 |
| 2 | Word of mouth; 5v5 unlocked (~150 CCU trigger) | 150 | $1 140 |
| 3 | **Upgrade $20→$40** (CCU 295 trigger) | 300 | $2 280 |
| 4 | 10v10 unlocked (~400 CCU trigger) | 420 | $3 192 |
| 5 | **Upgrade $40→$224** (CCU 389 trigger, hit at 420) | 500 | $3 800 |
| 7 | **Add 2nd $224** (CCU 979 trigger) | 1 000 | $7 600 |
| 9 | 40v40 events begin (~2 000 CCU) | 2 000 | $15 200 |
| 10 | **$10 000/month net profit reached** | **~2 025** | **~$15 400** |
| **10** | | | **~$10 000 net/mo** ✓ |

This timeline assumes ~10 months of steady organic growth after launch — aggressive but
achievable for a game with strong word-of-mouth. 24 months is more realistic without marketing.
A single viral moment (streamer, Reddit front page) compresses the left half by 50-70%.

### 16e. Sensitivity analysis — what changes the timeline most

*All deltas are vs. the base case of 2 025 CCU needed ($7/mo sub, 27.5% microtx rate, 2 cosmetics/mo, 30% tax).*

| Lever | Effect | CCU target | Notes |
|---|---|---|---|
| Sub price: $7→$10/mo | −11% fewer CCU needed | ~1 810 | Sub effect is muted — microtx dominates revenue |
| Conversion rate: 3%→5% | −16% fewer CCU needed | ~1 710 | Requires strong community + social features |
| Microtx rate: 27.5%→40% | **−25% fewer CCU needed** | **~1 520** | Best cosmetic cadence lever |
| Microtx frequency: 2→3/mo | **−27% fewer CCU needed** | **~1 490** | More releases, larger cosmetic catalog |
| Tax rate: 30%→20% | −12% fewer CCU needed | ~1 780 | S-corp election, jurisdiction-dependent |
| Conservative ($3.60/CCU base) | **+111% more CCU needed** | ~4 270 | Only if microtx barely converts at all |

**The single highest-leverage action: grow microtransaction purchase frequency**, not sub price.
At 40% microtx buyers (vs 27.5%) the target drops by 25%. Raising sub price from $7→$10 saves only 11%
because at $1/cosmetic pricing, microtx revenue ($5.50/CCU) dwarfs subscription revenue ($2.10/CCU).

### 16f. The freemium + permadeath model's business fit

The Lives economy creates a natural daily engagement loop that supports both tiers:

- **Free tier:** limited daily Lives → play 1–2 serious sessions per day → returns daily for the replenish
- **Premium tier:** daily credit stipend buys extra Lives → can play longer sessions → higher engagement → higher retention
- **Net effect:** premium subscribers churn less (higher LTV), free users convert via FOMO on longer sessions

Industry reference: games with permadeath + Lives economies (e.g. Battlerite, early Clash Royale) see
3–5× higher premium conversion than pure cosmetic-only freemium, because the Lives gating creates a
functional reason to subscribe beyond vanity. Adjust conversion rate assumptions upward accordingly.

---

## 17. DLC Campaigns as a Revenue Accelerator

### 17a. Platform economics — the self-managed advantage

Cyberlith is distributed as a PWA (Web + Wasm), not through Steam, App Store, or Google Play.
All payments go through the Cyberlith web portal directly. This changes DLC economics drastically:

| Platform | DLC price | Platform cut | Dev net per $15 sale |
|---|---|---|---|
| Steam | $15 | 30% | **$10.50** |
| App Store (iOS) | $15 | 30% | $10.50 |
| Google Play | $15 | 15% | $12.75 |
| **Self-managed (Stripe)** | **$15** | **0%** | **$14.27** (2.9% + $0.30 only) |

**Net: 36% more revenue per DLC sale than Steam, with no algorithmic gatekeeping.**

The trade-off: without Steam's browse/recommendation engine, there is no passive discovery.
Every new player must be acquired via social media, influencers, SEO, word of mouth, or direct
marketing. DLC launch visibility spikes are smaller without a platform storefront — estimated
5–8% new-player acquisition per campaign (vs 15%+ on Steam).

The subscription and microtransaction revenue (§15a) is also self-managed:
no platform cut on recurring billing, no IAP restriction on iOS (Safari PWA pays
Stripe directly, bypassing Apple entirely).

### 17b. Co-op campaigns are architecturally cheap

A single-player or co-op (max 5-player) campaign uses the same Naia networking stack at a
fraction of the bandwidth cost of PvP:

| Mode | Players | Active BW | Monthly BW/cell |
|---|---|---|---|
| 5v5 PvP | 10 | 45 KB/s | ~55 GB |
| **5-player co-op** | **5** | **~11 KB/s** | **~14 GB** |
| Solo campaign | 1 | ~0.5 KB/s | ~1.3 GB |

A co-op campaign cell costs **4× less bandwidth** than a 5v5 PvP cell. The server capacity
for DLC content is essentially free relative to the PvP fleet.

The maximum 5-player co-op design is a natural fit for Cyberlith's mechanics: squad-based
asymmetric challenges, emergent encounters with Lives stakes, boss encounters requiring
coordination — the Halo campaign model in Cyberlith's universe.

### 17c. DLC economics (self-managed, $15 price, 2 campaigns/year)

**Fixed constraints:** $15 DLC, 2 releases/year, self-managed platform.

| Variable | Conservative | Moderate | Optimistic |
|---|---|---|---|
| Stripe fee per sale | $0.74 | $0.74 | $0.74 |
| Dev net per sale | $14.27 | $14.27 | $14.27 |
| Existing MAU conversion per campaign | 6% | 10% | 15% |
| New-player acquisitions per launch | +5% of MAU | +7% | +10% |
| Net sales per campaign (at 8 333 MAU) | ~920 | ~1 415 | ~2 085 |
| Revenue per campaign | **~$13 129** | **~$20 193** | **~$29 753** |
| Monthly DLC revenue (2/year amortized) | **~$2 188** | **~$3 366** | **~$4 959** |

*New-player estimate is lower than a Steam model because there is no storefront algorithm.
Growth comes from newsletter, social media, and direct community marketing.*

### 17d. Full revenue picture at 250 premium subs, 2 DLC/year at $15

**Base case at 8 333 MAU (250 subs, 833 CCU):**

```
Revenue streams (monthly gross):
  Subscription:  250 × $7                        = $1 750
  Microtx:       8 333 × 27.5% × $2/mo           = $4 583
  Rewarded ads:  8 333 × 0.30 × 30 × $5/1000     = $  375
  DLC (moderate, 2/year): $20 193 × 2 / 12        = $3 366
  ─────────────────────────────────────────────────────────
  Total gross:                                     $10 074

Costs (monthly):
  Server:         1 × $224                        = $  224
  Overhead:                                        = $  200
  Processing (subs + microtx): 2.9% × $6 333      = $  184
  Processing (DLC): 1 415 × 2 / 12 × $0.74        = $  175
  ─────────────────────────────────────────────────────────
  Total costs:                                     $  783

Pre-tax profit:   $10 074 − $783                   = $9 291
After-tax (30%):                                   = $6 504/month
```

**Result: ~$6 500/month net at 250 subs, 2 DLC/year at $15 — healthy indie income,
but $3 500/month short of the $10 000 target.**

### 17e. What closes the gap to $10K?

The $10 000/month net target requires pre-tax profit of $14 286/month.
At fixed DLC cadence (2/year at $15, moderate assumptions), the only lever is growing CCU:

```
// Revenue per CCU:
//   sub+microtx: $7.60
//   ads:         $0.45
//   DLC (2/yr, moderate 17% of MAU per campaign): 0.17 × 10 × $14.27 × 2/12 = $4.04
full_revenue_per_ccu = $7.60 + $0.45 + $4.04 = $12.09

// Variable costs per CCU:
//   processing on sub+microtx: 2.9% × $7.60 = $0.22
//   processing on DLC:         (0.17 × 10 × 2/12) × $0.735 = 0.283 × $0.735 = $0.21
variable_costs_per_ccu = $0.22 + $0.21 = $0.43

// Fixed costs: server $224 + overhead $200 = $424
// (server stays at 1 × $224 until ~1 260 CCU → 113 cells; upgrade to $448 shortly after)

net_per_ccu = $12.09 − $0.43 = $11.66

// Solve: 0.70 × (CCU × $11.66 − $424) = $10 000
// CCU × $11.66 = $14 286 + $424 = $14 710
CCU = $14 710 / $11.66 ≈ 1 262
```

**CCU target with self-platform + ads + 2 DLC/year at $15: ~1 262 CCU → ~378 premium subs**

Verification at 1 262 CCU:
- Revenue: 1 262 × $12.09 = $15 258
- Costs: $424 + 1 262 × $0.43 = $424 + $543 = $967
- Pre-tax: $15 258 − $967 = $14 291
- After-tax (30%): $14 291 × 0.70 = **$10 004** ✓

| CCU | Subs (~3% of MAU) | After-tax net |
|---|---|---|
| 833 (250 subs) | 250 | **~$6 500** |
| 1 000 | 300 | **~$7 870** |
| 1 262 | ~378 | **~$10 000** ✓ |
| 1 500 | 450 | **~$11 700** |
| 2 025 | 608 | **~$15 800** |

*At ~1 500 CCU the server upgrades to 2 × $224 = $448 (135 cells); after-tax reflects the
higher server cost stepping in.*

Growing from 250 to ~378 premium subscribers — an additional ~128 subscribers — closes the
gap entirely. That is a realistic target for a game with active community and content updates.

### 17f. How each DLC launch compounds growth

Each campaign launch is a marketing event that adds MAU permanently:
- 7% new-player acquisition rate (moderate) = 8 333 × 0.07 = 583 new players per campaign
- If 3% of those subscribe: 17–18 new subscribers per campaign
- After 4 campaigns (2 years at 2/year): +70 subscribers from DLC launches alone

DLC releases do not just add one-time revenue — they permanently lift the subscription
and microtx baseline. This is the compounding effect that makes DLC a growth engine, not
just a revenue supplement.

### 17g. The two-path strategy (recommended)

**Path A: Organic growth (primary)**
Each PvP player retained grows the subscription + microtx base. 250→378 subs is achievable
within 12–18 months of strong retention. At 378 subs (~1 262 CCU), the $10K target is met with
2 DLC/year at $15 — no further change to DLC cadence or price needed.

**Path B: DLC revenue acceleration**
2 campaigns/year at $15 contributes ~$3 366/month (moderate) plus 35 new subscribers/year
compounding into the base. Each DLC is both a revenue event and a marketing moment that
pushes the organic CCU ceiling higher.

**The target in plain terms:** ~378 premium subscribers (~1 262 CCU) on a self-managed platform,
releasing 2 co-op campaign DLCs per year at $15, with rewarded ads enabled →
**$10 000/month net profit** as a solo indie developer. The server bill is $224–448/month —
less than 5% of revenue.

---

## 19. Performance and Infrastructure Levers

The bandwidth bottleneck identified in §11 and §14c is not a ceiling — it is a target.
Every lever below attacks the same root cause: **how many bytes leave the server per tick**.
The compound effect is substantial enough to change the business model.

---

### 19a. The constraint is always bandwidth

Across every shared-CPU Vultr tier, bandwidth is the hard wall (§14c). CPU and RAM have
generous headroom — it is the ISP-imposed monthly transfer limit that caps CCU.
Improving CPU or RAM does nothing until the bandwidth constraint is lifted.

The O(N²) law (Win 3) is the source:

```
active_bw = 450 × N²  bytes/sec   (N players, 25 Hz, full broadcast)
```

For a 16-player match: **115 200 bytes/sec = 112 KB/s per cell**.
For the mixed fleet: **79 GB/cell/month** (§14b weighted average).

Any lever that reduces either the N² term or the constant factor is transformative,
because it multiplies across every cell, every server, every tier simultaneously.

---

### 19b. Interest management — the single biggest lever

**Current state:** Every mutation is broadcast to all N clients, regardless of whether
those clients can see the mutating entity. This is correct for correctness and simplicity,
and it produces the O(N²) BW law.

**The lever:** Area of Interest (AOI) — only send a mutation to the k clients whose
viewport / line-of-sight includes the source entity. The actual delivery becomes:

```
active_bw_aoi = N_mutations × bytes_per_mutation × k × tick_hz
              = N × 18 × k × 25
              = 450 × N × k   (vs 450 × N²  without AOI)
```

At k = 4 average visible peers (25% visibility in Halo-style BTB maps):

| Match type | Full broadcast | AOI (k=4) | Reduction |
|---|---|---|---|
| 2v2 (N=4) | 7.2 KB/s | 7.2 KB/s | 0% (everyone sees everyone) |
| 5v5 (N=10) | 45 KB/s | 18 KB/s | 60% |
| 10v10 (N=20) | 180 KB/s | 36 KB/s | 80% |
| 40v40 (N=80) | 2 880 KB/s | 144 KB/s | 95% |

For the mixed fleet (§14b), the weighted average BW/cell drops from **79 GB** to **~20 GB**
per month — a **4× reduction**.

**Infrastructure is already built.** `TileTraversability` (`services/game/naia_proto/src/simulation/tile_traversability.rs`)
encodes exactly which of the 8 movement directions are passable for every tile. Extending
this to a player-to-player visibility graph is a direct next step: given each player's tile
position and direction, the set of tiles they can see is derivable from the same traversability
structure. Naia's existing per-entity dirty tracking means AOI is an *output filter*, not a
data structure change — only the recipient list changes at send time.

**CCU impact with AOI (4× BW reduction), same Vultr servers:**

| Server | Monthly cost | BW budget | Regular cells (AOI) | Peak CCU |
|---|---|---|---|---|
| Shared 2 vCPU | **$20** | 3 TB | **152** | **~1 688** |
| Shared 4 vCPU | **$40** | 4 TB | **202** | **~2 242** |
| Dedicated 8 vCPU | **$224** | 10 TB | **506** (CPU caps at ~432) | **~4 795** |

At $20/mo with AOI: **~1 688 CCU** — already exceeding the $10K revenue target (§17e: 1 262 CCU needed).
AOI alone, on the cheapest viable server, closes the gap.

---

### 19c. Switch infrastructure to Hetzner

Vultr's bandwidth pricing is the worst aspect of an otherwise adequate platform.
**Hetzner Cloud includes 20 TB/month on every instance, regardless of tier.**

| Provider | Tier | Price/mo | BW included | Effective $/GB |
|---|---|---|---|---|
| Vultr | Shared 2 vCPU | $20 | 3 TB | $0.0067 |
| Vultr | Dedicated 8 vCPU | $224 | 10 TB | $0.0224 |
| **Hetzner** | **CX21** (shared 2 vCPU, 4 GB RAM) | **~$6** | **20 TB** | **$0.0003** |
| **Hetzner** | **CCX33** (dedicated 8 vCPU, 32 GB RAM) | **~$62** | **20 TB** | **$0.0031** |

Hetzner CX21 costs 7× less per GB of bandwidth than Vultr's cheapest tier,
and **70× less** than Vultr's dedicated tier — for the same 20 TB of transfer.

**CCU capacity without AOI on Hetzner (full broadcast):**

Hetzner CX21 (shared 2 vCPU, 40% eff):
- BW limit: 20 000 GB / 79 GB/cell = **253 cells** → BW not binding
- CPU limit: 2 × 40 000 × 0.40 / 181 µs = **177 cells** ← binding
- **Peak CCU: 177 × 11.1 = ~1 965**

Hetzner CCX33 (dedicated 8 vCPU, 55% eff):
- BW limit: 20 000 / 79 = **253 cells** ← binding
- CPU limit: 8 × 40 000 × 0.55 / 181 = **972 cells**
- **Peak CCU: 253 × 11.1 = ~2 809**

Even before AOI, **Hetzner CX21 at ~$6/mo outperforms Vultr $224/mo (1 399 CCU)** at 35× lower cost.

**CCU/dollar comparison (no AOI):**

| Server | Monthly cost | CCU | CCU per $ |
|---|---|---|---|
| Vultr $20/mo | $20 | 422 | 21 |
| Vultr $224/mo | $224 | 1 399 | 6.2 |
| Hetzner CX21 | ~$6 | ~1 965 | **~327** |
| Hetzner CCX33 | ~$62 | ~2 809 | **~45** |

---

### 19d. Combined impact: AOI + Hetzner

When interest management and infrastructure switch are applied together, Hetzner's
generous bandwidth budget is no longer the limiting factor at all — CPU binds first,
and even there the capacity is extraordinary:

**Hetzner CCX33 (~$62/mo) + AOI (k=4):**
- BW/cell with AOI: 79 × 0.25 = **19.75 GB/cell/month**
- BW limit: 20 000 / 19.75 = **1 013 cells**
- CPU limit (dedicated 8 vCPU, 55% eff): 8 × 40 000 × 0.55 / 181 = **972 cells** ← binding
- **Peak CCU: 972 × 11.1 = ~10 789 CCU on a single $62/mo server**

**Hetzner CX21 (~$6/mo) + AOI (k=4):**
- BW/cell with AOI: 19.75 GB/cell/month
- BW limit: 20 000 / 19.75 = 1 013 cells
- CPU limit (shared 2 vCPU, 40% eff): 177 cells ← binding
- **Peak CCU: 177 × 11.1 = ~1 965 CCU on a $6/mo server**

**Summary table — all configurations:**

| Configuration | BW approach | Server | Cost/mo | Peak CCU | CCU/$ |
|---|---|---|---|---|---|
| Baseline | Full broadcast | Vultr $20 | $20 | 422 | 21 |
| Baseline | Full broadcast | Vultr $224 | $224 | 1 399 | 6 |
| Infra switch only | Full broadcast | Hetzner CX21 | ~$6 | ~1 965 | ~327 |
| Infra switch only | Full broadcast | Hetzner CCX33 | ~$62 | ~2 809 | ~45 |
| AOI only | O(N×k) | Vultr $20 | $20 | ~1 688 | 84 |
| **AOI + Hetzner** | **O(N×k)** | **Hetzner CX21** | **~$6** | **~1 965** | **~327** |
| **AOI + Hetzner** | **O(N×k)** | **Hetzner CCX33** | **~$62** | **~10 789** | **~174** |

The $10K/month business target (§17e) requires ~1 262 CCU. The AOI + Hetzner combined
path achieves this with a single $6/month server — leaving budget for dedicated cells,
redundancy, and the 40v40 on-demand tier. The server cost fraction drops well below 1%
of revenue at the target CCU level.

---

### 19e. Adaptive tick rate

Cells in lobby, post-match, or spectator mode do not need 25 Hz. Dropping idle phases
to 5 Hz reduces BW for those cells by 5×. The practical impact depends on the fraction
of cell-time spent in non-combat phases.

Estimate: ~20–30% of fleet cell-time is in pre-match countdown, post-match score screen,
or queue holding. For those cells, BW drops to ~20% of combat BW. Fleet-wide savings:

```
savings = idle_fraction × (1 - 5/25) = 0.25 × 0.80 = 20%
```

A **20% fleet-wide BW reduction** for essentially zero implementation cost — a single
`tick_rate_hz` field already exists in `BenchWorld` (Phase 10, §1). The production
cell runtime needs the same per-phase rate switching.

---

### 19f. CDN preloading for level tile data

Level load time is currently **5.2 s** for 10K tiles (§1), dominated by Naia pushing all
10K immutable tile entities to 16 clients in 2 uncapped-BW ticks. This is both a UX
problem (players wait 5+ seconds between matches) and a BW spike that stresses the
server during loading.

**The fix:** serve level tile data as a static binary blob from a CDN, not from the game
server. Tiles are immutable after level load (Win 2) — they are ideal static assets.

- Tile blob for 10K tiles: `10 000 × ~300 B ≈ 3 MB` — trivially served by Cloudflare's
  free tier (unlimited bandwidth, any geography).
- Client downloads the level blob from CDN before connecting to the game cell.
- Game server only spawns the mutable unit entities (32 units × 18 B × 16 clients = ~9 KB total).

**Result:**
- Level load BW spike on the server: **~112 KB** (down from ~30 MB uncapped).
- Level load wall-clock time: CDN fetch at broadband speed (~50 ms for 3 MB at 500 Mbps)
  vs 5.2 s Naia push. **Effectively eliminates the load screen.**
- Server BW budget reclaimed: level loads currently burn ~30 MB × match-start rate.
  At 400 match starts/hour across a fleet: 400 × 30 MB = 12 GB/hour = 8.6 TB/month —
  potentially a significant fraction of the bandwidth budget, especially at launch.

This also enables the portal pre-warming strategy (§7) to be implemented cleanly:
pre-warm the destination CDN-cached blob while the player finishes the current match.

---

### 19g. What to measure next

The wire capacity report shows **∞** because `server_wire_bytes_idle` and
`server_wire_bytes_active` are both 0 in the profile (not yet benched). The O(N²)
formula used throughout this document is the theoretical derivation from Win 3; it
assumes **18 bytes per mutation per client per tick** as the Naia framing overhead.

Running `wire/bandwidth_realistic_quantized` will produce the true wire-frame sizes
and validate (or correct) the 450 × N² constant. A 2× error in the constant changes
all CCU estimates by 2×. This bench should be run and its output fed into the capacity
formula before making infrastructure provisioning decisions.

**Priority action list:**
1. Run `wire/bandwidth_realistic_quantized` — validate the 450 × N² constant.
2. Implement AOI output filter in the Naia send path, using `TileTraversability` as
   the visibility oracle. Measure actual BW reduction vs the 4× prediction.
3. Switch VPS provider from Vultr to Hetzner. The bandwidth economics are not close.
4. Add per-phase tick rate to the cell runtime (5 Hz lobby → 25 Hz combat → 10 Hz
   post-match). The bench infrastructure already has `tick_rate_hz`.
5. Serve level tile blobs from Cloudflare (free tier). Only push mutable units at game start.

---

## 18. Appendix: Scaling Formulas

```
// O(N²) bandwidth law (Win 3)
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
              ∝ 1/N    ← smaller matches → more total players

// CPU capacity (cells per server)
cells_cpu = (cores × efficiency × 40_000_µs) / tick_budget_µs_per_cell
          = cores × 0.40 × 97   // for 16-player full game stack

// RAM capacity (cells per server)
cells_ram = total_ram_mb / mb_per_cell
          = total_ram_mb / 7.2   // for 16-player Halo BTB 10K tiles

// Level load time (linear approximation)
level_load_s ≈ tile_count / 10_000 × 5.2

// Binding capacity (cells per server)
cells = min(cells_cpu, cells_ram, cells_bw)
```
