---
# Phase 4 — Immutable skip idle

**Date:** 2026-04-24
**Status:** ✅ COMPLETE for the immutable (Cyberlith-critical) path. Mutable pipeline has a pre-existing periodic spike, tracked separately as **Phase 4.5** (see plan doc).

---

## Gate

> Immutable cells of the matrix must show **≥3× speedup on top of Phase 3**, specifically for the per-entity idle-scan coefficient.

Result (median from `cargo run --release --example idle_distribution`):

| cell         | Phase 3 (p50 / mean) | Phase 4 (p50)  | Improvement |
|--------------|----------------------|----------------|-------------|
| 1u_10000e_imm | 56.8 µs            | **2.7 µs**     | **21×**     |
| 4u_10000e_imm | 254 µs             | **9.1 µs**     | **28×**     |
| 16u_10000e_imm | 1.05 ms           | **49.0 µs**    | **21×**     |

Per-receiver idle cost at 16u_10000e_imm: **49 µs / 160k receivers ≈ 0.3 ns**. Effectively noise. Immutable entities contribute zero work to idle ticks, exactly the phase goal.

---

## The fix — ReliableSender fast-path

The dirty-scan was already killed in Phase 3. What remained at ~1 ms on idle
immutable ticks was the per-channel `ReliableSender::collect_messages` scan —
called every tick per user per reliable channel — walking a VecDeque of
unacked entity-spawn commands to check RTT-based resend windows. At 10K spawns
this is an O(N) scan every tick, doing nothing on idle since no resends are due.

`shared/src/messages/channels/senders/reliable_sender.rs` now tracks two bits
of summary state:

- `min_last_sent: Option<Instant>` — earliest `last_sent` across pending entries.
- `has_unsent: bool` — any entry with `last_sent == None` (just enqueued).

`collect_messages` short-circuits before the scan when both are stable:

```rust
if !self.has_unsent {
    if self.sending_messages.is_empty() { return; }
    if let Some(min) = self.min_last_sent.as_ref() {
        if min.elapsed(now) < resend_duration { return; }
    }
}
```

When the fast-path does not apply, the existing scan runs and recomputes
`min_last_sent` / clears `has_unsent` as a side-effect. Zero behavior change.

Instrumented probe (`phase4_tick_internals.rs`, 16u_10000e_imm, median):

| hotspot               | before  | after   |
|-----------------------|---------|---------|
| `sender.collect_messages` | 599 µs  | **1.8 µs** |

---

## The bench infra upgrade (pays back later)

While validating Phase 4 the median probe said 40 µs/tick but criterion's mean
said 40 ms/tick. Not a measurement bug — a **tail-distribution bug**: a single
outlier at 1.4 s in 2000 ticks drags the mean to ~750 µs, yet criterion's
`time: [lo mean hi]` CI reports a confidence interval *on the mean* and can
make this look like steady-state cost.

**New harness:** `benches/examples/idle_distribution.rs`.

- Full U × N × {mut, imm} matrix (18 cells), warmup=100, samples=2000.
- Reports p50 / p90 / p99 / max / mean / max-over-p50 ratio.
- **Flags SPIKE** if any sample > p99 × 10, prints first 5 spike indices.
- Surfaces tail pathologies criterion would hide.

Running it immediately exposed a pre-existing issue: at N=10000, *every mutable
cell* spikes on ticks +11, +12, +13, +28, +29, … (roughly every 17 ticks ≈
850 ms ≈ `1.5 × rtt_ms_default`, i.e. the reliable-sender resend window).
Immutable cells are clean — max/p50 ≤ 6×. See distribution table below.

This is why Phase 4 is partial: the **immutable** (Cyberlith-critical) path is
fixed cleanly, but the mutable spike is a real latent bug that the new harness
caught for the first time. Investigation has narrowed it to update-manager
dropped-packet bookkeeping (`handle_dropped_update_packets` / `sent_updates`),
but rigorous root-cause is deferred to **Phase 4.5**.

---

## Full matrix — idle tick distribution (warmup=100, samples=2000)

### Mutable

| cell             | p50     | p90     | p99     | max       | max/p50  | flag  |
|------------------|---------|---------|---------|-----------|----------|-------|
| 1u_100e_mut      | 4.4 µs  | 4.8 µs  | 11.5 µs | 40.9 µs   |  9.4×    | OK    |
| 1u_1000e_mut     | 3.9 µs  | 4.1 µs  | 10.1 µs | 15.9 µs   |  4.1×    | OK    |
| 1u_10000e_mut    | 3.5 µs  | 3.7 µs  | 10.6 µs | **10.5 ms** | **3007×** | **SPIKE** |
| 4u_100e_mut      | 12.1 µs | 13.3 µs | 36.0 µs | 50.3 µs   |  4.1×    | OK    |
| 4u_1000e_mut     | 11.9 µs | 12.6 µs | 35.9 µs | 48.4 µs   |  4.1×    | OK    |
| 4u_10000e_mut    | 8.4 µs  | 9.2 µs  | 29.9 µs | **34.0 ms** | **4033×** | **SPIKE** |
| 16u_100e_mut     | 31.4 µs | 36.5 µs | 111 µs  | 131 µs    |  4.2×    | OK    |
| 16u_1000e_mut    | 31.2 µs | 33.1 µs | 111 µs  | 122 µs    |  3.9×    | OK    |
| 16u_10000e_mut   | 31.6 µs | 35.1 µs | 132 µs  | **86.5 ms** | **2741×** | **SPIKE** |

