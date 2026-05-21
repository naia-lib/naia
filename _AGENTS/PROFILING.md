# Naia Profiling Recipes (valgrind-free)

Working reference for the perf-upgrade project (`BENCH_PERF_UPGRADE.md`).
All recipes below run **without valgrind** — they use `samply`, `cargo-flamegraph`,
and criterion comparative mode only.

## Tools

```bash
cargo install samply         # Firefox Profiler-compatible sampling profiler
cargo install flamegraph     # inferno-based flamegraphs
```

Linux requirement for both: `perf_event_paranoid` ≤ 1. One-time, per-session:

```bash
sudo sh -c 'echo 1 > /proc/sys/kernel/perf_event_paranoid'
# or persist:  echo 'kernel.perf_event_paranoid = 1' | sudo tee -a /etc/sysctl.conf
```

Fallback when root is unavailable: `cargo bench` with `RUSTFLAGS="-C force-frame-pointers=yes"` gives slightly richer stack traces that samply renders well even without unwinding info.

## Recipe 1 — samply on a single criterion bench

```bash
# Build the bench binary without running
cargo bench -p naia-benches --bench naia --no-run
# Find the binary (name includes a hash)
BIN=$(ls -t target/release/deps/naia-* | grep -v '\.d$' | head -1)
# Profile just the idle_matrix 16u_10000e cell
samply record "$BIN" --bench --profile-time 15 tick/idle_matrix/u_x_n/16u_10000e
```

Output lands in the Firefox Profiler web UI automatically. Save the trace file (File → Save as → `.samply`) to `_AGENTS/BENCH_UPGRADE_LOG/phase-NN-samply.json.zst`.

## Recipe 2 — cargo flamegraph SVG

```bash
cargo flamegraph -p naia-benches --bench naia -o flamegraph.svg -- \
  --profile-time 10 tick/idle_matrix/u_x_n/16u_10000e
mv flamegraph.svg _AGENTS/BENCH_UPGRADE_LOG/phase-NN-flamegraph-before.svg
```

## Recipe 3 — perf stat for cycle/cache counts (no flamegraph)

```bash
BIN=$(ls -t target/release/deps/naia-* | grep -v '\.d$' | head -1)
perf stat -d "$BIN" --bench --profile-time 5 tick/idle_matrix/u_x_n/16u_10000e
```

Useful for "did this change move L1 miss rate?" without regenerating a full graph.

## Recipe 4 — criterion baseline diff

```bash
# freeze current numbers
cargo bench -p naia-benches --bench naia -- --save-baseline perf_vN

# after a change, compare
cargo bench -p naia-benches --bench naia -- --baseline perf_vN tick/idle_matrix
```

criterion prints `[-X% Y% +Z%]` per cell. Change detection threshold is 5% by default; below that criterion reports "no change detected."

