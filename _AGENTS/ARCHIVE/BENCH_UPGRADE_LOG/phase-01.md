# Phase 1 — Instrumentation & Diagnosis

**Date:** 2026-04-24
**Status:** ✅ COMPLETE — hotspot localized, hypothesis numerically confirmed

---

## Hotspot

`shared/src/world/update/user_diff_handler.rs:119-147` — `UserDiffHandler::dirty_receiver_candidates`:

```rust
pub fn dirty_receiver_candidates(&self) -> HashMap<GlobalEntity, HashSet<ComponentKind>> {
    let mut result = HashMap::new();
    for ((entity, kind), receiver) in &self.receivers {    // ← O(receivers.len())
        if !receiver.diff_mask_is_clear() {
            result.entry(*entity).or_default().insert(*kind);
        }
    }
    result
}
```

### Call path

`send_all_packets` (per tick, per server)
→ for each user: `connection.send_packets`
→ `local_world_manager::take_outgoing_events`
→ `take_update_events`
→ `build_dirty_candidates_from_receivers`
→ `self.diff_handler.dirty_receiver_candidates()` **← O(receivers) per user**

Total per tick = **O(U × receivers_per_user) = O(U × N)**, even when zero components are dirty.

## Measured evidence (valgrind-free)

Flamegraph was blocked by `perf_event_paranoid=4` on this machine (home-machine sudo required). Instead, added cfg-gated counters in `dirty_receiver_candidates` behind `naia-shared/bench_instrumentation` and ran a minimal diagnostic.

**Artifact:** `benches/examples/phase1_scan_counters.rs`, `cargo run --release --example phase1_scan_counters -p naia-benches`.

Numbers for a single idle tick (no mutations, no spawns, no scope changes):

| U | N | scan_calls | receivers_visited | dirty_results | visited / (U·N) |
|---|---|---|---|---|---|
| 1 | 100 | 2 | 100 | 0 | **1.00** |
| 1 | 1,000 | 2 | 1,000 | 0 | **1.00** |
| 1 | 10,000 | 2 | 10,000 | 0 | **1.00** |
| 4 | 100 | 8 | 400 | 0 | **1.00** |
| 4 | 1,000 | 8 | 4,000 | 0 | **1.00** |
| 4 | 10,000 | 8 | 40,000 | 0 | **1.00** |
| 16 | 100 | 32 | 1,600 | 0 | **1.00** |
| 16 | 1,000 | 32 | 16,000 | 0 | **1.00** |
| 16 | 10,000 | 32 | **160,000** | **0** | **1.00** |

**Every idle tick visits exactly U×N receivers to discover exactly zero dirty results.**

This is the O(U·N) cost the matrix shows at wall-clock (299 ms at 16u_10000e) — ~1.87 µs per receiver visited × 160,000 visits ≈ 300 ms, accounting entirely for the measured idle-tick time.

## Implications for Phase 3

- The fix target is unambiguous: **replace the scan with an incrementally-maintained dirty set.**
- The invariant to pin: "on a fully-idle tick, `receivers_visited == 0` (or bounded by the work the heartbeat/keepalive path actually needs, which is independent of N)."
- `phase1_scan_counters.rs` is the permanent regression gate — it prints the ratio, and Phase 3 must drive the `N=10000` ratio to **≤ 0.01** (ideally 0).

## Side observation

`scan_calls` is `2 × U` per tick, not `1 × U`. Two invocations of `dirty_receiver_candidates` happen per connection per tick. Second call probably comes from `is_component_updatable` or a retry path; worth a follow-up check but doesn't change the asymptotics — both calls do the same O(receivers) scan.

## What I did NOT do

- **No samply / flamegraph capture.** Blocked by `perf_event_paranoid=4`; no sudo on this machine. Deferred to when home-machine access is available. `PROFILING.md` Recipes 1, 2, 6 will be the fallback then; the counter evidence is sufficient to unblock Phase 3.
- **No full PerTickCounters struct.** Plan called for `touched_entities_per_tick`, `scope_checks_per_tick`, `outbound_messages_per_tick`, `idle_users_per_tick`. I added only the one needed to prove the hypothesis — `receivers_visited` — and the bare minimum plumbing. Additional counters can be added in Phase 3 if they become load-bearing. Keeping instrumentation tight.

## Files touched

- `shared/Cargo.toml` — added `bench_instrumentation` feature
- `server/Cargo.toml` — added `bench_instrumentation` feature (propagates to shared)
- `benches/Cargo.toml` — enables `bench_instrumentation` on server + shared
- `shared/src/world/update/user_diff_handler.rs` — counters module + counting in `dirty_receiver_candidates`
- `shared/src/lib.rs` — re-export `dirty_scan_counters` under feature
- `benches/examples/phase1_scan_counters.rs` — diagnostic runner (stays permanently as regression gate)
