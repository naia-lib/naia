# Crucible — Shared Benchmark Infrastructure Plan
## One-stop benchmark orchestrator for naia and cyberlith

- **Date:** 2026-04-27
- **Status:** Approved for implementation
- **Owner:** Connor
- **Repos affected:** slag, naia, cyberlith

---

## Context

naia has criterion microbenchmarks (`naia-benches`) and a bespoke report/assert tool (`naia-bench-report`) that are naia-specific and not reusable. cyberlith needs a full-stack benchmark suite (BM-001–BM-007) that runs real Naia + Rapier + game logic. Both repos need: baseline management, regression gating, HTML/markdown reporting, and a CI-friendly assert gate. Currently none of this is shared or turnkey — the goal is to fix that with two new slag crates and a consistent pattern across both repos.

---

## Architecture overview

```
┌─────────────────────────────────────────────────────┐
│                     crucible CLI                    │
│  run · assert · compare · report · baseline         │
│  (slag/crates/crucible — binary)                    │
└────────────────┬────────────────────────────────────┘
                 │ depends on
┌────────────────▼────────────────────────────────────┐
│                    bench_core                        │
│  BenchResult · Baseline · AssertOutcome             │
│  run_regression · JsonSink · MarkdownSink · HtmlSink│
│  (slag/crates/bench_core — lib)                     │
└──────────────┬──────────────────────┬───────────────┘
               │                      │
┌──────────────▼──────────┐  ┌────────▼───────────────┐
│     naia/test/bench/    │  │  cyberlith/test/bench/  │
│     package: naia-bench │  │  package: cyberlith_bench│
│  naia domain win checks │  │  BM-001–BM-007 scenarios│
│  called by crucible as  │  │  game server + clients  │
│  post_assert handler    │  │  outputs bench_core JSON│
└─────────────────────────┘  └────────────────────────┘
```

---

## bench_core — shared data, logic, and output layer

### What it is

Pure lib crate in `slag/crates/bench_core/`. No game knowledge, no clap, no criterion, no async. Only deps: `serde`, `serde_json`.

### File layout

```
slag/crates/bench_core/
├── Cargo.toml
└── src/
    ├── lib.rs        re-exports
    ├── result.rs     BenchResult, metadata extension point
    ├── baseline.rs   Baseline load/save/compare
    ├── outcome.rs    AssertOutcome, ComparisonResult, Verdict
    ├── checks.rs     run_regression() — the shared regression logic
    └── sink.rs       ReportSink trait + JsonSink, MarkdownSink, HtmlSink
```

### Core types

`result.rs`:
```rust
/// A single benchmark measurement. `metadata` holds any repo-specific metrics
/// (P95 tick, wire bytes, FPS, etc.) as opaque JSON values so the core type
/// never needs to change as new scenarios are added.
pub struct BenchResult {
    pub id: String,
    pub median_ns: f64,
    pub std_dev_ns: f64,
    pub metadata: BTreeMap<String, serde_json::Value>,
}
```

`baseline.rs`:
```rust
pub struct Baseline {
    pub name: String,
    pub results: Vec<BenchResult>,
}

impl Baseline {
    pub fn load(path: &Path) -> Result<Self, ...>
    pub fn save(&self, path: &Path) -> Result<(), ...>
    pub fn compare(&self, new: &[BenchResult], threshold: f64) -> Vec<ComparisonResult>
}
```

`outcome.rs`:
```rust
pub struct AssertOutcome { pub pass: usize, pub fail: usize, pub skip: usize }
impl AssertOutcome {
    pub fn failed(&self) -> bool { self.fail > 0 }
    pub fn summary(&self) -> String { ... }
}

pub struct ComparisonResult {
    pub id: String,
    pub baseline_ns: f64,
    pub new_ns: f64,
    pub ratio: f64,   // new / baseline
    pub verdict: Verdict,
}

pub enum Verdict { Pass, Fail { ratio: f64, threshold: f64 }, Skip { reason: String } }
```

`checks.rs`:
```rust
/// Generic regression sweep: for each result, find its baseline entry,
/// compute ratio, flag FAIL if ratio > threshold.
/// This is the shared logic used by both crucible assert and naia-bench.
pub fn run_regression(
    results: &[BenchResult],
    baseline: &Baseline,
    threshold: f64,
) -> AssertOutcome
```

