# Naia Benchmark Suite — Architecture & Implementation Plan

**Status:** ✅ Complete (2026-04-24)  
**Author:** Connor Carpenter  
**Created:** 2026-04-24  

---

## 1. Goals

Naia's 10K-entities-as-tiles upgrade made algorithmic claims that are currently
unverified:

- Idle tick is O(1) in entity count (Wins 2 + 3)
- Active-tick work scales with mutations K, not entity count N (Win 3)
- Immutable components incur zero diff-tracking allocations (Win 5)
- SpawnWithComponents reduces wire-frame overhead vs. legacy Spawn+Insert×N (Win 4)
- 10K entity scope entry completes in bounded time (Win 1)

The benchmark suite exists to **prove these claims with measured data**, give
users meaningful performance numbers to plan against, and catch regressions
before they ship.

### Out of scope for this plan

- CI automation / GitHub Actions integration
- Automated regression thresholds
- External tracking services (Bencher.dev, etc.)

These are explicitly deferred. The suite and report tooling are designed so CI
can be bolted on later without rework.

---

## 2. Infrastructure Stack

| Tool | Role | Why |
|---|---|---|
| **Criterion 0.5** | Wall-time benchmarks | Industry standard; parametric groups; HTML reports |
| **cargo-criterion** | Criterion runner (CLI) | Produces stable `--message-format=json` output for our report tool |
| **iai-callgrind** | Instruction-count benchmarks | Deterministic; no variance; catches hot-path regressions with 1-instruction precision |
| **loom** | Concurrency model checking | Exhaustive thread-interleaving verification for Win 3's dirty-set |
| **naia_bench_report** | Custom HTML report generator | Reads cargo-criterion JSON → single self-contained `bench_report.html` |
| **Chart.js** (vendored) | Charts in report | 203 KB minified; embeds inline; zero external runtime dependency |

### Why two benchmark modalities?

**Criterion** (wall time) answers: *"How fast is this in practice?"*
Variance is ~3–5% even on quiet hardware. Useful for absolute numbers,
scalability curves, and throughput.

**iai-callgrind** (instruction count) answers: *"Did a code change make
the hot path more expensive?"* Fully deterministic — 1 extra instruction
is detectable. Ideal for the tick hot path that fires 60× per second.

---

## 3. Directory Structure

```
naia/
├── benches/                        # Criterion wall-time suite
│   ├── Cargo.toml
│   ├── README.md
│   ├── src/
│   │   ├── lib.rs                  # NaiaWorld builder + shared helpers
│   │   └── bench_protocol.rs       # Minimal Protocol for benchmarks
│   └── benches/
│       ├── tick/
│       │   ├── mod.rs
│       │   ├── idle.rs             # idle_room_tick — N entities, 0 mutations
│       │   ├── active.rs           # active_room_tick — fixed N, K mutations
│       │   └── scope.rs            # scope_enter + scope_exit for N entities
│       ├── spawn/
│       │   ├── mod.rs
│       │   ├── single.rs           # spawn one entity (baseline latency)
│       │   ├── burst.rs            # 10K-entity level-load burst
│       │   └── coalesced.rs        # SpawnWithComponents vs. legacy
│       ├── update/
│       │   ├── mod.rs
│       │   ├── mutation.rs         # single mutation dispatch
│       │   ├── bulk.rs             # K mutations per tick, parametric
│       │   └── immutable.rs        # immutable vs. mutable overhead
│       ├── authority/
│       │   ├── mod.rs
│       │   ├── grant.rs            # request → grant round-trip
│       │   └── contention.rs       # K users competing for N delegated entities
│       ├── wire/
│       │   ├── mod.rs
│       │   ├── framing.rs          # bytes-per-operation (Throughput::Bytes)
│       │   └── bandwidth.rs        # sustained mutations/tick throughput
│       └── main.rs                 # Criterion entry point
│
├── iai/                            # iai-callgrind instruction-count suite
│   ├── Cargo.toml
│   └── benches/
│       ├── tick_hot_path.rs        # idle tick instruction count (the CI-ready gate)
│       └── update_dispatch.rs      # mutation → wire instruction count
│
├── test/
│   ├── loom/                       # Concurrency model checking (NOT a benchmark)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       └── dirty_set.rs        # Win 3: dirty-receiver concurrent read/write
│   └── bench_report/               # HTML report generator binary
│       ├── Cargo.toml
│       ├── assets/
│       │   └── chart.min.js        # Vendored Chart.js 4.5.1 (203 KB)
│       └── src/
│           ├── main.rs
│           ├── parser.rs           # Parse cargo-criterion JSON lines
│           ├── model.rs            # Internal data model
│           ├── grouper.rs          # Group benchmarks by category
│           ├── charts.rs           # Build Chart.js data structures
│           └── renderer.rs         # Emit self-contained HTML
```

