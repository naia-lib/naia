# Capacity Analysis Hardening — Output Summary

**Date:** 2026-04-26
**Task:** Harden CAPACITY_ANALYSIS_2026-04-26.md into a living source-of-truth with explicit provenance, risk register, validation ladder, and multiple business scenarios.

---

## Summary Verdict

The document is now a defensible planning artifact. Every number carries a provenance label. The single most important finding from the hardening pass: **the full-stack tick estimate was inconsistently stated in the prior version** — the 408 µs figure may have double-counted Naia inside the physics estimate. The corrected conservative estimate is ~558 µs (Naia 58 + physics ~350 + logic ~100 + OS ~50). All capacity tables in §3–§14 carry `[Estimated, no benchmark]` labels and ±50% error bars until BM-001 (full-stack 2v2 benchmark) runs.

---

## Commands Run

```bash
cd /home/connor/Work/specops/naia && git rev-parse --short HEAD
# → 0a57c753

uname -a
# → Linux lifetop 6.8.0-110-generic ... x86_64

lscpu | grep -E "Model name|CPU\(s\)|Thread"
# → Model name: 12th Gen Intel(R) Core(TM) i9-12900HK
# → CPU(s): 20
# → Thread(s) per core: 2

free -h
# → Mem: 62Gi total, 29Gi used, 3.0Gi free, 30Gi buff/cache, 32Gi available
# → Swap: 2.0Gi total, 2.0Gi used
```

No benchmark runs were executed. All benchmark data is from the existing capacity report in the document (committed 2026-04-26, commit 0a57c753).

---

## Benchmark Results (from existing doc)

| Measurement | Value | Provenance |
|---|---|---|
| Level load (10K tiles + 32 units → 16 clients) | 5.2 s (5 511.6 ms from capacity report) | Measured, Naia-only |
| Server tick — idle (0 mutations, 16 clients) | 63 µs | Measured, Naia-only |
| Server tick — active (32 mutations, 16 clients) | 58 µs | Measured, Naia-only |
| Client receive — active tick (1 client) | 889 ns | Measured, Naia-only |
| Naia-only capacity (idle) | 649 concurrent games/core | Derived from 63 µs |
| Naia-only capacity (active) | 760 concurrent games/core | Derived from 58 µs |

---

## Files Changed

| File | Action |
|---|---|
| `/home/connor/Work/specops/naia/_AGENTS/CAPACITY_ANALYSIS_2026-04-26.md` | Overwritten in place — major restructure |
| `/home/connor/Work/specops/naia/_AGENTS/OUTPUT.md` | Created (this file) |

No code changes. No git operations.

---

## Data Provenance Changes

All numbers in the main document now carry one of these labels:
- `[Measured, Naia-only]` — from halo_btb_16v16 criterion run on lifetop, 2026-04-26
- `[Estimated, no benchmark]` — derived from engineering judgment, no benchmark backing
- `[Estimated]` — formula or structural reasoning, lower confidence
- `[Extrapolated]` — linear/quadratic extension of measured data point
- `[Extrapolated, formula only]` — O(N²) BW formula, no measured wire bytes
- `[Assumption]` — business model parameters with no external validation
- `[External pricing, manually verified 2026-04]` — cloud/platform pricing checked 2026-04
- `[Policy risk, requires verification]` — Steam payment policy, unconfirmed

**Key provenance correction:** The tick estimate changed from 408 µs (prior version) to ~558 µs (this version), reflecting a potential double-counting issue. This propagates to all cells/core and CCU estimates. Net effect: all CCU figures are ~25–30% lower than previously stated.

---

## Sections Rewritten / Added

| Section | Change |
|---|---|
| Document header | Added title, metadata block (date, commit, machine, confidence) |
| E. Executive Summary | NEW — candid what-is-proven / what-is-not / next gate |
| ER. Evidence Registry | NEW — table of all measured data points with IDs |
| §1 (was §1) | Added provenance labels; noted idle > active timing anomaly |
| §2 Tick budget | Added provenance labels; documented 408→558 µs correction |
| §3–§14 | All tables: provenance labels added; BW tables marked [Extrapolated, formula only] |
| §9 (was "Optimal Match Size") | REPLACED with Validation Ladder — 4 tiers with benchmark/playtest/business gates |
| §16 (was "Path to $10K") | REPLACED with three scenarios: Survival / Base / Upside |
| §18 (new) | Distribution and Steam/Platform Risk — Web/PWA, Steam discovery, Steam payment OPEN RISK |
| §19 (new) | Risk Register table — 12 risks with severity/likelihood/mitigation |
| §20 (new) | Decision Gates — 8 hard stops |
| §22 (new) | Provider Pricing Table with date stamps and shared vCPU warning |
| §23 (new) | Bandwidth Model — worst-case vs event-driven; proposed instrumentation metrics |
| §24 (new) | Measurement Backlog — 12 benchmarks, prioritized |
| §25 (was §18) | Appendix: Scaling Formulas — updated with provenance labels |

