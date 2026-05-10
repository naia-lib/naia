# Cyberlith Business & Technical Plan — Rewrite Output Summary

**Date:** 2026-04-26
**Task:** Rewrite CAPACITY_ANALYSIS_2026-04-26.md into an all-in-one planning document with business strategy, product strategy, game design pillars, survival math, and validation gates.

---

## Files Changed

| Action | Path |
|---|---|
| Renamed | `CAPACITY_ANALYSIS_2026-04-26.md` → `CYBERLITH_BUSINESS_AND_TECHNICAL_PLAN_2026-04-26.md` |
| Overwritten | `CYBERLITH_BUSINESS_AND_TECHNICAL_PLAN_2026-04-26.md` (new content, 888 lines) |
| Updated | `OUTPUT.md` (this file) |

## Rename Status

Rename completed successfully via `mv`. Verified with `ls -la`. No overwrite conflict — target file did not exist before the rename.

## Document Structure Summary

| Section | Description |
|---|---|
| §1 Executive Summary | Core claims, what is/isn't proven, immediate next gate |
| §2 Product Strategy | What Cyberlith is/isn't, distribution, PvP as wind tunnel |
| §3 Game Design Pillars | Camera/frame, input doctrine, two-hand item model, daemon item definitions, benchmark implications |
| §4 Business Model and Survival Math | $5K/month target, revenue model variables, DLC model, CCU break-even table, peak vs sustained distinction |
| §5 Technical Architecture Summary | High-level architecture, scaling philosophy, foundation status table |
| §6 Capacity Analysis | Measured Naia results with capacity report, what's unmeasured, correct interpretation, BM-001 through BM-007 benchmark ladder |
| §7 Infrastructure and Cost Model | Cost drivers, Hetzner/Vultr provider table, scaling conclusion |
| §8 Account, Platform, Entitlement | Direct accounts, free gameplay, Steam-later strategy, monetization-later strategy |
| §9 Playtest and Market Validation | Metrics before money, playtest stage table, 250 CCU peak vs sustained |
| §10 Risk Register | 15-risk table, blunt severity/likelihood/mitigation/gate columns |
| §11 Near-Term Execution Plan | 3 horizons: 2–4 weeks, 1–3 months, 3–6 months |
| §12 Decision Gates | 5 named gates + carried-forward hard gates |
| §13 Strategic Verdict | Evidence-gated framing, near-term goal statement |
| §14 Appendix A | Full evidence registry (M-001 through E-004), all benchmark tables, per-cell resource costs, tile/player-count sensitivity, tier CPU table, portal pre-warming, infrastructure levers |
| §15 Appendix B | Revenue formulas, survival/base case scenarios, DLC economics, 250-sub revenue calc, sensitivity analysis, Crystal model, server-cost-as-fraction table |
| §16 Appendix C | 14 open questions with blocking dependencies |

## Preserved Benchmark Facts

All measured values carried through exactly as in the original:

| ID | Value | Label |
|---|---|---|
| M-001 level load | 5.2 s (5,511.6 ms) | `[Measured, Naia-only]` |
| M-002 idle tick | 63 µs | `[Measured, Naia-only]` |
| M-003 active tick | 58 µs | `[Measured, Naia-only]` |
| M-004 client receive | 889 ns | `[Measured, Naia-only]` |
| E-001 full game tick | ~558 µs | `[Estimated, no benchmark]` |
| E-002 Rapier physics | ~350 µs | `[Estimated, no benchmark]` |
| E-003 active bandwidth 16v16 | ~230 KB/s | `[Extrapolated, formula only]` |
| Capacity report (649/760 Naia-only games/core) | Preserved verbatim in §6.1 | |
| Hetzner CCX33 ~$62/mo, 20 TB | Preserved in §7.2 | `[External pricing, manually verified 2026-04]` |

No benchmark values were invented, inflated, or omitted.

## Business Corrections Added

- $5,000/month take-home target explicitly stated (§4.1) with 6-month stability qualifier
- 250 CCU break-even fallacy explicitly corrected (§4.4 and §9.3): 250 CCU ≈ $1,000–$2,000/month take-home, not founder salary
- 250 peak CCU vs 250 sustained CCU distinction explicitly written out (§9.3)
- 700–1,000 sustained CCU range as realistic founder break-even (§4.4, §9.2, §9.3)
- Break-even CCU table by monetization model (§4.4)
- All monetization assumptions labeled `[Assumption]`; all revenue estimates labeled `[Estimated]`
- Steam-later framing made explicit and firm (§2.3, §8.3)
- Monetization-later framing made explicit and firm (§2.3, §8.4)
- Steam payment policy risk preserved with `[Policy risk, requires verification]` label

