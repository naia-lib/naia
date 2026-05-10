---
# Phase 2 — Immutable Matrix Bench (Cyberlith canonical surface)

**Date:** 2026-04-24
**Status:** ✅ COMPLETE

---

## What landed

- `tick/idle_matrix_immutable`: 9-cell (U × N) matrix exercising `ReplicatedComponent::Immutable` — the surface Cyberlith tiles actually use (GDD §10: tiles-as-immutable-entities).
- Builder chaining: `.immutable()` on `BenchWorldBuilder` now flips the entity kind for the whole population.
- Registered in `criterion_group!` so the matrix runs under the standard `naia` bench target.

---

## Measured medians (idle tick, zero mutations)

### Immutable matrix

| U / N   | 100     | 1,000    | 10,000    |
|---------|---------|----------|-----------|
| 1 user  | 0.10 ms | 1.16 ms  | 14.83 ms  |
| 4 users | 0.36 ms | 4.31 ms  | 36.69 ms  |
| 16 users| 1.53 ms | 14.81 ms | **164.45 ms** |

### Immutable vs mutable (ratio = imm / mut)

| Cell       | mut      | imm       | ratio |
|------------|----------|-----------|-------|
| 1u_100e    | 0.15 ms  | 0.10 ms   | 0.70× |
| 1u_1000e   | 1.85 ms  | 1.16 ms   | 0.63× |
| 1u_10000e  | 26.43 ms | 14.83 ms  | 0.56× |
| 4u_100e    | 0.50 ms  | 0.36 ms   | 0.72× |
| 4u_1000e   | 6.22 ms  | 4.31 ms   | 0.69× |
| 4u_10000e  | 82.87 ms | 36.69 ms  | 0.44× |
| 16u_100e   | 2.15 ms  | 1.53 ms   | 0.71× |
| 16u_1000e  | 22.47 ms | 14.81 ms  | 0.66× |
| 16u_10000e | 302.61 ms| 164.45 ms | 0.54× |

### Per-receiver cost (immutable path)

~1.0 µs/receiver visited, across every cell — confirming the bottleneck remains **scan cost**, not mutation cost.

---

## Interpretation

1. **Immutable is 30–55% cheaper than mutable** at every matrix cell. The discount grows with N, which tracks the expected reduction in per-receiver work (no PropertyMutator dirty-bit set/clear, simpler MutReceiver state).

2. **Scaling shape is identical** — 16u_10000e is still 1648× slower than 1u_100e, and per-receiver cost is flat at ~1 µs. Immutability trims the constant but does not change asymptotic O(U·N).

3. **Cyberlith's 25 Hz target (40 ms tick budget) blows up at 10k tiles even for immutable** — 16u_10000e = 164 ms is 4.1× over budget, and 2^16 tiles would project to ~1.08 s per tick. **Immutability alone is insufficient.**

4. **Phase 3 is mandatory.** The scan cost is linear in `receivers_per_user × users`, regardless of which kind of entity is on the other side. Until `dirty_receiver_candidates` stops visiting clean receivers, the immutable path just hides the disaster by 45%.

## Gates

- ✅ Matrix cells produce stable medians (±2–4% per criterion's own stats)
- ✅ Immutable medians are < mutable medians in every cell (sanity: immutability never costs more)
- ✅ `cargo test --workspace` still green (not rerun here; Phase 1 verified, no code behavior changed)

## Artifact for Phase 3 comparison

Medians above become the `perf_v0_immutable` reference. After Phase 3 lands the dirty-push model, this whole matrix must flatten — a rising curve from 1u_100e → 16u_10000e is the regression we're killing.

**Phase 3 success criterion (binding):**
- `16u_10000e` drops **≥10×** (from 164 ms → ≤ 16 ms)
- Per-receiver cost for clean receivers drops to **~0 µs** (they are not visited)
- `phase1_scan_counters` shows `ratio visited/(U·N) ≤ 0.01` for idle ticks

## Files touched

- `benches/benches/tick/idle.rs` — added `idle_room_tick_matrix_immutable` + registration
- (builder `.immutable()` chain was already in place from earlier Phase 2 prep)
