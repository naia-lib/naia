# Cyberlith Business & Technical Plan — 2026-04-26

Web-first indie action tactics platform: business realism, scaling architecture, and validation gates

---

**Owner:** Connor
**Date:** 2026-04-26
**Status:** Internal planning document
**Confidence:** Mixed; technical foundation promising, business assumptions unproven
**Primary use:** Guide near-term product, technical, and business decisions
**Current strategic phase:** Prove fun + retention before monetization

---

## Table of Contents

- [§1 Executive Summary](#1-executive-summary)
- [§2 Product Strategy](#2-product-strategy)
- [§3 Game Design Pillars](#3-game-design-pillars)
- [§4 Business Model and Survival Math](#4-business-model-and-survival-math)
- [§5 Technical Architecture Summary](#5-technical-architecture-summary)
- [§6 Capacity Analysis](#6-capacity-analysis)
- [§7 Infrastructure and Cost Model](#7-infrastructure-and-cost-model)
- [§8 Account, Platform, and Entitlement Strategy](#8-account-platform-and-entitlement-strategy)
- [§9 Playtest and Market Validation Strategy](#9-playtest-and-market-validation-strategy)
- [§10 Risk Register](#10-risk-register)
- [§11 Near-Term Execution Plan](#11-near-term-execution-plan)
- [§12 Decision Gates](#12-decision-gates)
- [§13 Current Strategic Verdict](#13-current-strategic-verdict)
- [§14 Appendix A — Original Benchmark Tables](#14-appendix-a--original-benchmark-tables)
- [§15 Appendix B — Original Revenue Scenario Tables](#15-appendix-b--original-revenue-scenario-tables)
- [§16 Appendix C — Open Questions](#16-appendix-c--open-questions)

---

## §1 Executive Summary

### Core claims

- Cyberlith can plausibly become a sustainable indie business, but only through earned validation.
- Server cost is likely not the main blocker.
- Full-stack performance, browser client performance, retention, acquisition, and monetization remain the main risks.
- Competitive PvP is the fastest mechanics-testing path, not necessarily the final primary game mode.
- Web/PWA-first distribution is the strongest strategic wedge.
- Steam should come later, after web traction and demo proof.
- Monetization should come later, after retention proof.
- 250 CCU is a meaningful proof-of-life target.
- Founder break-even likely requires ~700–1,000 sustained CCU or equivalent premium/DLC revenue.
- Near-term objective: make strangers replay the first room/match voluntarily.

### What is proven `[Measured, Naia-only]`

- **Naia networking stack is fast.** The `halo_btb_16v16_10k` scenario (16 players, 10K immutable tile entities, 32 mutable unit entities, 25 Hz) shows: idle tick **63 µs**, active tick **58 µs**, level load **5.2–5.5 s**, client receive **889 ns**. These are `[Measured, Naia-only]` on a single core of the lifetop dev machine (Intel Core i9-12900HK, 62 GiB RAM).
- **O(1) idle cost proven.** 10,000 immutable tile entities cost nothing at steady state. Map size does not affect per-tick server CPU.
- **O(mutations × users) scaling confirmed.** Active tick cost scales with the product of dirty count and connected client count, as expected.
- **Client-side networking is essentially free.** 889 ns/tick client receive = 0.002% of one CPU core.

### What is NOT proven

- **Rapier physics tick cost is unquantified.** The ~350 µs physics estimate and the ~558 µs total full-stack tick are `[Estimated, no benchmark]`. No full-stack Cyberlith game server has been benchmarked under real gameplay load.
- **Wire bytes are not measured.** The capacity report shows `∞` for wire capacity — wire benches exist but have not been run. All bandwidth figures are derived from a worst-case O(N²) formula, NOT from measured bytes-on-wire.
- **No multi-player full-stack benchmark exists.** All CCU capacity figures for the full game stack are extrapolations. Error bars: ±30–50%.
- **Browser client rendering is unmeasured.** Client-side Naia networking is free. Total client performance on target hardware (web/Wasm, mid-range laptop, mobile browser) is unknown.

### What blocks business confidence

1. **Full-stack tick cost is unknown.** The ~558 µs/cell full-game estimate could be 2× wrong in either direction.
2. **Bandwidth is not instrumented.** Production BW is likely far below the O(N²) worst-case estimate, but this has not been measured.
3. **Retention and fun are unproven.** No external players have played a real match.

### Immediate next gate

**Full-stack 2v2 benchmark** — run Cyberlith game server with 4 players, real Rapier physics, real game logic, and measure: tick wall time (P50/P95/P99), server outgoing bytes per tick, memory consumption. This single measurement resolves the largest technical unknown.

The business gate that matters even more: **make strangers replay the game voluntarily**.

---

## §2 Product Strategy

### §2.1 What Cyberlith Is

Cyberlith is a browser-first/PWA-first fixed-camera action tactics game with a single avatar shell, two hand-item channels, and daemon cores as equipment. It is being built PvP-first as a mechanics testbed, with future co-op, PvE, social-world, and adventure-room potential.

Key characteristics:
- Web/PWA as primary distribution. No download required. Direct account system from day one.
- Fixed orthographic camera. Simple visual language. No rotation.
- Keyboard-only viable desktop combat. Single-thumb mobile possible as a goal.
- Daemons are equipment, not a separate RTS hotkey layer.
- Match-instanced sessions. Small matches first. Portal/instancing model preserves path to larger platform later.
- Single account identity across web and Steam (eventually).

### §2.2 What Cyberlith Is Not Yet

- Not a mature live-service product.
- Not Steam-first. Steam is a later amplification step.
- Not monetization-first. Monetization comes after retention is proven.
- Not an MMO at MVP. Instanced sessions are the architecture; global MMO is not the launch shape.
- Not proven. The game has not been played by external people. Nothing is proven until strangers replay it voluntarily.

### §2.3 Distribution Strategy

- **www.cyberlith.com** is the primary platform. Direct accounts, direct payments.
- Free gameplay available from the start (no paywall before first play if possible).
- PWA install path supported for repeat visitors.
- Shareable match links / invite links as organic growth mechanism.
- Steam comes later — after web traction and a legible demo. Steam should amplify a working product, not rescue an unproven one.
- Monetization (subscriptions, cosmetics, DLC) comes after retention proof.
- `[Policy risk, requires verification]` Steam payment policy must be verified before any paid items launch on Steam. Steam-channel users should be assumed to require Steam Wallet / Steam DLC flows until confirmed otherwise.

### §2.4 PvP as Mechanics Wind Tunnel

PvP stress-tests everything at low player count: movement feel, latency, weapon/item balance, daemon behavior, map readability, prediction/interpolation, exploits, skill ceiling, rematch desire. You can run a meaningful PvP test with 4 players. You cannot test co-op campaign quality without campaign content.

Long-term product shape may become: co-op PvE rooms, adventure/scenario packs, social hub, campaign-like content, PvP as one mode among several. PvP-first is a test methodology, not necessarily the final product identity.

---

## §3 Game Design Pillars

### §3.1 Camera and Frame

- Fixed orthographic camera. No rotation.
- Approximate perspective: ~60° from side / ~30° from top.
- 1m³ cube tiles.
- ~15×11 visible tiles at standard zoom.
- Square gameplay frame inside 16:9 landscape display.
- Side UI panels for status, items, daemon state.

### §3.2 Input Doctrine

- No combat mouse. No right thumbstick for combat.
- Keyboard-only desktop combat is the primary desktop path.
- Left stick/D-pad + face buttons for controller.
- SNES-controller-compatible control surface is the goal.
- Single-thumb sequential play possible (mobile targeting long-term).
- Controls must be understandable within a few minutes — this is a Gate 1 requirement.

### §3.3 Two-Hand Item Model

L and R are left-hand and right-hand item channels. Weapons, shields, tools, daemon cores, Control Antenna, and Control Relay are all equipment items. Equipping or swapping an item is a player action, not a system mode change.

Daemon power is equipment power. There is no separate RTS hotkey layer.

### §3.4 Daemon Item Model — Exact Definitions

**Daemon Core:** Owns one daemon instance/type. Equipping it summons the linked daemon; unequipping desummons it. Gives type-specific commands when equipped. One Daemon Core = one daemon.

**Control Antenna:** Commands all currently active daemons. Does not summon or desummon any daemon. Equipment slot; no special mode required.

**Control Relay:** Deployable spatial anchor. Does not summon daemons. Does not become base-building. Provides a persistent position reference that daemons and abilities can use.

These definitions are not subject to reinterpretation. If the design changes, update this document explicitly.

### §3.5 Design Implication for Benchmarks

A benchmark that measures shell-only entities (no daemons, no projectiles, no item state) is insufficient for capacity planning. A production benchmark must include:

- Shell entities per player
- Active daemon entities per player (worst-case: all players with a Daemon Core equipped and daemon summoned)
- Projectile/effect entities in flight
- Daemon behavior updates (AI tick cost)
- Item state and heat/overheat state per entity
- Control Relay anchor entities
- Soft-targeting calculations
- Respawn/spawn events

BM-001 (shell-only 2v2) is a starting point. BM-002 (daemon worst-case) is required before daemon mechanics can be finalized.

---

## §4 Business Model and Survival Math

### §4.1 Personal Break-Even Target

Connor's target: **$5,000/month take-home after taxes**. This is a founder survival threshold, not an initial success metric. It is only meaningful after ~6 months of proven stability — not at first revenue, not at first playtest.

This target anchors the CCU estimates in §4.4. Work backward from $5K/month take-home to understand what player scale is required.

### §4.2 Revenue Model Variables

| Parameter | Value | Label |
|---|---|---|
| CCU → MAU multiplier | ×10 | `[Assumption]` Industry rule of thumb |
| Premium conversion (MAU) | 3% | `[Assumption]` Conservative |
| Premium subscription price | $7/mo | `[Assumption]` |
| Microtransaction buyer rate | 27.5% of MAU | `[Assumption]` Low-friction cosmetics model |
| Avg Crystal spend per buyer/month | $2.00 | `[Assumption]` |
| Monthly revenue per CCU (base) | **$7.60** | `[Assumption]` = $2.10 sub + $5.50 microtx |
| Fixed costs/month | ~$424 | `[Estimated]` server + overhead + processing |
| Variable costs per CCU | ~$0.43 | `[Estimated]` |
| After-tax assumed | 70% of pre-tax profit | `[Assumption]` 30% effective tax rate |

```
monthly_revenue ≈ CCU × 10 × (0.03 × $7 + 0.275 × $2.00)
                = CCU × ($2.10 + $5.50)
                = CCU × $7.60   [Assumption — Web/PWA direct channel only]

// Sensitivity range:
//   conservative (15% buyers, ~$1/mo):  CCU × $3.60
//   base case    (27.5% buyers, $2/mo): CCU × $7.60   (used throughout)
//   optimistic   (35% buyers, $3/mo):   CCU × $12.60
```

**Channel note:** All figures above assume Web/PWA direct channel (Stripe fees only, ~2.9% + $0.30). Steam-channel revenue carries ~30% platform fee on Steam Wallet/DLC purchases. Blended margin depends on Steam/Web player mix, which is unknown until production. Use Web/PWA figures as upper bound. `[Policy risk, requires verification]`

### §4.3 Richer Model With DLC

Adding 2 DLC campaigns/year at $15 (moderate case, Web/PWA direct):

```
dlc_revenue_per_ccu = $4.04   [Estimated — Web/PWA channel, moderate DLC conversion]
full_revenue_per_ccu = $7.60 + $4.04 = $11.64
variable_costs_per_ccu = $0.43
net_per_ccu = $11.21

// Solve for $5,000/month take-home (after 30% tax):
// 0.70 × (CCU × $11.21 − $424) = $5,000
// CCU = ($7,143 + $424) / $11.21 ≈ 674
```

### §4.4 Break-Even CCU Estimates

All values `[Assumption]` or `[Estimated]` — business assumptions unproven.

| Model | Required sustained CCU for ~$5K/month take-home |
|---|---:|
| Weak/conservative monetization | ~2,000+ |
| Base subs + cosmetics, no DLC | ~950–1,050 |
| Base subs + cosmetics + moderate DLC | ~675 |
| Strong monetization / strong DLC / strong cosmetics | ~500–600 |

**Critical distinctions:**

- **250 CCU is NOT the quit-job threshold.** At $7.60/CCU, 250 CCU produces ~$1,900/month gross. After costs and taxes, approximately $1,000–$2,000/month take-home. This is not founder salary.
- **250 peak CCU vs 250 sustained CCU are very different.** Peak CCU during a scheduled playtest event does not mean 250 people are playing every day. 250 sustained CCU (average across all hours of the day) is a serious indie live-game signal.
- **700–1,000 sustained CCU is closer to founder break-even** on the base sub + cosmetics + moderate DLC model.
- Do not plan for founder salary until at least 6 months of sustained CCU data exists.

### §4.5 Alternative Revenue Framing

Subscription may not be the right model until persistent value is proven. Small loyal audiences can monetize via:

- **Premium/early-access purchase** — reduces CCU dependence; one-time revenue per player
- **Founder/supporter packs** — early adopters who want to see the game succeed
- **Cosmetics** — identity expression; low friction if the game has an audience
- **DLC/scenario packs** — content drops with replayability; Steam-compatible
- **Steam later** — amplification after the web product is proven

Do not force subscription design before there is a reason for players to subscribe month after month. Design the core loop first. Monetization layer follows.

---

## §5 Technical Architecture Summary

### §5.1 High-Level Architecture

- **Web/PWA client:** Rust/Wasm in browser, served from www.cyberlith.com. No download required.
- **Direct Cyberlith account backend:** Identity, progression, loadouts, entitlements, session management.
- **Game session/cell servers:** Authoritative server model. Each cell is one game_server thread managing one level instance.
- **Naia networking:** Measured Naia replication stack. Authoritative server sends entity state to all clients.
- **Rapier physics:** Server-authoritative. Physics cost not yet measured.
- **Scalable cell/session model:** Sessions are additive. No cross-cell synchronization required at MVP. Portal pre-warming handles transitions.
- **Steam client:** Later frontend addition. Not in current scope.

### §5.2 Scaling Philosophy

- Small match sizes first (2v2 launches). Unlock larger modes as CCU grows and benchmarks pass.
- No monolithic MMO at MVP. Instanced cells are the architecture.
- PvP rooms stress-test mechanics and networking at low player counts.
- Preserve path to larger platform (co-op campaigns, larger events, social hub) without building it prematurely.

### §5.3 Technical Foundation Status

| Area | Status | Confidence |
|---|---|---|
| Naia replication microbenchmark | Measured/promising | Medium-high |
| Full-stack game tick | Estimated/unproven | Low-medium |
| Rapier/physics load | Major unknown | Low |
| Wire bytes/WAN | Unmeasured | Low |
| Browser render performance | Unmeasured/critical | Low |
| Account/platform backend | Planned/in progress | TBD |
| Steam integration | Later | Not current |

---

## §6 Capacity Analysis

### §6.1 Measured Naia Results

All measurements from `halo_btb_16v16_10k` scenario: 16 players, 10,000 immutable HaloTile entities, 32 mutable HaloUnit entities, 25 Hz.
Machine: lifetop (Intel Core i9-12900HK, 62 GiB RAM, Linux 6.8.0-110-generic).
Command: `cargo criterion -p naia-benches -- "scenarios/halo_btb_16v16"`.
Date: 2026-04-26.

| Measurement | Value | Label |
|---|---|---|
| Level load (10K tiles + 32 units → 16 clients) | **5.2 s** (σ ≈ 0.8 s; 5,511.6 ms from capacity report) | `[Measured, Naia-only]` M-001 |
| Server tick — idle (0 mutations) | **63 µs** | `[Measured, Naia-only]` M-002 |
| Server tick — active (32 mutations) | **58 µs** | `[Measured, Naia-only]` M-003 |
| Client receive — active tick | **889 ns** | `[Measured, Naia-only]` M-004 |

> Note: Active tick (58 µs) is slightly faster than idle (63 µs) — within noise at this timescale. Both reflect the Naia networking path only, with zero Rapier or game logic cost.

**Capacity report output (2026-04-26):**

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

The 649/760 figures are **Naia-networking-only** on one core. Full game stack (Rapier + logic) reduces this significantly. Do not use these numbers directly for production capacity planning.

**Architectural guarantees confirmed `[Measured, Naia-only]`:**

- **O(1) idle cost (Win 2):** 10,000 HaloTile entities (immutable) cost nothing at steady state. No dirty-tracking, no per-entity scan. Server CPU is entirely determined by mutable unit mutations and client count, never by map size.
- **O(mutations × users) scaling (Win 3):** Active tick cost scales with the product of mutations and connected clients, not total entity count.

### §6.2 What Is Not Yet Measured

None of these have been benchmarked. All downstream estimates carry ±30–50% error bars.

- Full game loop with Rapier physics under real gameplay load
- Real gameplay logic (damage, spawn management, game state)
- Daemon behavior (AI tick, path calculation, command response)
- Item heat/overheat state per entity
- Projectile and effect entity churn
- Actual outgoing wire bytes per tick
- WAN latency and jitter with real clients
- Browser client rendering (FPS, frame time, Wasm heap, GC pressure)
- Mobile/PWA thermals on mid-range device
- Persistence/account overhead
- Multi-cell concurrent load on same server
- Join burst (many clients joining simultaneously)
- Portal pre-warming time in full-stack game

### §6.3 Correct Capacity Interpretation

The Naia evidence suggests networking is likely not the first bottleneck in a full game stack. It does not prove full game capacity. Shell-only benchmarks are a starting point, not a production estimate.

Daemons are core to the design (see §3.4). Benchmarks without daemon entities are insufficient. Capacity planning must include a worst-case daemon load scenario — one active daemon per player, all daemons moving and issuing commands simultaneously.

The current estimated full-stack tick budget:

| Component | Cost (µs) | Label |
|---|---|---|
| Naia networking — active, 32 mutations, 16 clients | **58** | `[Measured, Naia-only]` M-003 |
| Rapier physics — 32 kinematic bodies + 10K static tiles | **~350** | `[Estimated, no benchmark]` |
| Game logic — damage, spawn management, game state | **~100** | `[Estimated]` |
| OS overhead, context switches | **~50** | `[Estimated]` |
| **Total per-cell tick (16-player, shell only)** | **~558 µs** | `[Estimated, no benchmark]` |
| **Cells per core — theoretical** | **~72** | 40,000 / 558 |
| **Cells per core — realistic (40% efficiency)** | **~29** | Cache + scheduling overhead |

This estimate inherits the full physics uncertainty. Physics dominates at ~350 µs `[Estimated, no benchmark]`. Rapier tile BVH optimization (replacing per-tile colliders with one static trimesh) is expected to reduce physics cost 2–4×, but has not been benchmarked.

### §6.4 Required Benchmark Ladder

Each benchmark must measure: P50/P95/P99 server tick wall time, outgoing bytes/sec per client, incoming bytes/sec per server, memory/session, CPU/session, browser FPS, input latency, reconnect rate, crash rate, match completion rate.

| ID | Benchmark | Unblocks | Priority |
|---|---|---|---|
| BM-001 | Full-stack 2v2 — 4 players, real Rapier, real game logic; P95 tick ≤ 40 ms; wire bytes measured | 2v2 as production default; all downstream CCU estimates | **Critical** |
| BM-002 | Full-stack 2v2 + daemon worst-case — all players with one daemon summoned, all daemons active | Daemon mechanics finalized; capacity with daemons | **Critical before daemon design lock** |
| BM-003 | Full-stack 4v4 / small event room stress | 4v4 mode unlock; larger event planning | High |
| BM-004 | Browser client render/FPS/thermal benchmark on mid-range laptop and mobile browser | Web distribution confidence | High |
| BM-005 | Wire bytes per tick — run `wire/bandwidth_realistic_quantized` bench | Bandwidth model validation; replace O(N²) formula | High |
| BM-006 | Account/session/persistence soak — login, join, disconnect, reconnect under load | Account backend confidence | Medium |
| BM-007 | Scheduled public playtest load test — real players, real network, real queuing | Pre-web-alpha confidence | Medium |

Do not model any match size as a production default until its benchmark gate is passed.

---

## §7 Infrastructure and Cost Model

### §7.1 Cost Drivers

- **CPU** — primary constraint once bandwidth is resolved with Hetzner's inclusive bandwidth
- **Bandwidth** — primary constraint on Vultr shared plans; not a constraint on Hetzner with event-driven BW model
- **Memory** — ~7.2 MB/cell for 16-player Halo BTB scenario `[Estimated]`; RAM rarely the binding constraint at small match sizes
- **Database/account services** — uncosted; will matter at scale
- **Logs/telemetry** — small at early stage; grows with player volume
- **DDoS/abuse protection** — Cloudflare for HTML/JS/Wasm static assets; Naia data goes direct per infra doctrine
- **CDN/static hosting** — Cloudflare for web client assets
- **Observability** — lightweight self-hosted or low-cost managed at early stage

### §7.2 Provider Notes

**Provider pricing as of 2026-04. Re-verify before any production purchasing decision — cloud pricing changes frequently.** `[External pricing, manually verified 2026-04]`

| Provider | Tier | Price/mo | BW included | CPU type | Estimated cells/server (full-stack) | Estimated peak CCU |
|---|---|---|---|---|---|---|
| Vultr | Shared 2 vCPU | $20 | 3 TB | shared | ~29 | ~322 |
| Vultr | Dedicated 8 vCPU | $224 | 10 TB | dedicated | ~236 | ~2,620 |
| **Hetzner** | **CX21** (shared 2 vCPU) | **~$6** | **20 TB** | shared | ~58 | **~644** |
| **Hetzner** | **CCX33** (dedicated 8 vCPU) | **~$62** | **20 TB** | dedicated | ~472 | **~5,239** |

All capacity figures `[Estimated, no benchmark]` — inherit full-stack tick uncertainty from §6.3.

**Key finding:** Hetzner CX21 at ~$6/mo delivers more estimated CCU than Vultr $20/mo. Hetzner CCX33 at ~$62/mo covers the entire base-case CCU target (~675–1,000 CCU) on one server with headroom. Hetzner 20 TB bandwidth inclusion makes bandwidth a non-constraint with the event-driven model.

**Shared vCPU warning:** Shared vCPU plans are subject to noisy-neighbor CPU jitter. For real-time game servers at 25 Hz, P99 tick latency spikes may violate the 40 ms budget. Use dedicated vCPU (Hetzner CCX, Vultr Optimized) for production.

**Infra doctrine:** Hetzner is the preferred production provider. Cloudflare only for HTML/JS/Wasm static assets — never for Naia game data.

Early server cost estimate for founder break-even range (675–1,000 CCU):
- 1× Hetzner CCX33 at ~$62/mo `[External pricing, manually verified 2026-04]`
- Server cost ~1% of revenue at base-case monetization — not the dominant business risk

### §7.3 Scaling Cost Conclusion

Early server costs are probably manageable. Acquisition, retention, and monetization conversion are the dominant business risks. Bandwidth may become material at large scale but is unlikely to bind before CCU hits thousands on Hetzner. Infrastructure should scale with validated demand — never over-provision ahead of proven player counts.

---

## §8 Account, Platform, and Entitlement Strategy

### §8.1 Direct Web Accounts

MVP includes a direct Cyberlith account system. Accounts support:
- Player identity (display name, account history)
- Progression and loadout persistence
- Playtest tracking and retention measurement
- Telemetry and analytics tie-in
- Future entitlements (cosmetics, DLC, subscriptions)

Minimize account friction before first play if possible. A guest-or-named-login-required decision is an open question (see §16 Appendix C). SteamID can substitute for email/password when players arrive via Steam.

### §8.2 Free Gameplay

Some free gameplay must be available immediately. Free web access is the playtest/acquisition wedge. Free tier supports shareable links, scheduled events, and community growth. Do not paywall the first match.

### §8.3 Steam Later

Steam comes after web traction. Steam should amplify an already-proven demo/product, not rescue an unproven one.

Requirements before Steam page goes live:
- Real Linux-native / Steam Deck-quality playable app (not a redirect-only page)
- Proton compatibility validated for Windows-via-Steam users
- Steam Wallet/DLC adapter built before any paid items list on Steam
- Steam payment policy verified `[Policy risk, requires verification]`

Steam entitlements must eventually reconcile with web account entitlements via the canonical backend entitlement system. No platform-specific purchase logic enters game code.

### §8.4 Monetization Later

Build entitlement architecture early enough not to paint into a corner — but do not design the core gameplay loop around monetization before retention is proven.

Planned future monetization channels (not yet active):
- Premium access / early-access purchase
- Founder/supporter pack
- Cosmetics (Crystal currency, direct web purchase)
- DLC/scenario packs
- Subscription (only if persistent value is proven)

The one-backend / multiple-purchase-provider pattern is the target architecture. Payment providers are adapters, not game systems. No provider-specific logic enters the game layer. All purchases verified server-side before entitlement is granted.

---

## §9 Playtest and Market Validation Strategy

### §9.1 Metrics Before Money

Track these before modeling any revenue. These are the signals that tell you whether the product works.

| Metric | Why it matters |
|---|---|
| Account creation conversion | Are link-click players becoming accounts? |
| First match completion rate | Can new players get through the tutorial/first match? |
| Rematch rate | Did people want to play again immediately? |
| D1/D7 return rate | Are people coming back on their own? |
| Sessions per user per week | Is this becoming a habit? |
| Party/invite rate | Are players bringing other players? |
| Link-share rate | Are players sharing match links externally? |
| Discord/community conversion | Are players joining the community? |
| Build/loadout experimentation rate | Are players engaging with the item system? |
| Rage quit / confusion exit points | Where are players dropping out? What do they not understand? |
| Performance by device | Are mid-range laptops and phones keeping up? |

### §9.2 Playtest Stages

| Stage | Target | Success Signal |
|---|---:|---|
| Local/private | 2–4 players | Core mechanics understandable |
| Trusted alpha | 5–20 testers | Repeated play without begging |
| Scheduled web test | 16–32 peak CCU | Social loop forming |
| Public web alpha | 50 peak CCU | Real signal |
| Public demo/event | 100 peak CCU | Strong signal |
| Pre-Steam event | 250 peak CCU | Serious traction |
| Post-Steam / EA | 250 sustained CCU | Real live product |
| Founder break-even | 700–1,000 sustained CCU or equivalent revenue | Possible salary discussion |

### §9.3 What 250 CCU Means

- **250 peak CCU** = strong playtest/event traction. People showed up for a scheduled event. Does not prove they will return on a random Tuesday.
- **250 sustained CCU** = serious indie live-game signal. People are playing across all hours of the day, every day. This is a meaningful milestone.
- Neither automatically proves founder salary. At $7.60/CCU base model, 250 sustained CCU produces approximately $1,900/month gross, roughly $1,000–$2,000/month take-home. That is well below the $5,000/month founder target.
- **700–1,000 sustained CCU** is the realistic range for founder break-even on a base subs + cosmetics + moderate DLC model.

---

## §10 Risk Register

Blunt assessment. Not softened.

| Risk | Severity | Likelihood | Current Evidence | Mitigation | Next Gate |
|---|---|---|---|---|---|
| Game is not fun enough | **Critical** | Unknown — not yet tested externally | No external players have played; no rematch data | Build first playable loop; run trusted alpha immediately | Gate 1: rematch rate |
| Controls too weird/confusing | **High** | Medium — unusual control scheme | No external tester feedback; controls are non-standard | First tester sessions; observe without coaching | Gate 1: understandable within a few minutes |
| Browser client performance weak | **High** | Unknown — untested | Client Naia receive = 889 ns (free); rendering untested | BM-004: browser FPS benchmark on mid-range hardware | BM-004 |
| Full-stack tick cost higher than estimated | **Critical** | Medium — physics estimate only | ~558 µs estimated; Naia-only measured at 58 µs; Rapier is unknown | Run BM-001 immediately | BM-001 |
| Bandwidth higher than expected | **Medium** | Low — event-driven model should be far below O(N²) | Wire capacity shows ∞ (not measured); formula says ~230 KB/s at 16v16 worst-case | BM-005: measure actual wire bytes per tick | BM-005 |
| Daemon worst-case load too high | **High** | Unknown — no daemon-load benchmark | Daemons are core to design but absent from all benchmarks | BM-002: full-stack 2v2 + daemon worst-case | BM-002 |
| PvP testbed misread as final product | **Medium** | Low — documented design intent | Documented in §2.4 | Keep §2.4 visible in planning; revisit product mode at Gate 4 | Gate 4 |
| Web account friction hurts first play | **High** | Medium — login walls are known conversion killers | No account system tested yet; no friction measurement | Test guest-vs-account flow before public alpha | Gate 3 |
| Acquisition weaker than expected | **High** | High — no marketing strategy yet | No external marketing attempted; no social loop measured | Measure invite rate and link share from first playtest; build community early | Gate 3, Gate 4 |
| Monetization assumptions too optimistic | **High** | High — all monetization is unproven assumption | $7.60/CCU and 3% sub conversion are industry rule-of-thumb assumptions | Label as assumptions throughout; do not plan spend on these numbers | Gate 5 |
| Steam launch premature | **Medium** | Medium — Steam temptation is real | No web traction yet; Steam requires real Linux build and payment adapter | Do not create Steam page until Gate 3 and Gate 2 pass | Gate 2, Gate 3 |
| Scope creep into platform before game proves fun | **High** | Medium — solo dev building infrastructure | Document is a planning doc, not a build order; platform features are §8 not §11 | Keep near-term execution plan (§11) narrow; revisit scope at each gate | Gate 1 |
| Solo-dev burnout / time budget | **High** | Medium — extended pre-revenue development | No external measure of time pressure yet | Set explicit milestone timelines; 2–4 week sprints toward Gate 1 | Gate 1 completion |
| Security/abuse/DDoS | **Medium** | Low at early stage; grows with visibility | No production exposure yet; Cloudflare for static assets | Rate limiting, server-side verification, Cloudflare for web layer; add abuse controls before public events | Gate 3 |
| Data/telemetry not actionable | **Medium** | Medium — telemetry design is often deferred | No telemetry schema defined yet | Define telemetry schema before first public playtest; instrument rematch and D1 return from day one | Before Gate 2/3 |

---

## §11 Near-Term Execution Plan

### §11.1 Next 2–4 Weeks

- Finish first playable PvP loop (2v2, real physics, real items)
- Instrument match completion and rematch desire in the first session flow
- Run full-stack 2v2 benchmark (BM-001): tick P50/P95/P99, wire bytes, memory
- Run or synthesize daemon-stress benchmark (BM-002 or synthetic daemon-load harness)
- Verify web account creation flow end-to-end
- Test PWA install path (install from browser, launch, connect)

### §11.2 Next 1–3 Months

- Public-ish web alpha (small invite list; scheduled playtest nights)
- Telemetry dashboard for rematch rate, D1/D7, session length
- Iterate controls, items, daemon cores based on tester feedback
- BM-004: measure browser performance on low/mid devices (laptop + mobile)
- Create first trailer or gif-worthy gameplay clips
- Harden account/session/reconnect handling
- Determine first daemon core list and first weapon list (see §16 Appendix C)

### §11.3 Next 3–6 Months

- Public web demo
- Larger scheduled events targeting 50–100 peak CCU
- Consider Steam page only after demo is legible and Gate 3 conditions are met
- Build community/Discord pipeline
- Decide monetization experiment only after Gate 5 conditions are met
- Revisit founder-salary math only if signals justify

---

## §12 Decision Gates

These are hard stops. Named decisions cannot be made until gate conditions are met.

**Gate 1 — Is It Fun?**
Condition: 10+ testers play independently, 5+ replay voluntarily without being asked, rematch rate is promising, controls are understandable within a few minutes by a new player. Gate 1 is the most important gate. Nothing else matters if this fails.

**Gate 2 — Is It Technically Stable?**
Condition: BM-001 passes (P95 tick ≤ 40 ms under 2v2 + real physics), BM-002 passes (daemon worst-case acceptable), browser client stable on mid-range laptop, reconnects work without data loss, no severe WAN or bandwidth surprise from BM-005.

**Gate 3 — Is Web Distribution Working?**
Condition: Players can click a link and play with no download required. Account system does not kill conversion. Invite links work and are being shared. Scheduled events create measurable CCU spikes. PWA install path is stable.

**Gate 4 — Is There Organic Pull?**
Condition: Players ask for more sessions without being prompted. At least some players join Discord or community channel. Players share links or clips externally. Players return after D7 without a notification. This is the retention gate.

**Gate 5 — Is Monetization Justified?**
Condition: Pass only after: retention proof (D7 ≥ 20%, D30 meaningful), repeated sessions per user, visible identity/customization desire from players, enough active audience to run a monetization experiment without poisoning trust. Do not add monetization pressure before Gate 4 is solid.

Additional hard gates carried forward from capacity analysis:
- Do not model any match size as a production default until full-stack benchmark passes P95 ≤ 40 ms for that size.
- Do not use Steam as a meaningful acquisition channel until real Linux-native / Steam Deck-quality build ships.
- Do not model Steam-channel revenue at Web/PWA margin until Steam payment policy is verified.
- Do not let Steam-specific purchase logic enter gameplay code. Ever.

---

## §13 Current Strategic Verdict

Cyberlith has a credible path to becoming a sustainable indie business, but the path is narrow and evidence-gated. The strongest strategic shape is not Steam-first, monetization-first, or platform-first. It is web/PWA-first, direct-account-enabled, free-to-playtest, PvP-as-mechanics-lab, and retention-before-revenue.

The near-term goal is not 700 CCU. The near-term goal is to make a small number of strangers replay the game voluntarily. If that happens, the technical foundation and web distribution strategy give Cyberlith a real chance to climb toward 250 peak CCU events, then 250 sustained CCU, then eventual founder break-even territory.

Server cost is probably not the blocker. Acquisition, retention, and the quality of the first playable loop are the actual risks. The Naia networking foundation is measured and solid. Everything above the networking layer — physics, game logic, daemon behavior, browser rendering, controls feel, match pacing — remains unproven under real conditions.

The plan is possible. It is not proven. The job now is to make the first playable loop undeniable.

---

## §14 Appendix A — Original Benchmark Tables

### Evidence Registry

All measured data used in this document. Labels used throughout: `[Measured]`, `[Measured, Naia-only]`, `[Extrapolated]`, `[Estimated]`, `[Assumption]`, `[External pricing, manually verified 2026-04]`, `[External pricing, unverified — check date]`, `[Policy risk, requires verification]`.

| ID | Scenario | Command | Date | Machine | Includes | Excludes | Result | Confidence |
|---|---|---|---|---|---|---|---|---|
| M-001 | halo_btb_16v16 level load | `cargo criterion -p naia-benches -- "scenarios/halo_btb_16v16"` | 2026-04-26 | lifetop (i9-12900HK, 62 GiB) | Naia entity replication, 16 clients, 10K tiles + 32 units, uncapped BW | Rapier, game logic, actual network transport | **5.2 s** (5,511.6 ms from capacity report) | High (Naia-only) |
| M-002 | halo_btb_16v16 idle tick | same | 2026-04-26 | lifetop | Naia server tick, 16 clients, 10K tiles, 0 mutations | Rapier, game logic | **63 µs** | High (Naia-only) |
| M-003 | halo_btb_16v16 active tick | same | 2026-04-26 | lifetop | Naia server tick, 16 clients, 32 mutations | Rapier, game logic | **58 µs** | High (Naia-only) |
| M-004 | halo_btb_16v16 client receive | same | 2026-04-26 | lifetop | One client receive path, active tick | Server tick cost, other clients | **889 ns** | High (Naia-only) |
| E-001 | Full game tick — 16v16 | — | — | — | Naia (M-003) + Rapier estimate + logic estimate + OS overhead | Not yet measured | **~558 µs** | Low |
| E-002 | Rapier physics — 32 bodies + 10K tiles | — | — | — | Physics tick only | Not yet measured | **~350 µs** | Low |
| E-003 | Active bandwidth — 16v16 | O(N²) formula | 2026-04-26 | — | Worst-case formula | Measured bytes; event-driven model | **~230 KB/s** | Low |
| E-004 | Player-count extrapolations | Naia M-002/M-003 linear scaling | 2026-04-26 | — | Naia scaling by N | Full-stack, physics, BW | Various | Low–Medium |

### Per-Cell Resource Costs — CPU

| Scenario | Tick cost | % of one core | Cells/core (realistic) | Label |
|---|---|---|---|---|
| Naia-only, idle | 63 µs | 0.16% | 254 | `[Measured, Naia-only]` M-002 |
| Naia-only, active | 58 µs | 0.15% | 274 | `[Measured, Naia-only]` M-003 |
| Full game (Naia + physics + logic) | ~558 µs | 1.40% | **~29** | `[Estimated, no benchmark]` E-001 |

### Per-Cell Resource Costs — Memory

| Component | Per-cell | Label |
|---|---|---|
| 10K HaloTile entities (server representation) | ~3 MB | `[Estimated]` |
| 32 HaloUnit entities + dirty tracking | ~50 KB | `[Estimated]` |
| 16 client connections (Naia state + buffers) | ~1.6 MB | `[Estimated]` |
| Network send/receive buffers (16 × 64 KB × 2) | ~2 MB | `[Estimated]` |
| Naia server overhead (room, routing, event queues) | ~500 KB | `[Estimated]` |
| Game logic state (positions, health, game state) | ~100 KB | `[Estimated]` |
| **Total per cell (16-player Halo BTB)** | **~7.2 MB** | `[Estimated]` |

### Per-Cell Resource Costs — Bandwidth (Outbound, Worst-Case O(N²) Formula)

**These figures use the O(N²) worst-case formula, NOT measured bytes. Production BW with event-driven model is estimated at 10–15× below this worst case `[Estimated]` — but this estimate itself is not measured. Run BM-005 to replace with measured values.**

```
active_bw_bytes_per_sec = N × 18 B × N × 25 Hz = 450 × N²  [Extrapolated, formula only]
```

| State | Calculation | Per-cell rate | Label |
|---|---|---|---|
| Level load burst | 10K × ~20 B/entity × 16 clients | ~3 MB over 80 ms (~37.5 MB/s peak) | `[Extrapolated]` |
| Active combat (32 mutations, 16 clients) | 32 × 18 B × 16 × 25 Hz | **~230 KB/s** | `[Extrapolated, formula only]` |
| Idle (keepalives + clock sync) | 16 × 5 B/tick × 25 Hz | **~2 KB/s** | `[Extrapolated]` |
| Realistic mix, 30% active duty | 0.3×230 + 0.7×2 | ~70 KB/s | `[Extrapolated]` |

### Tile-Count Sensitivity

| Map size | Memory/cell | CPU/cell | Level load | Label |
|---|---|---|---|---|
| 1K tiles (small arena) | ~1.5 MB | ~558 µs (same) | ~0.5 s | `[Estimated]` / `[Extrapolated]` |
| 10K tiles (Halo BTB, benchmarked Naia-only) | ~7.2 MB | ~558 µs (same) | ~5.2 s | `[Estimated]` / `[Measured, Naia-only]` M-001 |
| 32K tiles (large campaign) | ~22 MB | ~558 µs (same) | ~17 s | `[Extrapolated]` |
| 64K tiles (massive map) | ~45 MB | ~558 µs (same) | ~33 s | `[Extrapolated]` |

**Key leverage:** Tile count is free in CPU (Win 2 `[Measured, Naia-only]`). Map size does not affect per-tick server CPU. You only pay in RAM and level-load time.

### Player-Count Sensitivity

| Players/cell | Memory/cell | Naia tick (active) | Full tick (est.) | Label |
|---|---|---|---|---|
| 16 (8v8) | ~4 MB | ~29 µs | ~290 µs | `[Extrapolated]` from M-003 / `[Estimated]` |
| 20 (10v10) | ~5 MB | ~45 µs | ~380 µs | `[Extrapolated]` / `[Estimated]` |
| 32 (16v16, benchmarked) | ~7.2 MB | **58 µs** | **~558 µs** | `[Measured, Naia-only]` M-003 / `[Estimated]` |
| 64 (32v32) | ~14 MB | ~116 µs | ~760 µs | `[Extrapolated]` / `[Estimated]` |
| 80 (40v40) | ~21 MB | ~145 µs | ~880 µs | `[Extrapolated]` / `[Estimated]` |

### 4-Tier Match Architecture — Per-Tier CPU (Full-Stack Estimate)

Naia component: 0.113 µs × N² `[Measured, Naia-only]`. Physics + logic ≈ 11 µs × N `[Estimated]`. OS/overhead: 50 µs fixed `[Estimated]`.

| Tier | Naia (µs) | Physics + logic (µs) | Total (µs) | Cells/core (40% eff.) | Label |
|---|---|---|---|---|---|
| 2v2 | 1.8 | 66 | **~120** | **133** | `[Estimated, no benchmark]` |
| 5v5 | 11 | 110 | **~170** | **94** | `[Estimated, no benchmark]` |
| 10v10 | 45 | 231 | **~326** | **49** | `[Estimated, no benchmark]` |
| 40v40 | 725 | 880 | **~1,655** | **10** | `[Estimated, no benchmark]` |

### Minimum Player Pool to Fill Each Match Type

| Tier | Players/match | Minimum CCU to offer mode | Label |
|---|---|---|---|
| 2v2 | 4 | **~50 CCU** | `[Assumption]` |
| 5v5 | 10 | **~150 CCU** | `[Assumption]` |
| 10v10 | 20 | **~400 CCU** | `[Assumption]` |
| 40v40 | 80 | **~2,000 CCU** | `[Assumption]` |

### Portal Pre-Warming

Level load for 10K tiles = 5.2 seconds `[Measured, Naia-only]` M-001. Pre-warming protocol:

1. When player is 10+ seconds from a portal, begin booting the destination cell thread.
2. Destination cell spawns Naia server, allocates entity state, loads tiles.
3. When player crosses the portal, cell is already in steady state.
4. If destination cell is already running, instant join — no load time.

For 64K-tile maps: start pre-warming 40+ seconds before portal use. Note: This benchmark is Naia-only. Full game stack (level geometry load, physics init) may increase actual load time.

### Performance and Infrastructure Levers

**Event-driven position updates (bandwidth win):**

`EntityPhysicsInputs` is populated only on `CollisionEvent::Stopped` and `NetworkedMoveBuffer::set()` with a value-comparison guard. For an entity moving at constant velocity: zero position bytes transmitted on that tick. Real sustained BW is proportional to input-change rate, not tick rate. The O(N²) worst-case formula overestimates production BW by roughly one order of magnitude for typical gameplay `[Estimated]`.

**Tile physics BVH — replace N_tile colliders with one static trimesh:**

Every wall tile as a separate Rapier rigid body yields ~3,000–4,000 independent tile colliders in broad-phase for a 10K-tile map. Replacing with a single static `ColliderBuilder::trimesh()` reduces broad-phase to 1 AABB and narrow-phase to O(log N_triangles) BVH traversal. Expected impact: 2–4× reduction in physics tick time `[Estimated, no benchmark]`.

**Adaptive tick rate:**

Lobby/post-match/spectator cells at 5 Hz instead of 25 Hz → ~20% fleet-wide CPU reduction `[Estimated]`.

---

## §15 Appendix B — Original Revenue Scenario Tables

### Revenue Model Formulas

```
// Base revenue formula [Assumption — Web/PWA channel only]
monthly_revenue ≈ CCU × 10 × (0.03 × $7 + 0.275 × $2.00)
                = CCU × $7.60

// DLC revenue per CCU (moderate case, Web/PWA direct, 2 campaigns/year) [Estimated]
dlc_revenue_per_ccu = 0.17 × 10 × $14.27 × 2 / 12 = $4.04

// Combined per CCU with DLC [Estimated]
full_revenue_per_ccu = $7.60 + $4.04 = $11.64
variable_costs_per_ccu = $0.43
net_per_ccu = $11.21

// Two-channel revenue model [Assumption] — use once Steam/Web player split is known
// steam_gross = steam_players × steam_payer_share × avg_purchase_value
// web_gross   = web_players × web_payer_share × avg_purchase_value
// blended_net = steam_gross × (1 - ~0.30) + web_gross × (1 - ~0.029) - costs
// Until split is known: use web-only upper bound

// Rewarded ads: EXCLUDED from all scenarios — reference formula only
// monthly_ad_revenue_ref = CCU × 10 × 0.30 × 30 × ($5/1000) = CCU × $0.45
```

### Survival Case — Retention Proof

**Goal:** Not revenue. Prove the game retains players and has a viable economy.

| Metric | Value | Label |
|---|---|---|
| Target CCU | ~100 | `[Assumption]` |
| Revenue model | Subscription + cosmetics only (no DLC, no ads) | |
| Monthly gross | ~$760 | `[Estimated]` 100 CCU × $7.60 |
| Monthly costs | ~$220 (server $20 + overhead $200) | `[Estimated]` |
| After-tax net | ~$378 | `[Estimated]` |
| Match modes | 2v2 only | |

Gate to exit: D7 retention ≥ 30%, D30 retention ≥ 10%, microtx conversion ≥ 20%.

### Base Case — $5,000–10,000/Month Net

Primary target. Achievable via Web/PWA direct + moderate DLC.

```
// At 1,312 CCU (Web/PWA only, no rewarded ads, upper-bound margin) [Estimated]
Revenue: 1,312 × $11.64 = $15,272
Costs: $424 + 1,312 × $0.43 = $988
Pre-tax: $15,272 − $988 = $14,284
After-tax (30%): $14,284 × 0.70 ≈ $9,999
```

| CCU | Subs (~3% of MAU) | Estimated after-tax net | Label |
|---|---|---|---|
| 250 | 75 | ~$1,000–$2,000 | `[Estimated]` — well below founder target |
| 675 | ~203 | ~$5,000 | `[Estimated]` — founder break-even (DLC model) |
| 833 (250 subs) | 250 | **~$6,500** | `[Estimated]` |
| 1,000 | 300 | **~$7,870** | `[Estimated]` |
| 1,312 | ~394 | **~$10,000** | `[Estimated]` |
| 1,500 | 450 | **~$11,700** | `[Estimated]` |
| 2,025 | 608 | **~$15,800** | `[Estimated]` |

### DLC Economics — Platform Comparison

| Platform | DLC price | Platform cut | Dev net per $15 sale | Label |
|---|---|---|---|---|
| Steam | $15 | 30% | **$10.50** | `[External pricing, manually verified 2026-04]` |
| Google Play | $15 | 15% | $12.75 | `[External pricing, manually verified 2026-04]` |
| **Self-managed (Stripe)** | **$15** | **0%** | **$14.27** (2.9% + $0.30 only) | `[External pricing, manually verified 2026-04]` |

Web/PWA direct yields 36% more per DLC sale than through Steam. All DLC revenue estimates in this document use the Web/PWA (Stripe) margin as upper bound.

### DLC Revenue at 250 Premium Subs (Web/PWA, Moderate Case)

```
// 8,333 MAU (250 subs, 833 CCU) — Web/PWA channel only [Estimated]
Subscription:  250 × $7                        = $1,750
Microtx:       8,333 × 27.5% × $2/mo           = $4,583
DLC (moderate, 2/year): $20,193 × 2 / 12       = $3,366
Total gross:                                     $9,699

Costs:
  Server: 1 × $224                              = $  224
  Overhead:                                      = $  200
  Processing (subs + microtx): 2.9% × $6,333   = $  184
  Processing (DLC): 1,415 × 2 / 12 × $0.74     = $  175
  Total costs:                                   $  783

Pre-tax profit: $9,699 − $783                   = $8,916
After-tax (30%):                                = $6,241/month
```

### Sensitivity Analysis (vs Base Case, 1,312 CCU target)

All deltas vs base case ($7/mo sub, 27.5% microtx rate, 2 cosmetics/mo, 30% tax). `[Estimated]`

| Lever | Effect on CCU target | Notes |
|---|---|---|
| Sub price: $7→$10/mo | −11% (~1,120 CCU) | Sub effect muted; microtx dominates |
| Conversion rate: 3%→5% | −16% (~1,060 CCU) | Requires strong community |
| Microtx rate: 27.5%→40% | **−25% (~947 CCU)** | Best cosmetic cadence lever |
| Microtx frequency: 2→3/mo | **−27% (~920 CCU)** | More releases, larger catalog |
| Tax rate: 30%→20% | −12% (~1,110 CCU) | S-corp election dependent |
| Conservative ($3.60/CCU base) | **+111% (~2,660 CCU)** | Only if microtx barely converts |

### Crystal Currency Model

| Pack | Price | Crystals | Stripe fee | Dev net | Effective overhead |
|---|---|---|---|---|---|
| Small | $5 | 500C | $0.445 | $4.555 | 8.9% |
| Medium | $10 | 1,000C | $0.590 | $9.410 | 5.9% |
| Large | $20 | 2,000C | $0.880 | $19.120 | 4.4% |

### Server Cost as Fraction of Revenue

| CCU | Revenue/mo | Server cost (Hetzner CCX33 ~$62/mo) | Server % of revenue | Label |
|---|---|---|---|---|
| 50 | ~$380 | ~$62 | 16.3% | `[Estimated]` |
| 250 | ~$1,900 | ~$62 | 3.3% | `[Estimated]` |
| 675 | ~$5,130 | ~$62 | 1.2% | `[Estimated]` |
| 1,312 | ~$9,971 | ~$62 | 0.6% | `[Estimated]` |

Server cost is not the dominant business risk at any CCU level above 50.

---

## §16 Appendix C — Open Questions

These questions require decisions before the corresponding systems can be built or milestones can be planned.

| Question | Blocking |
|---|---|
| What is the exact first playable mode? (2v2 deathmatch? zone control? team objective?) | Gate 1 planning |
| What is the target session size for first public playtest? | BM-001, playtest scheduling |
| Guest account vs required login before first play? | Account system design, Gate 3 |
| First daemon core list — how many types at MVP launch? What do they do? | BM-002, daemon design lock |
| First weapon list — what items exist in the first 2v2 room? | Gate 1 content |
| First maps — how many, at what tile count, with what layout? | Gate 1, BM-001 sizing |
| First telemetry schema — which events get instrumented day one? | Retention measurement, Gate 4 |
| First public playtest date — when is Gate 2 targeted? | All near-term planning |
| Server provider choice for first production deployment — Hetzner CCX33 confirmed? | Infrastructure for Gate 3 |
| Database/account stack — which database, self-hosted or managed, schema design? | Account backend, Gate 3 |
| Steam timeline — when does the Steam page go live? What are the prerequisites? | Gate 3, distribution planning |
| Monetization timeline — when is the first monetization experiment run? | Gate 5 |
| Co-op PvE / adventure rooms — when does this enter the roadmap explicitly? | Product scope and sequencing |
| Community platform — Discord at what milestone? What is the community strategy? | Gate 4 |

---

*This document is the internal Cyberlith planning source of truth as of 2026-04-26. Update measured benchmark values in §6 and §14 as benchmarks run. Update business assumptions in §4 and §15 only when validated data replaces assumptions. Do not inflate estimates. Do not commit or perform any git operations on this file without explicit instruction.*