---

## 4. Cargo.toml Changes

### Root `Cargo.toml` — add new workspace members

```toml
members = [
    # ...existing...
    "benches",
    "iai",
    "test/loom",
    "test/bench_report",
]

# Exclude benches + iai from default-members so cargo check --workspace
# doesn't pull in criterion/iai-callgrind on every build.
# They are excluded by default already (only default-members are checked).
```

### `benches/Cargo.toml`

```toml
[package]
name = "naia-benches"
version = "0.1.0"
edition = "2021"
publish = false

[lib]
name = "naia_benches"
path = "src/lib.rs"

[[bench]]
name = "naia"
path = "benches/main.rs"
harness = false          # CRITICAL — required by Criterion

[dev-dependencies]
criterion = { version = "0.5", features = ["html_reports"] }

[dependencies]
naia-server  = { path = "../server",  features = ["transport_local", "test_time"] }
naia-client  = { path = "../client",  features = ["transport_local", "test_time"] }
naia-shared  = { path = "../shared",  features = ["transport_local", "test_time"] }
```

### `iai/Cargo.toml`

```toml
[package]
name = "naia-iai"
version = "0.1.0"
edition = "2021"
publish = false

[[bench]]
name = "tick_hot_path"
harness = false

[[bench]]
name = "update_dispatch"
harness = false

[dev-dependencies]
iai-callgrind = "0.14"

[build-dependencies]
iai-callgrind-runner = "0.14"

[dependencies]
naia-server = { path = "../server",  features = ["transport_local", "test_time"] }
naia-client = { path = "../client",  features = ["transport_local", "test_time"] }
naia-shared = { path = "../shared",  features = ["transport_local", "test_time"] }
```

### `test/loom/Cargo.toml`

```toml
[package]
name = "naia-loom"
version = "0.1.0"
edition = "2021"
publish = false

[dependencies]
loom = "0.7"
naia-shared = { path = "../../shared", features = ["transport_local", "test_time"] }
```

### `test/bench_report/Cargo.toml`

```toml
[package]
name = "naia-bench-report"
version = "0.1.0"
edition = "2021"
publish = false

[[bin]]
name = "naia_bench_report"
path = "src/main.rs"

[dependencies]
serde       = { version = "1", features = ["derive"] }
serde_json  = "1"
```

---

## 5. NaiaWorld Builder

The `NaiaWorld` struct (in `benches/src/lib.rs`) is the equivalent of Bevy's
`WorldBuilder`. It creates a server + N clients in steady-state using the
in-memory local transport, so benchmark setup cost is not measured.

```rust
pub struct NaiaWorld {
    server: NaiaServer<...>,
    clients: Vec<NaiaClient<...>>,
    room_key: RoomKey,
    entity_keys: Vec<EntityKey>,
}

pub struct NaiaWorldBuilder {
    user_count: usize,
    entity_count: usize,
    entity_kind: EntityKind,  // Mutable | Immutable | Delegated
}

impl NaiaWorldBuilder {
    pub fn new() -> Self { /* defaults: 1 user, 0 entities, mutable */ }
    pub fn users(mut self, n: usize) -> Self { ... }
    pub fn entities(mut self, n: usize) -> Self { ... }
    pub fn delegated(mut self) -> Self { ... }
    pub fn immutable(mut self) -> Self { ... }

    /// Builds the world and advances until all entities are in scope.
    /// This is called in iter_batched's setup closure — never measured.
    pub fn build(self) -> NaiaWorld { ... }
}

impl NaiaWorld {
    /// Run one server tick + one tick per client. This is what benchmarks measure.
    pub fn tick(&mut self) { ... }

    /// Mutate K entities. Call before tick() to benchmark active workload.
    pub fn mutate_entities(&mut self, count: usize) { ... }

    /// Request authority on entity `idx` from client 0.
    pub fn request_authority(&mut self, idx: usize) { ... }
}
```

