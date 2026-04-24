# Naia Benchmark Suite

Wall-time (Criterion) and instruction-count (iai-callgrind) benchmarks for Naia,
plus loom concurrency tests and an HTML report generator.

---

## Quick start

```bash
# One-time installs
cargo install cargo-criterion
sudo apt install valgrind          # required for iai-callgrind (Ubuntu/Debian)
```

---

## Criterion (wall-time)

```bash
# Run the full suite
cargo criterion -p naia-benches

# Run one category
cargo criterion -p naia-benches -- tick/
cargo criterion -p naia-benches -- spawn/
cargo criterion -p naia-benches -- update/
cargo criterion -p naia-benches -- authority/
cargo criterion -p naia-benches -- wire/

# HTML reports land in target/criterion/ — open any index.html
```

### Baseline workflow (manual regression tracking)

```bash
# Save a named baseline on main/before a change
cargo criterion -p naia-benches -- --save-baseline main

# On your feature branch, compare against it
cargo criterion -p naia-benches -- --baseline main

# Or use critcmp for a cleaner diff table
cargo install critcmp
critcmp main HEAD
```

---

## iai-callgrind (deterministic instruction count)

Requires valgrind at runtime. Catches single-instruction regressions in the tick loop.

```bash
# Tick hot-path (primary gate)
cargo bench -p naia-iai --bench tick_hot_path

# Mutation dispatch pipeline
cargo bench -p naia-iai --bench update_dispatch
```

Outputs instruction count, L1/L2/LL cache hit rates, branch mispredicts.
Compare between branches by eye — no baseline tooling needed.

---

## Loom (concurrency model checking)

Exhaustive thread-interleaving tests for Win-3's dirty-set push model.

```bash
RUSTFLAGS="--cfg loom" cargo test -p naia-loom
```

This is a correctness test, not a benchmark — it has no performance output.

---

## HTML report generator

Reads `cargo criterion --message-format=json` output and produces a single
self-contained `bench_report.html` with Chart.js charts (no CDN, works offline).

```bash
# Run suite and pipe to report generator
cargo criterion -p naia-benches --message-format=json 2>/dev/null \
  | cargo run -p naia-bench-report \
  > bench_report.html

xdg-open bench_report.html   # Linux
open bench_report.html        # macOS

# Title override
cargo criterion -p naia-benches --message-format=json 2>/dev/null \
  | cargo run -p naia-bench-report -- --title "Feature branch: Win-3 rewrite" \
  > bench_report.html
```

---

## What to look for

| Benchmark | Expected shape | What it proves |
|---|---|---|
| `tick/idle` parametric | **Flat line** — time flat vs entity count | O(1) idle tick (Win 2 + 3) |
| `tick/active` parametric | Linear in K mutations, K << N | Work scales with mutations, not entities (Win 3) |
| `spawn/burst` | Reasonable linear, fast constant | 10K tiles load in bounded time (Win 1) |
| `update/immutable` mutable vs immutable | Delta ≤ noise floor | Zero diff-tracking cost (Win 5) |
| `spawn/coalesced` | Coalesced < legacy for ≥ 2 components | Wire efficiency improvement (Win 4) |

A rising curve in `tick/idle` means Win 2 or Win 3 has regressed.
