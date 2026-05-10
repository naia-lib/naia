# Phase 4.5 — Mutable resend-window spike (CLOSED 2026-04-24 by sidequest absorption)

**Outcome:** Absorbed by Priority Accumulator Sidequest Phase A bandwidth cap (`b710ca4e`). No dedicated Phase 4.5 fix needed — the token-bucket bandwidth gate caps per-tick outbound at MTU + budget, so the periodic 10K-item resend burst now spreads across ticks as steady-state load instead of a single spike.

## Before (2026-04-24, pre-sidequest)

From `idle_distribution` at the close of Phase 4, BEFORE the priority accumulator landed:

| cell              | p50     | max        | max/p50  | flag      |
|-------------------|---------|------------|----------|-----------|
| 1u_10000e_mut     | 3.5 µs  | 10.5 ms    | 3007×    | **SPIKE** |
| 4u_10000e_mut     | 8.4 µs  | 34.0 ms    | 4033×    | **SPIKE** |
| 16u_10000e_mut    | 31.6 µs | 86.5 ms    | 2741×    | **SPIKE** |

Spike cadence: every ~17 ticks (≈ `1.5 × rtt`) — reliable-sender resend window.

## After (2026-04-24, post-sidequest A+B)

`cargo run --release -p naia-benches --example idle_distribution` (warmup=100, samples=2000):

### Mutable

| cell              | p50      | p90      | p99      | max        | mean     | max/p50 | flag  |
|-------------------|----------|----------|----------|------------|----------|---------|-------|
| 1u_100e_mut       | 4.7 µs   | 5.0 µs   | 12.2 µs  | 31.1 µs    | 5.1 µs   | 6.6×    | OK    |
| 1u_1000e_mut      | 4.3 µs   | 4.8 µs   | 11.2 µs  | 19.9 µs    | 4.7 µs   | 4.6×    | OK    |
| 1u_10000e_mut     | 4.5 µs   | 5.2 µs   | 13.7 µs  | 26.3 µs    | 5.0 µs   | 5.8×    | OK    |
| 4u_100e_mut       | 14.7 µs  | 16.3 µs  | 44.6 µs  | 55.1 µs    | 16.4 µs  | 3.8×    | OK    |
| 4u_1000e_mut      | 13.8 µs  | 15.1 µs  | 41.7 µs  | 56.4 µs    | 15.3 µs  | 4.1×    | OK    |
| 4u_10000e_mut     | 17.9 µs  | 20.9 µs  | 67.0 µs  | 114.9 µs   | 20.4 µs  | 6.4×    | OK    |
| 16u_100e_mut      | 59.7 µs  | 72.8 µs  | 219.5 µs | 253.1 µs   | 66.7 µs  | 4.2×    | OK    |
| 16u_1000e_mut     | 57.7 µs  | 74.4 µs  | 214.3 µs | 261.1 µs   | 66.4 µs  | 4.5×    | OK    |
| 16u_10000e_mut    | 40.5 µs  | 46.5 µs  | 146.6 µs | 167.1 µs   | 45.0 µs  | 4.1×    | OK    |

### Immutable (regression gate)

| cell              | p50      | p90      | p99      | max        | mean     | max/p50 | flag  |
|-------------------|----------|----------|----------|------------|----------|---------|-------|
| 1u_100e_imm       | 3.7 µs   | 4.0 µs   | 8.9 µs   | 13.4 µs    | 4.0 µs   | 3.6×    | OK    |
| 1u_1000e_imm      | 3.4 µs   | 3.6 µs   | 7.8 µs   | 10.0 µs    | 3.6 µs   | 2.9×    | OK    |
| 1u_10000e_imm     | 3.4 µs   | 3.6 µs   | 8.7 µs   | 16.1 µs    | 3.7 µs   | 4.7×    | OK    |
| 4u_100e_imm       | 11.2 µs  | 15.0 µs  | 30.1 µs  | 44.9 µs    | 12.3 µs  | 4.0×    | OK    |
| 4u_1000e_imm      | 12.1 µs  | 14.1 µs  | 37.4 µs  | 98.4 µs    | 13.4 µs  | 8.1×    | OK    |
| 4u_10000e_imm     | 12.0 µs  | 12.3 µs  | 35.4 µs  | 44.4 µs    | 13.1 µs  | 3.7×    | OK    |
| 16u_100e_imm      | 45.4 µs  | 52.0 µs  | 153.6 µs | 214.3 µs   | 49.3 µs  | 4.7×    | OK    |
| 16u_1000e_imm     | 42.6 µs  | 47.2 µs  | 147.3 µs | 194.0 µs   | 46.8 µs  | 4.6×    | OK    |
| 16u_10000e_imm    | 42.3 µs  | 46.9 µs  | 157.6 µs | 175.1 µs   | 46.5 µs  | 4.1×    | OK    |

**Every cell reports `OK` (max/p50 ≤ 10×).** Mutable 10K-entity cells went from 2741–4033× down to 4.1–6.4×. Immutable cells are uniformly clean (2.9–8.1×) — no regression from the sidequest's channel-sort + bandwidth-gate overhead.

## Root cause attribution

Sidequest Phase A added:

1. **Token-bucket bandwidth accumulator** (`shared/src/connection/bandwidth_accumulator.rs`) — caps per-tick outbound at `budget + MTU` (one-packet overshoot). Default budget 64 000 B/s × 16.67 ms ≈ 1 067 B/tick.
2. **Gate in `send_packet`** — both server and client stop writing packets once the tick cap is hit; carry-forward to next tick.

The resend burst's wire pressure (~10K × message size ≫ 1 067 B) now bleeds across ~N ticks of steady-state load, where spike behavior was previously a single cliff per resend window. The per-tick CPU cost correspondingly caps at "one MTU of work," which is O(MTU / avg-item-size) ≈ low dozens of items, not 10 000.

## Files confirming absorption

- `shared/src/connection/bandwidth_accumulator.rs` — token-bucket + 9 unit tests
- `server/src/connection/connection.rs`, `client/src/connection/connection.rs` — `can_spend_bandwidth` gate in `send_packet`
- `shared/src/connection/priority_accumulator_integration_tests.rs` — A-BDD-1 (tick cap), A-BDD-2 (drain), A-BDD-7 (bounded catchup)

## Status

Phase 4.5 CLOSED by absorption. No dedicated fix authored. Phase 5 (spatial scope index) unblocked.