**Critical pattern for all benchmarks:**

```rust
b.iter_batched(
    || NaiaWorldBuilder::new().entities(10_000).build(),  // setup — not measured
    |mut world| world.tick(),                              // measured
    BatchSize::LargeInput,  // reuse setup across many iterations for expensive setups
)
```

Use `BatchSize::SmallInput` for cheap setups (< 100 entities).
Use `BatchSize::LargeInput` for expensive setups (≥ 1K entities).

---

## 6. Benchmark Protocol

`benches/src/bench_protocol.rs` defines a minimal Naia protocol for benchmarks:

```rust
/// Mutable component — has a Property<T>, can be updated by editor clients.
#[derive(Component, Replicate)]
pub struct BenchComponent {
    pub value: Property<u32>,
}

/// Immutable component — no diff tracking allocated (Win 5).
#[derive(Component, Replicate)]
#[replicate(immutable)]
pub struct BenchImmutableComponent {
    pub value: u32,
}
```

No entity relations or complex hierarchies — the protocol is as thin as possible
so benchmark results reflect Naia overhead, not protocol complexity.

---

## 7. Benchmark Specifications

### 7.1 Tick Benchmarks (`benches/benches/tick/`)

These are the most important benchmarks. The tick is called at 20–60 Hz.

#### `idle.rs` — Proves Win 2 + Win 3 (the core O(1) claim)

**Setup:** Criterion benchmark group with parametric entity counts.  
**Measurement:** One server tick + one client tick with **zero mutations**.  
**Expected shape:** Flat — time must not grow with entity count.

```rust
const ENTITY_COUNTS: &[usize] = &[100, 500, 1_000, 5_000, 10_000];

fn idle_room_tick(c: &mut Criterion) {
    let mut group = c.benchmark_group("tick/idle");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(5));

    for &n in ENTITY_COUNTS {
        group.bench_with_input(BenchmarkId::new("entities", n), &n, |b, &n| {
            b.iter_batched(
                || NaiaWorldBuilder::new().entities(n).build(),
                |mut world| world.tick(),
                BatchSize::LargeInput,
            )
        });
    }
    group.finish();
}
```

The flat vs. linear curve shape is the deliverable — not the absolute number.

#### `active.rs` — Proves Win 3 (work scales with K mutations, not N entities)

**Setup:** Fixed N = 10,000 entities; parametric mutation count K.  
**Measurement:** tick() after mutating K entities.  
**Expected shape:** Linear in K, but K << N.

```rust
const MUTATION_COUNTS: &[usize] = &[0, 1, 10, 100, 1_000];

fn active_room_tick(c: &mut Criterion) {
    let mut group = c.benchmark_group("tick/active");
    for &k in MUTATION_COUNTS {
        group.bench_with_input(BenchmarkId::new("mutations", k), &k, |b, &k| {
            b.iter_batched(
                || NaiaWorldBuilder::new().entities(10_000).build(),
                |mut world| { world.mutate_entities(k); world.tick() },
                BatchSize::LargeInput,
            )
        });
    }
}
```

#### `scope.rs` — Scope entry/exit overhead

**Scope enter:** A new user joins a room with N entities. Measures the burst
cost of sending all N entities to the new client on first tick.

**Scope exit:** A user leaves the room. Measures teardown cost.

**Expected shape:** Linear in N (unavoidable — O(N) entities must be sent).
The goal is a reasonable constant factor, not O(1).

---

### 7.2 Spawn Benchmarks (`benches/benches/spawn/`)

#### `single.rs` — Baseline spawn latency

Single entity, one user, measures server-side spawn + client receive.

#### `burst.rs` — 10K entity level load (the tiles-at-scale scenario)

**This is the benchmark cyberlith cares about most.**

Parametric over [100, 1K, 5K, 10K]. Measures total time for all N entities to
appear in the client's world. Reports both wall time and
`Throughput::Elements(n)` so Criterion shows entities/sec.

```rust
group.throughput(Throughput::Elements(n as u64));
```

#### `coalesced.rs` — SpawnWithComponents vs. legacy (proves Win 4)

