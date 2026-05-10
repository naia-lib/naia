---
# Phase 3 — Kill the O(U·N) idle tick

**Date:** 2026-04-24
**Status:** ✅ COMPLETE — gate met with margin

---

## Gate

> `16u_10000e` drops **≥10×** (from 164 ms → ≤ 16 ms) and `phase1_scan_counters`
> shows `visited/(U·N) ≤ 0.01` on idle ticks.

Result: **16u_10000e (mut) = 1.60 ms, 189× improvement. Dirty-set visited = 0 per idle tick. Gate smashed.**

---

## Two wins, honestly attributed

Phase 3 delivered two independent improvements; the bench log should credit both.

### Win A — Dirty-push model (this phase's real work)

Replaced `UserDiffHandler::dirty_receiver_candidates`' O(receivers) scan with an
incrementally-maintained `DirtySet` indexed by `(GlobalEntity, ComponentKind)`.

- `MutReceiver` now carries an `Arc<OnceLock<DirtyNotifier>>`.
- `UserDiffHandler::register_component` attaches a `DirtyNotifier` (holding a
  `Weak<DirtySet>`) the first time a receiver is bound.
- `MutReceiver::mutate` / `or_mask` / `clear_mask` fire `notify_dirty` /
  `notify_clean` on the clean↔dirty transitions — O(1) per edge, zero work
  otherwise.
- `dirty_receiver_candidates` now reads the set directly (O(dirty), not O(N)).

Counters (feature `bench_instrumentation`, idle 16u×10k):

| metric             | before | after |
|--------------------|--------|-------|
| scan_calls         | 16     | 16    |
| receivers_visited  | 160000 | **0** |

### Win B — Bench methodology (Bevy-idiom reframe)

The prior matrix used `iter_batched(LargeInput)` — criterion pre-builds ~10
inputs per batch. At 16u×10k each world is ~several hundred MB; ten worlds
drove the machine into swap, and the thrash showed up **as if** it were per-tick
cost. The measured 164–302 ms was dominated by page faults, not algorithmic
work.

Fix: `b.iter(|| world.tick())` with the world built **once per cell**. This is
the idiom Bevy uses for `world.update()` benches — it measures the hot path
instead of the allocator.

### Bisect — what each win actually bought

| Config                              | 16u_10000e mut |
|-------------------------------------|----------------|
| Phase 2 (LargeInput + scan)         | 302 ms         |
| **iter + scan** (bench fix alone)   | **31.7 ms**    |
| **iter + dirty-push** (full Phase 3)| **1.60 ms**    |

- Bench fix alone: **~10× (swap artifact removal)**
- Dirty-push alone, over the corrected bench: **~20× (real scan-kill)**
- Combined headline: **189×**

Both numbers are legitimate wins but measure different things. The Phase 2
baseline numbers were real (same hardware, same code), and the matrix sweep
**was** flagging a real problem — it just wasn't the problem we expected.

---

## The measurement detective story (for next time)

This is the narrative to retain. Trust the evidence, not the hypothesis.

1. Phase 2 landed the immutable matrix. 16u_10000e measured 164 ms; per-receiver
   cost looked like ~1 µs/receiver. That matched a "dirty-scan is the O(U·N)
   cost" hypothesis.
2. Phase 3 landed dirty-push. Counters confirmed the scan was gone (0
   receivers visited). Bench wall-time **regressed 18%**.
3. Under the "evidence-based scientist" mandate, I wrote
   `examples/phase3_tick_breakdown.rs`: fresh build, one tick, then three
   steady ticks — medians across 5 runs. Steady tick at 16u_10000e = **0.9 ms**.
   Criterion reported **358 ms**. **500× discrepancy.**
4. The gap ruled out algorithmic cost as the criterion bottleneck. Reviewed
   `BatchSize::LargeInput` semantics: pre-builds up to 10 inputs per batch.
   Ten 16u×10k worlds × ~100s of MB each → swap. The "scan cost" was phantom
   page faults.
5. Reframed bench with `b.iter` (world once per cell). Numbers dropped two
   orders of magnitude. Immediately stashed the dirty-push patch and reran —
   31.7 ms with new bench, old scan. The scan was real; just 10× smaller than
   we'd been measuring.

**Lesson:** a criterion result is the product of (algorithm + memory + harness).
Always have an out-of-harness timing (a raw `Instant::now()` diagnostic) to
cross-check before chasing a regression inside the harness.

---

## Full matrix — new baseline

### Mutable

| U / N   | 100      | 1,000    | 10,000   |
|---------|----------|----------|----------|
| 1 user  | 3.18 µs  | 3.11 µs  | 62.3 µs  |
| 4 users | 7.09 µs  | 8.63 µs  | 203 µs   |
| 16 users| 23.5 µs  | 54.1 µs  | **1.60 ms** |

### Immutable

| U / N   | 100      | 1,000    | 10,000   |
|---------|----------|----------|----------|
| 1 user  | 2.37 µs  | 3.20 µs  | 56.8 µs  |
| 4 users | 8.90 µs  | 6.45 µs  | 254 µs   |
| 16 users| 33.3 µs  | 34.9 µs  | **1.05 ms** |

### Scaling shape

- **Per-receiver cost on idle tick has collapsed to ~0 µs.** The 16u_10000e
  cell at 1.05 ms for 160k receivers = ~6.6 ns per receiver. That's the cost
  of everything *not* the dirty scan (sender frame state, tick id advance,
  queue housekeeping) — not receiver visitation.
- **25 Hz budget (40 ms):** at 16u_10000e mut = 1.60 ms we have **25× headroom**.
  Projection to 2^16 tiles × 16 users at 1.60 ms × 6.5 = ~10.5 ms — still well
  under budget even before Phase 4's immutable-skip optimization.

---

## Files touched

- `shared/src/world/update/mut_channel.rs` — `DirtyNotifier`, `DirtySet` type,
  `MutReceiver::{attach_notifier, mutate, or_mask, clear_mask}` hooks.
- `shared/src/world/update/user_diff_handler.rs` — `dirty_set: Arc<DirtySet>`
  field, wiring in `register_component` / `deregister_component`, O(dirty) read
  in `dirty_receiver_candidates`, `bench_instrumentation` counter update.
- `benches/benches/tick/idle.rs` — drop `iter_batched(LargeInput)`, adopt
  `b.iter(|| world.tick())` on a pre-built world (Bevy idiom).
- `benches/examples/phase3_tick_breakdown.rs` — the diagnostic that broke the
  criterion mirage.

## Follow-ups for later phases

- **Phase 4 (immutable skip idle):** `1.05 ms` at 16u_10000e imm — already
  cheap, but per-receiver cost of 6.6 ns × 160k = ~1 ms is dominated by
  *something* we haven't localized. An immutable-entity fast path that skips
  dirty-set maintenance entirely should push this to < 200 µs.
- **Phase 5 (spatial scope index):** still relevant; scope_enter at 10k is not
  measured yet.
- **Phase 7 gate:** update `perf_v0` reference to the above matrix, tighten
  `--assert-wins` thresholds.