---

## Current Open Risks (Severity-Ranked)

1. **Full-stack tick cost unmeasured (Critical)** — All CCU capacity figures carry ±50% error bars. Run BM-001.
2. **Bandwidth not instrumented (Critical)** — Wire capacity shows ∞; O(N²) formula may be 10× too conservative. Run BM-005.
3. **Steam payment policy (High)** — Do not model Steam-channel revenue as full-margin until verified. Gate 5.
4. **Physics cost may be 2× estimate (High)** — No Rapier profiling done. Could flip binding constraint.
5. **Client performance on web/Wasm (High)** — Naia client is free (889 ns) but rendering on mid-range Wasm is unmeasured.
6. **Content treadmill (High)** — 2 DLC/year assumed; no proven release velocity.
7. **Shared CPU jitter (Medium)** — P99 tick spikes on shared vCPU plans may violate 40 ms budget.

---

## Recommended Next Benchmark Mission

**Priority 1: BM-001 — Full-stack 2v2 benchmark**

Run Cyberlith game server with:
- 4 players (2v2)
- Real Rapier physics (32 unit bodies + tile colliders)
- Real game logic (damage resolution, spawn, game state)
- Measure: tick wall time P50/P95/P99, server outgoing bytes per tick, memory per cell

This single benchmark replaces all `[Estimated, no benchmark]` labels in §2, §3a, §4, and resolves the primary business confidence blocker.

**Priority 2: BM-005 — Wire bytes**

Run `cargo criterion -p naia-benches -- "wire/bandwidth_realistic_quantized"` and record bytes per active tick. Feed into §3c formulas to replace `[Extrapolated, formula only]` with `[Measured]`.

**Priority 3: Gate 5 — Steam policy**

Read current Steam Subscriber Agreement. Contact Valve developer support. Confirm whether external checkout (Stripe) is permitted for IAP from Steam-installed games. Document the verdict in §18c and close or downgrade the risk.

---

## Update: Steam/Proton/Payments Strategy (2026-04-26)

**Date:** 2026-04-26
**Scope:** Major strategic update to CAPACITY_ANALYSIS_2026-04-26.md covering platform distribution, Steam framing, payment architecture, and business scenario channel-mix.

### Checklist

1. Steam/Web/PWA strategy updated — §18 replaced with full platform matrix and multi-channel framing. ✓
2. Steam modeled as separate lower-margin channel — explicit framing: Steam = lower-margin discovery; Web/PWA = high-margin direct payments. Two-channel revenue formula added to §26. ✓
3. Linux-native + Proton-compatible / no Windows-native reflected — platform matrix in §18b shows native Linux Steam build for Steam/Steam Deck; Proton for Windows Steam users; no Windows-native build (intentionally out of scope). Steam/Proton validation checklist added to §9. ✓
4. Rewarded ads removed from core plan — §15f downgraded to reference-only; ads excluded from all three business scenarios (Survival, Base, Upside). Revenue figures in §17d and §17e updated accordingly. ✓
5. Backend entitlement/payment-provider architecture added — new §19 "Payment Architecture — One Backend, Multiple Purchase Providers" with data model, SKU catalog, purchase flow invariants, and Steam account linking notes. ✓

### New sections added

| Section | Title |
|---|---|
| §18 | Distribution, Platform Matrix, and Steam Strategy (replaces old §18) |
| §19 | Payment Architecture — One Backend, Multiple Purchase Providers (new) |
| §21, Gates 9–15 | Seven new decision gates covering Steam monetization, entitlement architecture, Proton validation, Windows-native deferral, rewarded ads, and permanent block on platform logic in game code |
| §9 Steam/Proton track | 10-checkpoint validation checklist before Steam can be modeled as acquisition channel |
| §20 Risk Register | 8 new/updated risk rows covering Steam policy, entitlement fragmentation, Proton compatibility, rewarded ads, and more |
| §26 Formulas | Two-channel revenue model variables + `blended_net_revenue` formula |

### Remaining policy questions requiring direct Steamworks verification

1. **Can Steam games route users to external payment (Stripe) for in-app purchases?** Expected answer: no, or heavily restricted. Until confirmed, Steam-channel purchases must use Steam Wallet/DLC flows.
2. **Are Steam Wallet microtransactions permitted for free-to-play games without going through Steam Direct?** Verify Steamworks Partner documentation on MTX policy for F2P titles.
3. **What is the exact Steam fee structure for Steam Wallet purchases vs Steam DLC?** Confirm ~30% cut applies uniformly or varies by item type.
4. **Are there special rules for games targeting Steam Deck (Proton vs native)?** Verify if a Proton-compatible game without a Windows-native build can receive Steam Deck Verified/Playable status.
5. **Does linking to a PWA/web account from a Steam game violate any policy?** Verify the account-linking flow (SteamID → CyberlithAccount) is policy-compliant.

**Action:** Read https://partner.steamgames.com/doc/home and contact Valve developer support before Gates 9, 10, or 15 can be closed.