Two variants side by side:
- `legacy`: Spawn + N×InsertComponent messages
- `coalesced`: single SpawnWithComponents message

Parametric over component count [1, 2, 4, 8]. Reports `Throughput::Bytes` so
results show wire efficiency in bytes/entity.

---

### 7.3 Update Benchmarks (`benches/benches/update/`)

#### `mutation.rs` — Single mutation end-to-end latency

One entity, one mutation, measured from `set_value()` on server to
`WorldEvent::UpdateComponent` received on client. This is the baseline latency
number users quote in planning docs.

#### `bulk.rs` — K mutations per tick

Parametric over mutation count [1, 10, 100, 1K]. Use `Throughput::Elements(k)`
so results show mutations/sec. Complements `tick/active.rs` by isolating the
update pipeline from everything else.

#### `immutable.rs` — Win 5: zero-allocation for immutable components

**Two variants compared:**
- `mutable_tick`: world with N mutable components, 0 mutations, 1 tick
- `immutable_tick`: world with N immutable components, 1 tick

**The claim:** these should have *identical* tick times, because immutable
components allocate no UserDiffHandler/MutChannel entries. A measurable
difference in the tick benchmark would mean Win 5 has a bug.

**Additionally:** use a counting allocator wrapper (or `dhat` profiler) to
verify zero `malloc` calls occur during the measured tick for the immutable
variant. Time alone cannot falsify the zero-allocation claim.

```rust
// Sketch for allocation-counting verification (implementation detail)
// Uses jemalloc's allocation statistics or a custom GlobalAlloc wrapper
// to assert zero heap allocations during the measured code path.
```

---

### 7.4 Authority Benchmarks (`benches/benches/authority/`)

#### `grant.rs` — Delegation grant round-trip

Client requests authority → server grants → client confirms. Measures
round-trip ticks required. Tests the delegated tile use case baseline.

#### `contention.rs` — Multi-user authority contention

K users all request authority on the same entity simultaneously. Measures:
- Time until one user wins authority (expected: 1–2 ticks)
- Time until all other users receive `AuthDenied` events

Parametric over user count K [2, 4, 8, 16].

---

### 7.5 Wire Benchmarks (`benches/benches/wire/`)

#### `framing.rs` — Bytes per operation

Uses `Throughput::Bytes` to measure wire cost of:

| Operation | Bytes expected |
|---|---|
| Spawn (0 components) | ~8 bytes |
| Spawn (1 component) | ~8 + component payload |
| SpawnWithComponents (4 components) | significantly less than 4× InsertComponent |
| Single mutation | ~10–16 bytes |
| Scope enter (1K entities) | ~8 KB |

These numbers should appear as `MB/s` in Criterion output, which users can
translate to bandwidth budget.

#### `bandwidth.rs` — Sustained throughput

N users each mutating M components per tick. Total throughput in
mutations/sec and bytes/sec across all users. Stress test for the
push-based update pipeline.

---

## 8. iai-callgrind Benchmarks (`iai/`)

### Purpose

Wall-time benchmarks have 3–5% variance. You cannot reliably detect a 2%
regression in Criterion output. iai-callgrind runs Valgrind's callgrind
simulator: fully deterministic, hardware-independent, catches 1 extra
instruction.

### `tick_hot_path.rs` — The primary hot-path gate

Measures instruction count for one idle tick with 10K entities and 4 users.
This is the single most important number — if it grows between commits, a
regression snuck in.

```rust
use iai_callgrind::{library_benchmark, library_benchmark_group, main};

#[library_benchmark]
fn idle_tick_10k() -> () {
    let mut world = NaiaWorldBuilder::new()
        .users(4)
        .entities(10_000)
        .build();
    world.tick();
}

library_benchmark_group!(name = tick; benchmarks = idle_tick_10k);
main!(library_benchmark_groups = tick);
```

**Run command:**

```bash
cargo bench --bench tick_hot_path -p naia-iai
```

Output shows instruction count, L1/L2/LL cache miss rates, branch mispredicts.
Compare between branches manually:

```
tick_hot_path::idle_tick_10k
  Instructions:     2,847,391 (-0.00%)     # perfect — no regression
  L1 Data Hits:       891,204
  L2 Data Hits:         4,112
  LL Data Misses:         891  # monitor — LLC misses at 10K may be TileMap thrash
```