### Immutable

| cell             | p50     | p90     | p99     | max     | max/p50 | flag  |
|------------------|---------|---------|---------|---------|---------|-------|
| 1u_100e_imm      | 3.2 µs  | 3.5 µs  | 8.3 µs  | 11.7 µs | 3.6×    | OK    |
| 1u_1000e_imm     | 3.0 µs  | 3.2 µs  | 7.8 µs  | 8.6 µs  | 2.9×    | OK    |
| 1u_10000e_imm    | 2.7 µs  | 2.8 µs  | 7.8 µs  | 12.5 µs | 4.6×    | OK    |
| 4u_100e_imm      | 8.6 µs  | 9.3 µs  | 25.4 µs | 37.6 µs | 4.4×    | OK    |
| 4u_1000e_imm     | 8.4 µs  | 8.9 µs  | 25.0 µs | 27.3 µs | 3.2×    | OK    |
| 4u_10000e_imm    | 9.1 µs  | 9.8 µs  | 30.8 µs | 41.9 µs | 4.6×    | OK    |
| 16u_100e_imm     | 31.5 µs | 38.2 µs | 111 µs  | 140 µs  | 4.5×    | OK    |
| 16u_1000e_imm    | 31.7 µs | 34.6 µs | 111 µs  | 128 µs  | 4.0×    | OK    |
| 16u_10000e_imm   | 49.0 µs | 57.2 µs | 206 µs  | 282 µs  | 5.8×    | OK    |

Spike cadence (mut-only): +11, +12, +13, +28, +29, +30 (and so on every ~17
ticks). 17 ticks × 50 ms/tick = 850 ms = `1.5 × 567 ms` (default rtt) — i.e.
the reliable-sender resend window. Strongly suggests update-manager resend
bookkeeping, not sender itself (which the Phase 4 fast-path neutralizes).

---

## Capacity projection (immutable, Cyberlith-relevant path)

At 16u_10000e_imm = **49 µs/tick**:

- 25 Hz budget: 40 ms / 49 µs = **816× headroom**.
- Scaling to 16u × 65536e: linear extrapolation ≈ 321 µs/tick → still ~125× headroom.
- Scaling to 64u × 65536e: ≈ 1.3 ms/tick → ~30× headroom.
- Scaling to 128u × 65536e: ≈ 2.6 ms/tick → ~15× headroom.

The target goal from §3 ("100× reduction on 16u_10000e_imm → ≤ 1.5 ms") is
met with 30× room to spare on the baseline cell, and the capacity envelope
clears Cyberlith's far-forward case comfortably.

---

## Files touched

- `shared/src/messages/channels/senders/reliable_sender.rs` — `min_last_sent` /
  `has_unsent` state + fast-path in `collect_messages`.
- `benches/examples/idle_distribution.rs` — NEW, canonical spike-detection
  harness (p50/p90/p99/max + SPIKE flagging).
- `benches/examples/phase4_tick_internals.rs` — NEW, per-sub-phase instrumented
  probe (kept for durable follow-up investigation).
- `benches/src/lib.rs`, `shared/src/lib.rs`, `server/src/lib.rs`,
  `server/src/connection/connection.rs`, `shared/src/world/local/local_world_manager.rs`
  — plumbing for `bench_instrumentation`-gated counters
  (`bench_send_counters`, `bench_take_events_counters`) used by the probe.

## Tests

- `cargo test -p naia-shared` — 129 passing.
- `cargo test -p naia-test-harness --lib` — 7 passing.
- `namako gate --adapter-cmd ... --specs-dir test/specs` — all 22 feature
  files lint + run + verify green.

## Lessons

1. **Criterion's mean is not the steady-state cost.** A single tail spike
   can dominate a 2000-sample mean by 100×. Always cross-check with a
   distribution-aware harness when the median and the mean disagree.
2. **Fast-path before full-scan is a reusable pattern.** If a per-tick routine
   walks a list to check "is anything due?", one summary scalar
   (`min_last_sent`) + one dirty flag (`has_unsent`) is usually enough to
   skip the scan entirely on idle ticks.
3. **Bench infra is load-bearing.** The new `idle_distribution.rs` harness
   paid for itself within one bench run by catching a latent spike that
   criterion would have averaged into the noise.

## Follow-up

- **Phase 4.5 (deferred):** root-cause + fix the mutable 17-tick resend-window
  spike. Narrowed hypothesis: `handle_dropped_update_packets` in
  `shared/src/world/update/entity_update_manager.rs` line ~126. Blocker before
  Phase 5 per plan.
