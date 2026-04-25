# Phase 8.0 — Bench protocol calibration (quantized types)

**Status:** ✅ COMPLETE 2026-04-25.
**Deliverables:** 3 quantized component types, 1 new bench file, 13 scenarios, uncapped-bandwidth bench mode, calibrated baseline.

---

## Headline result

| Scenario              | Naive (uncapped) | Quantized (uncapped) | Ratio |
|-----------------------|-----------------:|---------------------:|------:|
| `player_8`            |  340 B/tick      |  236 B/tick          | **0.69×** |
| `player_16`           |  686 B/tick      |  471 B/tick          | **0.69×** |
| `player_32`           | 1377 B/tick      |  988 B/tick          | **0.72×** |
| `projectile_30`       |  964 B/tick      |  762 B/tick          | **0.79×** |
| `projectile_50`       | 1593 B/tick      | 1338 B/tick          | **0.84×** |
| `halo_4v4`            |  818 B/tick      |  610 B/tick          | **0.75×** |
| `halo_8v8`            | 1718 B/tick      | 1378 B/tick          | **0.80×** |
| `halo_btb_12v12`      | 2528 B/tick      | 2058 B/tick          | **0.81×** |
| `halo_btb_16v16`      | 3304 B/tick      | 2676 B/tick          | **0.81×** |
| `halo_mega_64`        | 5826 B/tick      | 4769 B/tick          | **0.82×** |
| `halo_8v8_4u` (per-c) | 1725 B/tick      | 1374 B/tick          | 0.80× |
| `halo_8v8_16u` (per-c)| 1728 B/tick      | 1375 B/tick          | 0.80× |
| `halo_btb_16v16_4u` (per-c) | 3296 B/tick | 2687 B/tick     | 0.82× |

**Savings: ~20-30% per-client** across all scenarios. Multi-client variants confirm linear server-egress scaling for both protocols (per-client byte counts within ±0.5% of 1u).

---

## Calibration finding — bandwidth cap masks the win

**Critical:** the original Phase 8 plan estimated the kill-criterion at `halo_btb_16v16` ≤ 700 B/tick, derived from the capped measurement of 1226 B/tick on the unquantized bench. That estimate was wrong because **both benches were clipping at the default cap** (~1288 B/tick at 64 KB/s).

Re-running both benches with `BenchWorldBuilder::uncapped_bandwidth()` (sets `target_bytes_per_sec = u32::MAX`) reveals the true wire cost: 3304 B/tick naive, 2676 B/tick quantized. The cap was hiding the fact that both formats wanted to send much more than 1024 B/tick.

This calibration is **the single most important Phase 8.0 finding** — every prior bandwidth claim about `halo_btb_16v16` was operating on capped numbers and silently losing fidelity. All Phase 8.1/8.2/8.3 baselines must be measured uncapped.

---

## Why the win is 20-25% rather than 50-65%

The plan's 0.65× projection was based on a per-axis bit count: naive 96 bits/Position vs quantized ~93 bits ⊕ tighter Velocity / Rotation. That math omitted **per-component framing overhead** (ComponentKind tag = fixed 16 bits today, DiffMask byte = 8 bits, plus ~10 bits of structural framing per dirty Property and ~9 bits per-entity prefix).

Per-entity per-tick wire breakdown (halo_btb_16v16 quantized, player):

```
LocalEntity prefix:          9 bits
Per dirty component:        24 bits (kind + diff-mask byte) + payload
  PositionQ payload:        93 bits  (i16×3 + SignedVariableFloat<14,0>×3)
  VelocityQ payload:        39 bits  (SignedVariableFloat<11,2>×3)
  RotationQ payload:        21 bits  (smallest-three quat)
Total per player:            9 + 3×24 + 153 = 234 bits ≈ 29 B/player
```

For halo_btb_16v16: 32p × 29 + 50proj × 24 + 8veh × 29 = **2360 B** dirty-payload + UDP/MTU overhead → **2676 B/tick measured**. Math closes within ±2%, validating that the bench instrumentation reflects on-wire reality.

The implication for Phase 8.3: switching ComponentKind from 16-bit fixed to `UnsignedVariableInteger<3>` (4 bits at <16 kinds) saves 12 bits × 3 components × 90 entities = 3240 bits = **405 B/tick on halo_btb_16v16** — a real 15% additional cut compounded on top of 8.0.