`sink.rs`:
```rust
pub trait ReportSink {
    fn emit(&self, results: &[BenchResult], outcome: &AssertOutcome);
}

pub struct JsonSink    { pub path: PathBuf }
pub struct MarkdownSink;                      // writes to stdout
pub struct HtmlSink   { pub title: String, pub path: PathBuf }
```

### What bench_core explicitly does NOT contain

- Criterion JSON parsing (driver-specific, lives in crucible)
- Any game or Naia domain knowledge
- CLI argument parsing
- Async code
- Any repo-specific win checks

---

## crucible — benchmark orchestrator CLI

### What it is

Binary crate in `slag/crates/crucible/`. Deps: `bench_core`, `clap` (derive), `serde`, `serde_json`, `toml`.

### File layout

```
slag/crates/crucible/
├── Cargo.toml
└── src/
    ├── main.rs       clap entry point
    ├── config.rs     CrucibleConfig (deserialised from crucible.toml)
    ├── driver.rs     DriverKind { Criterion, CargoBin } + run logic + auto-install
    ├── source.rs     CriterionJsonSource (criterion --message-format=json → Vec<BenchResult>)
    └── commands/
        ├── run.rs
        ├── assert.rs
        ├── compare.rs
        ├── report.rs
        └── baseline.rs
```

### crucible.toml — per-repo config file (place at repo root)

```toml
# naia/crucible.toml
driver        = "criterion"       # uses cargo-criterion
package       = "naia-benches"    # bench package
bench         = "naia"            # --bench <name>
results_dir   = "target/bench"
baseline_name = "perf_v0"
post_assert   = "cargo run -p naia-bench -- --assert-wins"
```

```toml
# cyberlith/crucible.toml
driver        = "cargo_bin"       # runs a cargo binary directly
package       = "cyberlith_bench"
args          = ["--scenario", "all"]
results_dir   = "target/bench"
baseline_name = "perf_v0"
```

### CLI subcommands

**`crucible run`**
```sh
crucible run [--scenario <id>] [--assert]
```
Steps:
1. Reads `crucible.toml` from cwd (error if not found)
2. If `driver = "criterion"`: checks whether `cargo-criterion` is installed; if not, runs `cargo install cargo-criterion` automatically
3. Executes the configured driver, captures output
4. Parses output into `Vec<BenchResult>` via `CriterionJsonSource` (criterion) or by deserialising bench_core JSON (cargo_bin)
5. Writes results to `<results_dir>/latest.json`
6. If `post_assert` is configured, invokes it as a shell command, passing the results path as `--input`
7. If `--assert` flag: runs `crucible assert <results_dir>/latest.json`, exits non-zero on failure

**`crucible assert`**
```sh
crucible assert [results.json]       # defaults to <results_dir>/latest.json
                [--baseline <path>]  # defaults to <results_dir>/<baseline_name>.json
                [--threshold <f64>]  # defaults to 1.20
```
- Loads results and baseline
- Calls `bench_core::run_regression()`
- Prints per-cell pass/fail to stdout
- Exits 0 if all pass, 1 if any fail

**`crucible compare`**
```sh
crucible compare <a.json> <b.json>
```
- Loads two result files
- Prints a markdown diff table: id | a_ns | b_ns | ratio | verdict
- Always exits 0 (informational only)

**`crucible report`**
```sh
crucible report [results.json]
                [--format html|markdown]   # default: html
                [--out <path>]             # default: <results_dir>/report.html
```
- Loads results
- Calls `bench_core::HtmlSink` or `MarkdownSink`
- Writes to output path

**`crucible baseline`**
```sh
crucible baseline save [results.json] --name <label>
crucible baseline list
```
- `save`: copies results file to `<results_dir>/<label>.json`
- `list`: prints all JSON files in `results_dir`

### Auto-install behaviour

- Criterion: `cargo install cargo-criterion --quiet` if `cargo criterion` not found in PATH
- Future drivers: same pattern — document what will be installed before doing so

### CriterionJsonSource — in crucible, not bench_core

criterion's `--message-format=json` wire format is a naia-specific concern (naia is the only criterion user). This parser lives in crucible because crucible is the orchestrator; it is NOT in bench_core because bench_core has no input-parsing responsibilities.