Saved baselines live under `target/criterion/*/perf_vN/` and are **not** git-tracked (they're large, local, and regenerable). Persist wins via the phase log files (`_AGENTS/BENCH_UPGRADE_LOG/phase-NN.md`) instead — record the specific numbers in prose + the diff output.

## Recipe 5 — instruction count without valgrind

When criterion wall-clock is noisy and you want something deterministic, use `perf stat -e instructions`:

```bash
BIN=$(ls -t target/release/deps/naia-* | grep -v '\.d$' | head -1)
perf stat -e instructions,cycles,cache-misses "$BIN" \
  --bench --profile-time 3 tick/idle_matrix/u_x_n/16u_10000e
```

This is the closest valgrind-free analog to iai-callgrind. Noise is a few % rather than iai's <1%, but it doesn't need callgrind and it works in CI.

## Recipe 6 — quick "what's the hot function right now"

For a fast sanity check without saving a full trace:

```bash
BIN=$(ls -t target/release/deps/naia-* | grep -v '\.d$' | head -1)
perf record -F 997 -g --call-graph dwarf "$BIN" \
  --bench --profile-time 5 tick/idle_matrix/u_x_n/16u_10000e
perf report --stdio --sort dso,symbol | head -40
```

## Recipe 7 — game_cell pipeline overlap (cross-thread, end-to-end)

The recipes above profile naia in isolation (`naia-benches`). To measure naia *as the cyberlith game server uses it* — the `Recv ∥ Sim ∥ Send` per-tick pipeline running on worker threads — use the **`pipeline_timing`** aggregator, not the criterion benches.

- **Where:** `adapters/bevy/server/src/pipeline_timing.rs` — a process-global, thread-safe per-stage accumulator (atomic), gated by the `pipeline_timing` cargo feature. Records `recv`/`sim`/`send`/`tick`/`barrier`/`apply`/`snap` busy-ns; the consumer divides by the sim-tick count. It's process-global (not thread-local) precisely because Recv/Send run on worker threads that a thread-local accumulator on the driver thread can't see.
- **The architecture it measures:** under `Plugin::sim_integration_full` (cyberlith's game cell), naia owns Recv + Send worker threads. Each tick the consumer opens a **park window** (`park_workers()` → exclusive access → `unpark_workers()`) to get a consistent world snapshot. `workers_active = not(deterministic)` (`adapters/bevy/server/build.rs`) selects active workers vs. the parked-synchronous deterministic path. **Do not touch `park_workers`/`unpark_workers` internals without reading `cyberlith/.../memory/project_e6c_single_app_complete.md`** — they were stabilized against a `thread::park` token-race deadlock.
- **Run it (from the cyberlith repo, the consumer):**
  ```bash
  cargo run -p cyberlith_bench --release --no-default-features \
    --features parallel,jemalloc -- --scenario game_pipeline_parallel --warmup 400 --ticks 800
  ```
  Prints per-stage µs + `ratio avg_tick/max(stage)` + saturation headroom. This is the canonical overlap metric (target ratio → ~1.0). The cyberlith-side per-phase sub-attribution uses `bench_profile` (also process-global; add `,bench_profile`).

## Which recipe when

| Goal | Use |
|---|---|
| Find the hot function (naia in isolation) | Recipe 6 (perf report) or Recipe 2 (flamegraph SVG) |
| Share a browsable trace | Recipe 1 (samply) |
| Validate a micro-change | Recipe 3 (perf stat -d) or Recipe 5 (instructions) |
| Gate a naia phase complete | Recipe 4 (criterion baseline diff) — **required** |
| Measure naia in the cyberlith game-cell pipeline (overlap ratio, worker threads) | Recipe 7 (`pipeline_timing` + `game_pipeline_parallel`) |

## Gotchas

- **Release mode only.** Every recipe runs against the release-profile bench binary. Debug-profile profiles are garbage for perf work.
- **Warm the binary.** Criterion's internal warmup handles this for `cargo bench`, but `samply record` invocations should use `--profile-time ≥ 5` so the first few samples don't dominate.
- **LocalTransportHub noise.** The bench harness uses an in-process transport. This is deterministic on single-thread runs but can show lock contention artifacts on multi-user cells. If a flamegraph is dominated by `parking_lot` / `RwLock` calls, those are likely real in production too — the in-process transport is representative.
- **Frame pointers.** Cargo's default release profile is `--release -C opt-level=3` without frame pointers. We rely on DWARF unwinding (`--call-graph dwarf`) for `perf` and on samply's built-in dwarf support. If traces look truncated, add `RUSTFLAGS="-C force-frame-pointers=yes"` to the bench invocation.
- **Recipe 7 wire-byte totals are NOT a correctness metric.** The `game_pipeline_parallel` bench's `total_wire_s2c_bytes` swings ~4× run-to-run (in-process clients + worker scheduling). Use it for nothing; prove wire-identity via cyberlith's byte-exact e2e/sim suites. The stable signals are the per-stage µs + the overlap ratio. The `naia::receive_process`/`translate_mid`/`send_packets` bracket buckets also misattribute under pipeline mode (they bracket non-pipeline system sets that are empty there).