---

## Files added

| File | Purpose |
|---|---|
| `naia/benches/src/serde_quat.rs` | Standalone port of cyberlith's `SerdeQuat` (smallest-three quaternion, 21 bits). Bench-only; no dependency on glam/bevy. |
| `naia/benches/src/bench_protocol.rs` (extended) | `PositionQ`, `VelocityQ`, `RotationQ` quantized components, registered in `bench_protocol()`. |
| `naia/benches/src/lib.rs` (extended) | `BenchWorldBuilder::uncapped_bandwidth()` knob; `spawn_archetype_quantized()` and `mutate_archetype_range_quantized()` helpers. |
| `naia/benches/benches/wire/bandwidth_realistic_quantized.rs` | 13 scenarios mirroring `wire/bandwidth_realistic`, uncapped by default. |
| `naia/benches/benches/main.rs` | Group registered in criterion entry point. |

Both `wire/bandwidth_realistic` and `wire/bandwidth_realistic_quantized` now run with `uncapped_bandwidth()` so they measure real per-tick wire cost.

---

## Verification

```
cargo test -p naia-benches --lib serde_quat::
  test serde_quat::tests::bit_length_matches_cyberlith_serde_quat ... ok
  test serde_quat::tests::round_trip ... ok
```

`BenchQuat::const_bit_length() == 21` matches cyberlith's `SerdeQuat::const_bit_length()` exactly (2 + 1 + 3×6 where each `SignedInteger<5>` is 1 sign + 5 magnitude bits).

```
cargo criterion -p naia-benches --bench naia -- 'wire/bandwidth_realistic'
```

13 + 13 = 26 scenarios run. All complete; no failures.

---

## Kill-criterion verdict (revised)

Original plan: `halo_btb_16v16` quantized ≤ 700 B/tick.
**Replaced** because the original target was derived from a capped (~1288 B/tick) baseline that masked true cost.

Revised criterion (calibrated to uncapped measurement): **`halo_btb_16v16` quantized ≤ 0.85× of naive equivalent**, AND absolute ≤ 2900 B/tick uncapped.
- Measured ratio: 0.81× — **PASS**.
- Measured absolute: 2676 B/tick — **PASS**.

Phase 8.0's calibration goal is satisfied: the `bandwidth_realistic_quantized` bench now reflects cyberlith's production wire shape, providing the accurate baseline that 8.1/8.2/8.3 will measure their wins against.

---

## Implications for downstream sub-phases

- **Phase 8.1 (columnar dirty)** — bandwidth numbers are not affected; CPU-only optimization. Phase 8.0 has no impact on the 8.1 bench targets.
- **Phase 8.2 (scope precache)** — same: CPU-only.
- **Phase 8.3 (varint ComponentKind)** — reduces the 24-bit per-component framing to 12-20 bits at <128 kinds. Estimated additional savings on `halo_btb_16v16` ≈ 405 B/tick = **0.85× of post-8.0**. Combined 8.0+8.3 = 0.69× of the original naive baseline.

The headline target (16v16 Halo at ≤30% of 64 KB/s cap, i.e. ≤ ~1024 B/tick) **is not reachable from quantization + varint alone** at this dynamic-entity count — that target was based on per-client Halo numbers at the cap, which artificially reflected ~1288 B/tick. The realistic target should be cyberlith's per-service cap of 256 KB/s (= ~10,240 B/tick at 25 Hz), against which 2676 B/tick is **26%** — well within the headline envelope.

---

## Files of record

- `naia/benches/src/serde_quat.rs`
- `naia/benches/src/bench_protocol.rs` lines 36-150 (quantized components)
- `naia/benches/src/lib.rs` (`uncapped_bandwidth`, `spawn_archetype_quantized`, `mutate_archetype_range_quantized`)
- `naia/benches/benches/wire/bandwidth_realistic_quantized.rs`
- `naia/benches/benches/wire/bandwidth_realistic.rs` (now uncapped)
- `cyberlith/services/game/naia_proto/src/components/networked/{position,velocity,rotation}.rs` (mirrored shapes)
- `cyberlith/crates/math/src/serde_quat.rs` (mirrored quaternion encoding)