```rust
// crucible/src/source.rs
pub struct CriterionJsonSource;

impl CriterionJsonSource {
    /// Reads criterion --message-format=json lines from a Read source,
    /// filters for reason=="benchmark-complete", returns Vec<BenchResult>.
    pub fn parse<R: Read>(reader: R) -> Vec<BenchResult>
}
```

---

## naia-bench — naia domain win-check layer

### What it is

Renamed and restructured from current `naia/test/bench_report/`. New location: `naia/test/bench/`. Package name: `naia-bench`.

Dep on `bench_core`. Does NOT dep on criterion (crucible handles criterion execution and parsing).

### What changes from current bench_report

- Drops its hand-rolled `BenchResult`, `AssertOutcome`, `CriterionSource` — replaced by `bench_core` types
- Drops HTML report generation — replaced by `crucible report`
- Drops baseline regression sweep — replaced by `bench_core::run_regression()` called from `crucible assert`
- **Keeps**: all naia-specific domain checks — Win-2 (idle O(1) flatness), Win-3 (dirty-receiver push model), Win-4 (coalesced spawn), Win-5 (immutable cost), phase thresholds, halo budget/client-keepup checks
- These checks encode naia domain knowledge and must never move to slag

### Interface

```sh
cargo run -p naia-bench -- --assert-wins --input <results.json>
```

Exits 0 if all naia win checks pass, 1 otherwise. Invoked by crucible as `post_assert`.

### File layout

```
naia/test/bench/
├── Cargo.toml        package = "naia-bench"
└── src/
    ├── main.rs
    └── wins.rs       Win-2–5, phase thresholds, halo checks
```

---

## cyberlith_bench — full-stack game benchmark driver

### What it is

New crate at `cyberlith/test/bench/`. Package name: `cyberlith_bench`. Deps: `bench_core`, naia, Rapier, game logic.

### What it does

Implements BM-001–BM-007 benchmark scenarios. Each scenario:
1. Spawns a game server + N simulated clients using naia's local (in-process) transport
2. Runs the game loop for a configured duration
3. Collects metrics into `BenchResult.metadata`
4. Outputs bench_core JSON to stdout

### Metadata keys used by cyberlith_bench

```
p50_tick_ns, p95_tick_ns, p99_tick_ns, max_tick_ns
wire_bytes_out_per_client_sec, wire_bytes_in_per_server_sec
memory_bytes_per_session
match_completion_rate
crash_count, reconnect_count
browser_fps (null until BM-004)
```

### Benchmark ladder

| ID | Name | Scenario | Key metric | Priority |
|---|---|---|---|---|
| BM-001 | Shell 2v2 baseline | 2 players, 2 shells, 0 daemons, real Naia + Rapier + game logic | P95 server tick ≤ 40 ms | **Critical — run first** |
| BM-002 | Daemon worst-case 2v2 | 2 players, each with 1 daemon summoned, all active | P95 tick with daemons | **Critical before daemon design lock** |
| BM-003 | Event room stress | 4v4, all players, all daemons | P95 tick at event scale | High |
| BM-004 | Browser client | WASM client, mid-range device target | browser FPS ≥ 30, input latency ≤ 100 ms | **Critical — unmeasured** |
| BM-005 | Wire bytes + WAN | Measure outgoing bytes/sec per client under BM-001 conditions | bytes/sec within budget | High |
| BM-006 | Session/persistence soak | 60-minute session, account operations, reconnects | no memory leak, reconnect success rate | Medium |
| BM-007 | Scheduled playtest load | Real players, scheduled event | match completion rate, crash rate | Medium — needs real infra |

### File layout

```
cyberlith/test/bench/
├── Cargo.toml        package = "cyberlith_bench"
└── src/
    ├── main.rs       clap: --scenario <id>, --duration <secs>, --out <path>
    ├── harness.rs    server + client spawner using local transport
    ├── collector.rs  metrics collection into BenchResult metadata
    └── scenarios/
        ├── bm001.rs
        ├── bm002.rs
        ├── bm003.rs
        ├── bm004.rs  (stub — browser, manual measurement)
        ├── bm005.rs
        ├── bm006.rs
        └── bm007.rs  (stub — requires real infra)
```

---

## CI flows