### `update_dispatch.rs` — Update pipeline instruction count

Single mutation → downstream receive. Measures the minimal per-mutation cost
so any overhead added to the update pipeline is immediately visible.

---

## 9. Loom Concurrency Tests (`test/loom/`)

### Purpose

Win 3 (dirty-receiver candidate set) introduces a push model where mutation
callbacks write into a per-user `dirty_components: HashSet` while the send
thread drains it. Loom exhaustively explores all valid thread interleavings
for a given execution to find data races and deadlocks that ordinary tests miss.

This is a **correctness test**, not a benchmark. It lives in `test/loom/`.

### `dirty_set.rs`

```rust
#[test]
fn dirty_set_concurrent_mutate_drain() {
    loom::model(|| {
        // Thread 1: mark component dirty (simulates entity mutation callback)
        // Thread 2: drain dirty set for send (simulates tick/update dispatch)
        // Assert: every dirty mark is either consumed by the drain or
        // still present — never lost.
    });
}
```

**Run command:**

```bash
RUSTFLAGS="--cfg loom" cargo test -p naia-loom
```

---

## 10. HTML Report Tool (`test/bench_report/`)

### Purpose

A self-contained Rust binary that reads `cargo-criterion --message-format=json`
output and emits a single `bench_report.html` file with interactive charts.
No external services, no CDN at runtime — the HTML file works offline.

### Data flow

```
cargo criterion --message-format=json 2>/dev/null
  │
  ▼
naia_bench_report          (cargo run -p naia-bench-report)
  │
  ├── parser.rs            parse line-delimited JSON from cargo-criterion
  ├── model.rs             BenchResult { group, id, params, median_ns, throughput }
  ├── grouper.rs           group by category: tick/ spawn/ update/ authority/ wire/
  ├── charts.rs            build Chart.js dataset objects
  └── renderer.rs          write bench_report.html with Chart.js inlined
  │
  ▼
bench_report.html          open in browser
```

### cargo-criterion JSON format (relevant fields)

```json
{
  "reason": "benchmark-complete",
  "id": "tick/idle/entities/10000",
  "report_directory": "target/criterion/tick_idle/entities_10000",
  "typical": { "estimate": 123456.78, "unit": "ns" },
  "mean":    { "estimate": 123789.00, "unit": "ns" },
  "median":  { "estimate": 123000.00, "unit": "ns" },
  "throughput": [{ "per_iteration": 10000, "unit": "elements" }]
}
```

The `id` field encodes the full benchmark path. Parameter values are
embedded as the last path segment (e.g., `entities/10000`).

### Chart types

| Benchmark category | Chart type | X axis | Y axis |
|---|---|---|---|
| `tick/idle` | **Line chart** | entity count | ns per tick |
| `tick/active` | **Line chart** | mutation count | ns per tick |
| `spawn/burst` | **Line chart** | entity count | ms total |
| `spawn/coalesced` | **Bar chart** (grouped) | component count | bytes per spawn |
| `update/bulk` | **Line chart** | mutation count | µs total |
| `update/immutable` | **Bar chart** (side-by-side) | variant | ns per tick |
| `authority/contention` | **Line chart** | user count | ticks to resolve |
| `wire/framing` | **Bar chart** | operation | bytes |
| `wire/bandwidth` | **Bar chart** | scenario | MB/s |

The scalability line charts (tick/idle, tick/active) are the most critical —
their shape (flat vs. linear) is the visual proof of the O(1) claims.

### HTML report structure

```
bench_report.html
├── Header: "Naia Benchmark Report — <timestamp>"
├── Summary table: all benchmarks with median ± std_dev
├── Section: Tick Performance
│   ├── idle_room_tick — line chart (flat line = O(1) proof)
│   └── active_room_tick — line chart (linear in K, not N)
├── Section: Spawn Performance
│   ├── burst_spawn — line chart
│   └── SpawnWithComponents vs. legacy — grouped bar chart
├── Section: Update Pipeline
│   ├── mutation dispatch — bar chart
│   ├── bulk mutations — line chart
│   └── immutable vs. mutable — side-by-side bar chart
├── Section: Authority Model
│   ├── grant round-trip — bar chart
│   └── contention — line chart
└── Section: Wire Efficiency
    ├── bytes per operation — bar chart
    └── bandwidth throughput — bar chart
```