## Technical Validation Gates Added

- BM-001 through BM-007 benchmark ladder with priority and what-it-unblocks (§6.4)
- Daemon-worst-case benchmark (BM-002) called out as critical for daemon design lock
- Browser client render benchmark (BM-004) called out as critical for web distribution confidence
- 5 named decision gates with explicit conditions (§12)
- Hard gate: never model match size as production default without P95 ≤ 40 ms benchmark
- Hard gate: never put Steam-specific purchase logic in gameplay code

## Sections Removed or Reframed

| Original Section | Treatment |
|---|---|
| §5 Guild Wars Instancing model | Technical context folded into §5 Architecture and §14 Appendix A |
| §6 Tile Count as Design Lever | Preserved in §14 Appendix A |
| §7 Portal Pre-Warming | Preserved in §14 Appendix A |
| §8 40v40 Match Analysis | Numbers folded into §14 appendix tables; 40v40 framed as aspirational/event-only |
| §9 Validation Ladder | Reformatted and split: §6.4 (benchmark ladder) + §9.2 (playtest stages) + §12 (decision gates) |
| §13 4-Tier Match Architecture | Numbers preserved in §14 appendix; tier mix percentages preserved |
| §14–§15 Mixed Fleet | Numbers preserved in §14 appendix |
| §16 Business Plan Scenarios | Preserved and expanded in §4 + §15 Appendix B |
| §17 DLC Campaigns | Economics preserved in §15 Appendix B |
| §18 Distribution / Platform Matrix | Reframed and moved to §2 + §8 |
| §19 Payment Architecture | Core principles preserved in §8.4; backend data model moved to background context |
| §20 Risk Register | Expanded from original 18 risks to 15 focused risks per task spec |
| §21 Decision Gates | Reformatted as §12 with 5 named gates + hard-gate list |
| §22 Performance Levers | Preserved in §14 Appendix A |
| §23 Provider Pricing | Preserved in §7.2 |
| §24 Bandwidth Model | Preserved in §14 Appendix A |
| §25 Measurement Backlog | Reformatted as §6.4 benchmark ladder |
| §26 Appendix: Scaling Formulas | Core formulas preserved in §15 Appendix B |

## Commands Run / Status

| Command | Status |
|---|---|
| `ls -la /home/connor/Work/specops/naia/_AGENTS/` | OK — confirmed CAPACITY_ANALYSIS file existed, no pre-existing CYBERLITH_BUSINESS file |
| Read CAPACITY_ANALYSIS (1,348 lines in 3 passes) | OK — all content read |
| `mv CAPACITY_ANALYSIS_2026-04-26.md CYBERLITH_BUSINESS_AND_TECHNICAL_PLAN_2026-04-26.md` | OK — rename confirmed |
| Write new document (888 lines) | OK |
| `grep -n "^# \|^## " ...` — heading structure validation | OK — all 16 sections present |
| `grep -n "250 CCU\|700\|1,000\|PWA\|Control Antenna\|Control Relay\|Daemon Core\|Naia\|Rapier\|BM-001\|BM-002" ...` | OK — all required terms present |
| `which markdownlint` | Not available on this system; no markdownlint run |

## Stop Conditions — None Triggered

- CAPACITY_ANALYSIS file existed: YES
- No pre-existing CYBERLITH_BUSINESS file: CONFIRMED (no overwrite risk)
- No contradictory CCU/revenue formulas: formulas are consistent ($7.60/CCU base, $11.21/CCU with DLC, $5K take-home target)
- Benchmark data is clear: M-001 through M-004 have exact measured values; E-001/E-002 explicitly labeled `[Estimated, no benchmark]`

## Assumptions / Unresolved Questions

All inherited from original document; none added:
- CCU → MAU ×10 multiplier is `[Assumption]`
- 3% premium conversion is `[Assumption]`
- $7/mo subscription price is `[Assumption]`
- 27.5% microtx buyer rate is `[Assumption]`
- Rapier physics ~350 µs is `[Estimated, no benchmark]`
- Full game tick ~558 µs is `[Estimated, no benchmark]`
- All bandwidth figures are `[Extrapolated, formula only]` — wire bytes not measured
- Steam payment policy is `[Policy risk, requires verification]`

---

Ready for Connor review.