```sh
# naia — full gate (one command)
crucible run --assert
# expands to:
#   1. installs cargo-criterion if missing
#   2. cargo criterion --message-format=json -p naia-benches --bench naia
#   3. parses JSON → Vec<BenchResult>
#   4. writes target/bench/latest.json
#   5. cargo run -p naia-bench -- --assert-wins --input target/bench/latest.json  (Win-2–5 + phase checks)
#   6. crucible assert target/bench/latest.json  (generic regression vs perf_v0)
#   7. exits 0 only if both pass

# cyberlith — full gate per scenario
crucible run --scenario bm001 --assert
# expands to:
#   1. cargo run -p cyberlith_bench -- --scenario bm001 > target/bench/latest.json
#   2. crucible assert target/bench/latest.json

# one-off reporting
crucible report                          # HTML report of latest run
crucible compare target/bench/perf_v0.json target/bench/latest.json
crucible baseline save --name perf_v0   # promote latest to baseline
```

---

## Implementation order

1. ✅ **slag: `bench_core`** — implement all types, `run_regression`, sinks. No input parsing. Pure lib. Should be <300 lines total.
2. ✅ **slag: `crucible` skeleton** — `run` + `assert` subcommands only; `config.rs`, `driver.rs`, `source.rs`, criterion auto-install. Validate with naia criterion suite.
3. ✅ **naia: `crucible.toml`** — add to repo root, point at existing naia-benches + naia-bench-report as post_assert.
4. ✅ **naia: migrate `test/bench_report/` → `test/bench/`** — renamed, swapped internal types to bench_core, dropped criterion parsing and baseline regression (now owned by crucible), kept all win checks. Also fixed `slag_session_server` scope_checks → scope_checks_pending.
5. ⏳ **naia CI validation** — `crucible baseline save --from-criterion` migrated perf_v0 (31 entries). Structural validation: `naia-bench --assert-wins --input <results.json>` pipeline verified. Full `crucible run --assert` (39/0/0) requires 45-min criterion run — pending.
6. ✅ **slag: `crucible compare` + `report` + `baseline`** — all subcommands implemented in step 2.
7. ✅ **cyberlith: `test/bench/` skeleton** — BM-001 implemented (P50=83µs, P95=1.58ms, 500-tick run). Stubs bm002–bm007 in place. harness.rs (BenchHarness) + collector.rs (TickCollector) extracted; bm001.rs refactored to ~30 lines.
8. ✅ **cyberlith: `crucible.toml`** — added to repo root.
9. ✅ **cyberlith CI validation** — `crucible run --scenario bm001 --assert` green. baseline perf_v0 saved. 1/0/0 regression gate.
10. ⏳ **cyberlith: BM-002–BM-006** — implement iteratively as game features stabilise.
11. ⏳ **cyberlith: BM-007** — deferred until real infra available.

---

## Explicit boundary table

| Concern | Lives in | Reason |
|---|---|---|
| BenchResult, Baseline, AssertOutcome types | bench_core | shared across all consumers |
| Regression sweep logic | bench_core | single implementation, used by crucible + naia-bench |
| HTML / Markdown / JSON report sinks | bench_core | output is generic, not driver-specific |
| CriterionJsonSource | crucible | criterion is naia-specific; only crucible needs to parse it |
| Auto-install cargo-criterion | crucible | tool management is an orchestrator concern |
| crucible.toml parsing | crucible | config is orchestrator-level |
| Win-2–5, phase thresholds, halo checks | naia-bench | encode naia domain knowledge; must never move to slag |
| BM-001–BM-007 scenario implementations | cyberlith_bench | require real game code; can't live in slag or naia |
| Metadata key definitions for game metrics | cyberlith_bench | game-specific; bench_core treats metadata as opaque Value |
| Browser FPS measurement | cyberlith_bench / manual | requires real browser; BM-004 starts as manual |

---

## Open questions

- [ ] Should `crucible` be installed as a system binary (`cargo install`) or always run via `cargo run -p crucible`?
- [ ] Baseline storage: flat files in `target/bench/` (gitignored) or committed JSON files? (Current naia pattern uses committed `perf_v0` files in `target/criterion/`; worth aligning.)
- [ ] Should `crucible run` write structured logs for the CI job artifact, or is stdout + exit code sufficient?
- [ ] BM-004 (browser FPS): manual measurement or automated via headless browser? Defer decision until WASM client is stable.
- [ ] BM-007 (scheduled playtest): needs real infra provisioning. Out of scope until playtest stage.
- [ ] `naia-bench` post_assert interface: pass results path as `--input`? Or pipe bench_core JSON via stdin? Decide before implementation.