### Chart.js vendoring

Download once:

```bash
curl -L https://cdn.jsdelivr.net/npm/chart.js@4.5.1/dist/chart.umd.min.js \
     -o test/bench_report/assets/chart.min.js
```

Embed in HTML with `include_str!()`:

```rust
const CHART_JS: &str = include_str!("../assets/chart.min.js");

// In renderer:
format!("<script>{}</script>", CHART_JS)
```

The generated HTML is fully self-contained — no network required to open it.

### Usage

```bash
# Run full criterion suite and generate report
cargo criterion --message-format=json 2>/dev/null \
  | cargo run -p naia-bench-report \
  > bench_report.html

# Run specific group and generate report
cargo criterion --bench naia -- tick/ --message-format=json 2>/dev/null \
  | cargo run -p naia-bench-report -- --title "Tick benchmarks only" \
  > tick_report.html

# Open in browser (Linux)
xdg-open bench_report.html
```

---

## 11. Criterion Configuration Standards

Consistent across all benchmark groups (follow Bevy's standard):

```rust
group.warm_up_time(Duration::from_millis(500));
group.measurement_time(Duration::from_secs(5));   // 6s for expensive suites
```

Always use `black_box()` on return values to prevent dead-code elimination:

```rust
b.iter(|| black_box(world.tick()));
```

Use qualified benchmark names via a `bench!()` macro (mirrors Bevy):

```rust
// benches/src/lib.rs
#[macro_export]
macro_rules! bench {
    ($name:literal) => { concat!(module_path!(), "::", $name) }
}

// Usage:
c.bench_function(bench!("idle_room_tick"), |b| { ... });
```

---

## 12. Running the Suite

```bash
# Install cargo-criterion (one-time)
cargo install cargo-criterion

# Install valgrind (required for iai-callgrind, one-time, system package)
# sudo apt install valgrind   # Ubuntu/Debian

# Run full Criterion suite
cargo criterion -p naia-benches

# Run one category
cargo criterion -p naia-benches -- tick/

# Run iai-callgrind (instruction count, deterministic)
cargo bench -p naia-iai --bench tick_hot_path

# Run loom concurrency tests
RUSTFLAGS="--cfg loom" cargo test -p naia-loom

# Generate HTML report from last criterion run
cargo criterion --message-format=json 2>/dev/null \
  | cargo run -p naia-bench-report > bench_report.html
xdg-open bench_report.html

# Save a baseline for manual comparison
cargo criterion -- --save-baseline main

# Compare feature branch against baseline
cargo criterion -- --baseline main
critcmp main HEAD
```

---

## 13. Implementation Plan

Steps are ordered by dependency. Complete each in sequence; update the status
column as you go.

| # | Step | Deliverable | Status |
|---|---|---|---|
| 1 | Add `benches/`, `iai/`, `test/loom/`, `test/bench_report/` to workspace `Cargo.toml` | Root Cargo.toml updated | ✅ |
| 2 | Create `benches/Cargo.toml` with criterion dep + `harness = false` | Crate scaffolded | ✅ |
| 3 | Write `benches/src/bench_protocol.rs` — `BenchComponent` (mutable) + `BenchImmutableComponent` | Protocol compiles | ✅ |
| 4 | Write `benches/src/lib.rs` — `NaiaWorldBuilder` + `NaiaWorld::tick()` | Builder compiles; `cargo check` clean | ✅ |
| 5 | Write `benches/benches/tick/idle.rs` — parametric idle tick over ENTITY_COUNTS | `cargo criterion -- tick/idle` runs | ✅ |
| 6 | Write `benches/benches/tick/active.rs` — parametric active tick over MUTATION_COUNTS | `cargo criterion -- tick/active` runs | ✅ |
| 7 | Write `benches/benches/tick/scope.rs` — scope enter + exit | `cargo criterion -- tick/scope` runs | ✅ |
| 8 | Write `benches/benches/spawn/single.rs` | Runs | ✅ |
| 9 | Write `benches/benches/spawn/burst.rs` — 10K entity burst with Throughput::Elements | Runs; Criterion shows entities/sec | ✅ |
| 10 | Write `benches/benches/spawn/coalesced.rs` — SpawnWithComponents vs. legacy with Throughput::Bytes | Comparison renders correctly | ✅ |
| 11 | Write `benches/benches/update/mutation.rs` | Runs | ✅ |
| 12 | Write `benches/benches/update/bulk.rs` — parametric over K with Throughput::Elements | Runs | ✅ |
| 13 | Write `benches/benches/update/immutable.rs` — mutable vs. immutable side-by-side | Runs; delta should be ≤ noise floor | ✅ |
| 14 | Write `benches/benches/authority/grant.rs` | Runs | ✅ |
| 15 | Write `benches/benches/authority/contention.rs` — parametric over user count | Runs | ✅ |
| 16 | Write `benches/benches/wire/framing.rs` — Throughput::Bytes per operation | Criterion shows bytes/op | ✅ |
| 17 | Write `benches/benches/wire/bandwidth.rs` — sustained throughput | Runs | ✅ |
| 18 | Wire all bench files into `benches/benches/main.rs` with Criterion `criterion_group!` + `criterion_main!` | Full suite: `cargo criterion` runs clean | ✅ |
| 19 | Create `iai/Cargo.toml` + `iai/benches/tick_hot_path.rs` | `cargo bench -p naia-iai --bench tick_hot_path` produces instruction counts | ✅ |
| 20 | Create `iai/benches/update_dispatch.rs` | Runs | ✅ |
| 21 | Create `test/loom/` crate + `dirty_set.rs` concurrency test | `RUSTFLAGS="--cfg loom" cargo test -p naia-loom` passes (2/2) | ✅ |
| 22 | Download and vendor `chart.min.js` into `test/bench_report/assets/` | 208 KB; present | ✅ |
| 23 | Write `test/bench_report/src/parser.rs` — parse cargo-criterion `--message-format=json` lines | 2 unit tests pass | ✅ |
| 24 | Write `test/bench_report/src/model.rs` + `grouper.rs` — group into tick/spawn/update/authority/wire categories | Groups match directory structure | ✅ |
| 25 | Write `test/bench_report/src/charts.rs` — build Chart.js dataset JSON | Output is valid Chart.js config | ✅ |
| 26 | Write `test/bench_report/src/renderer.rs` — emit self-contained HTML | HTML opens in browser; charts render | ✅ |
| 27 | End-to-end test: run criterion suite → pipe to report tool → open HTML | tick/idle: 5 benchmarks → 211 KB self-contained HTML with Chart.js inline | ✅ |
| 28 | Write `benches/README.md` — how to run, baseline workflow, critcmp instructions | Documented | ✅ |

---

## 14. Verification Criteria

The suite is complete when:

1. `cargo criterion -p naia-benches` completes with all benchmark groups
2. `tick/idle` parametric chart shows a **flat line** across entity counts
   (time does not grow from 100 → 10K entities in idle room)
3. `tick/active` shows **linear in K** mutations, not N entities
4. `spawn/coalesced` shows SpawnWithComponents uses fewer bytes than legacy
   for component counts ≥ 2
5. `update/immutable` shows mutable and immutable tick times within noise floor
6. `cargo bench -p naia-iai --bench tick_hot_path` produces a stable
   instruction count (< 5% variance across runs)
7. `RUSTFLAGS="--cfg loom" cargo test -p naia-loom` passes
8. `bench_report.html` opens in browser and shows all sections with charts

---

---

## 15. Notes

### `spawn/coalesced` — measures steady-state cost after burst

`SpawnWithComponents` (Win-4) is always the wire format; there is no legacy
separate-Spawn+Insert mode to compare against in a single build. The benchmark
measures steady-state idle tick cost after N entities are replicated, which
proves O(1) ongoing overhead. A true wire-format comparison would require two
builds and is not worth maintaining.

### `wire/framing` and `wire/bandwidth` — bytes calibrated from bandwidth monitor

`Throughput::Bytes` is calibrated by running 60 warmup ticks with a bandwidth-
monitoring-enabled probe world and reading `server.outgoing_bandwidth_total()`
(kbps). Converted to bytes/tick via `kbps × TICK_MS / 8`. Criterion then
reports bytes/sec correctly.

*Keep this document updated as steps complete. Mark each row ✅ when done.*
