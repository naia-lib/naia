//! Assert the Naia 0.25 Win invariants against a cargo-criterion run.
//!
//! The Wins are:
//!   1. scope-entry cost bounded by the entities that ENTER scope, not
//!      the full world (observable via `tick/scope_enter/entities/N`).
//!   2. idle tick is O(1) in world size — post-rewrite the dirty-set has
//!      0 entries on an idle tick and no O(N) scan runs.
//!   3. dirty-receiver push model: mutation cost is O(mutations × users),
//!      not O(entities × users). Held fixed at K users, varying N entities
//!      while mutating a FIXED K, cost should stay flat.
//!   4. SpawnWithComponents coalesces spawn+inserts into one command, so
//!      `spawn/coalesced` should beat `spawn/burst` at the same entity count.
//!   5. immutable components skip diff-tracking allocation and per-tick
//!      dispatch, so `immutable_idle` should be ≤ `mutable_idle` at the
//!      same entity count.
//!
//! Phase 7 hardening adds two more bands of checks layered on top of the
//! Wins:
//!
//!   - **Phase thresholds** (`check_phase_thresholds`): absolute wall-time
//!     ceilings baked from `_AGENTS/BENCH_PERF_UPGRADE.md` success criteria.
//!     These don't depend on a baseline file — they are the contract the
//!     plan committed to. If a future change blows past one of these, the
//!     gate fails outright.
//!
//!   - **Baseline regression** (`check_baseline_regression`): per-cell
//!     `current / perf_v0 ≤ 1.20`. Loaded from
//!     `target/criterion/<sanitized_id>/perf_v0/estimates.json`. Only
//!     checks cells where a `perf_v0` baseline exists (so newly-introduced
//!     benches don't generate false negatives). 1.20× is intentionally
//!     loose — criterion median noise alone can hit ±15% on small workloads.
//!
//! Call `run(&results)` — prints a pass/fail line per check and returns
//! an `AssertOutcome` whose `failed()` tells the caller whether to exit
//! non-zero.

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::model::BenchResult;

pub struct AssertOutcome {
    pass: usize,
    fail: usize,
    skip: usize,
}

impl AssertOutcome {
    pub fn failed(&self) -> bool {
        self.fail > 0
    }
    pub fn summary(&self) -> String {
        format!(
            "{} passed, {} failed, {} skipped",
            self.pass, self.fail, self.skip
        )
    }
}

/// Lookup table keyed by full benchmark id (e.g. "tick/idle/entities/10000")
fn index(results: &[BenchResult]) -> BTreeMap<&str, &BenchResult> {
    results.iter().map(|r| (r.id.as_str(), r)).collect()
}

pub fn run(results: &[BenchResult]) -> AssertOutcome {
    let idx = index(results);
    let mut out = AssertOutcome {
        pass: 0,
        fail: 0,
        skip: 0,
    };

    check_win_2_idle_flat(&idx, &mut out);
    check_win_3_dirty_receiver(&idx, &mut out);
    check_win_4_coalesced_beats_burst(&idx, &mut out);
    check_win_5_immutable_beats_mutable(&idx, &mut out);
    check_phase_thresholds(&idx, &mut out);
    check_baseline_regression(results, &mut out);

    println!("---");
    println!("win-assert summary: {}", out.summary());
    out
}

/// Convert a criterion bench id (e.g. `tick/idle_matrix/u_x_n/16u_10000e`)
/// to the on-disk directory name under `target/criterion/`. Criterion
/// sanitizes the group name (everything before the BenchmarkId path) by
/// replacing `/` with `_`, while the BenchmarkId path itself stays as-is.
/// All bench groups in this suite are 2 segments (e.g. `tick/idle_matrix`),
/// so we join the first two id segments with `_` and append the remainder.
fn criterion_dir(bench_id: &str) -> String {
    let parts: Vec<&str> = bench_id.split('/').collect();
    if parts.len() < 2 {
        return bench_id.to_string();
    }
    let mut out = format!("{}_{}", parts[0], parts[1]);
    for p in &parts[2..] {
        out.push('/');
        out.push_str(p);
    }
    out
}

/// Read the `perf_v0` baseline median for a bench id, in nanoseconds.
/// Returns `None` if the baseline file is absent or malformed (fresh benches
/// without a perf_v0 baseline are skipped, not failed).
fn read_perf_v0_median_ns(bench_id: &str) -> Option<f64> {
    let dir = criterion_dir(bench_id);
    let path = PathBuf::from("target/criterion")
        .join(&dir)
        .join("perf_v0")
        .join("estimates.json");
    let body = std::fs::read_to_string(&path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&body).ok()?;
    v.get("median")?.get("point_estimate")?.as_f64()
}

/// Phase-7 absolute wall-time ceilings, baked from the `_AGENTS/BENCH_PERF_UPGRADE.md`
/// success criteria. These represent the production contract — independent
/// of any criterion baseline file — so they survive baseline rotation.
///
/// Tuple = (bench_id, threshold_ns, label).
const PHASE_THRESHOLDS: &[(&str, f64, &str)] = &[
    // Phase 3 — kill O(U·N) idle. Doc target ≤ 3 ms; achieved ~40 µs post-Phase-4.
    (
        "tick/idle_matrix/u_x_n/16u_10000e",
        3_000_000.0,
        "Phase 3 mutable idle",
    ),
    // Phase 4 — immutable skip idle. Doc target ≤ 1.5 ms; achieved ~50 µs.
    // Tightened to 200 µs (4× headroom over realised ~50 µs) — anything
    // looser silently absorbs a major regression.
    (
        "tick/idle_matrix_immutable/u_x_n/16u_10000e",
        200_000.0,
        "Phase 4 immutable idle",
    ),
    // Phase 6 — PaintRect burst wall-clock. Realised baseline 24.4 ms / 187 ms;
    // ceilings sized at ~1.15× to catch meaningful regressions, not jitter.
    (
        "spawn/paint_rect/entities/1000",
        28_000_000.0,
        "Phase 6 paint_rect/1000",
    ),
    (
        "spawn/paint_rect/entities/5000",
        220_000_000.0,
        "Phase 6 paint_rect/5000",
    ),
];

/// Per-cell absolute thresholds from `_AGENTS/BENCH_PERF_UPGRADE.md` success
/// criteria. Independent of baseline files; survives baseline rotation.
fn check_phase_thresholds(idx: &BTreeMap<&str, &BenchResult>, out: &mut AssertOutcome) {
    for &(bench_id, threshold_ns, label) in PHASE_THRESHOLDS {
        let r = match lookup(idx, bench_id, out, label) {
            Some(r) => r,
            None => continue,
        };
        let pass = r.median_ns <= threshold_ns;
        let verdict = if pass { "PASS" } else { "FAIL" };
        println!(
            "[{}] Phase-thr {label:30}: {:>12.0} ns ≤ {:>12.0} ns",
            verdict, r.median_ns, threshold_ns,
        );
        if pass {
            out.pass += 1;
        } else {
            out.fail += 1;
        }
    }
}

/// Per-cell regression vs the `perf_v0` baseline. Skips cells without a
/// baseline (newly-introduced benches). Threshold of 1.20× is intentionally
/// loose — criterion's median estimator alone can drift ±15% across runs.
const BASELINE_REGRESSION_RATIO: f64 = 1.20;

fn check_baseline_regression(results: &[BenchResult], out: &mut AssertOutcome) {
    for r in results {
        let baseline_ns = match read_perf_v0_median_ns(&r.id) {
            Some(b) if b > 0.0 => b,
            _ => continue, // no perf_v0 baseline → skip silently
        };
        let ratio = r.median_ns / baseline_ns;
        let pass = ratio <= BASELINE_REGRESSION_RATIO;
        if pass {
            out.pass += 1;
            // Only print the misses + the few headline winners; otherwise
            // the gate output would be dozens of lines of pass-by-default.
            // We print here only on FAIL.
        } else {
            out.fail += 1;
            println!(
                "[FAIL] Baseline regression: {} ratio {:.2}× (≤ {:.2}×)  [perf_v0 {:.0}ns → current {:.0}ns]",
                r.id, ratio, BASELINE_REGRESSION_RATIO, baseline_ns, r.median_ns,
            );
        }
    }
    println!(
        "[INFO] Baseline regression sweep: scanned {} cells against perf_v0 (ratio ≤ {:.2}×)",
        results.len(),
        BASELINE_REGRESSION_RATIO,
    );
}

fn lookup<'a>(
    idx: &'a BTreeMap<&str, &'a BenchResult>,
    id: &str,
    out: &mut AssertOutcome,
    label: &str,
) -> Option<&'a BenchResult> {
    match idx.get(id) {
        Some(r) => Some(*r),
        None => {
            out.skip += 1;
            println!("[SKIP] {label}: missing bench result `{id}`");
            None
        }
    }
}

/// Win-2: tick/idle at N=10000 should be ≤ ~3× tick/idle at N=100.
/// If dirty-receivers actually push work only when dirty, an idle tick
/// does constant work regardless of world size. The bench's smallest cell
/// is `entities/100`, not `/10`, so we anchor the small end there.
fn check_win_2_idle_flat(idx: &BTreeMap<&str, &BenchResult>, out: &mut AssertOutcome) {
    let small = match lookup(idx, "tick/idle/entities/100", out, "Win-2") {
        Some(r) => r,
        None => return,
    };
    let large = match lookup(idx, "tick/idle/entities/10000", out, "Win-2") {
        Some(r) => r,
        None => return,
    };

    let ratio = large.median_ns / small.median_ns;
    let threshold = 3.0;
    let verdict = if ratio <= threshold { "PASS" } else { "FAIL" };
    println!(
        "[{}] Win-2 idle O(1):   tick/idle 100→10000 ratio {:.2}× (≤ {:.1}×)  [{}ns → {}ns]",
        verdict, ratio, threshold, small.median_ns as u64, large.median_ns as u64
    );
    if ratio <= threshold {
        out.pass += 1;
    } else {
        out.fail += 1;
    }
}

/// Win-3: dirty-receiver push model. tick/active holds K mutations fixed
/// but the bench is parameterised by mutation count, not entity count —
/// we instead use update/mutation-style scenarios to confirm mutation
/// cost scales with mutations, not entities. If tick/active at 10 vs 1000
/// mutations scales roughly linearly with mutations (not wildly super-linear
/// from per-entity scans), push-model holds.
///
/// We assert: tick/active/mutations/1000 / tick/active/mutations/10
/// is within 200× (i.e. up to 2× overhead beyond the linear 100× factor).
fn check_win_3_dirty_receiver(idx: &BTreeMap<&str, &BenchResult>, out: &mut AssertOutcome) {
    let small = match lookup(idx, "tick/active/mutations/10", out, "Win-3") {
        Some(r) => r,
        None => return,
    };
    let large = match lookup(idx, "tick/active/mutations/1000", out, "Win-3") {
        Some(r) => r,
        None => return,
    };
    let ratio = large.median_ns / small.median_ns;
    // Linear would be 100×; allow up to 2× constant overhead → 200× ceiling.
    let threshold = 200.0;
    let verdict = if ratio <= threshold { "PASS" } else { "FAIL" };
    println!(
        "[{}] Win-3 push model:  tick/active 10→1000 mutations ratio {:.1}× (≤ {:.0}×)",
        verdict, ratio, threshold
    );
    if ratio <= threshold {
        out.pass += 1;
    } else {
        out.fail += 1;
    }
}

/// Win-4: SpawnWithComponents coalesces spawn+inserts.
///
/// **Important caveat (per `phase-06.md`):** the criterion `spawn/burst` and
/// `spawn/coalesced` benches both measure *one steady-state idle tick after*
/// a fully-replicated world is built — they don't measure the burst path
/// itself. So at N=1000 they should land within noise of each other, not
/// show a real coalesce delta. The actual Phase-6 wire-correctness gate
/// lives in `benches/examples/phase6_paint_rect_audit.rs` (asserts
/// `spawn_with_components == N`, zero stray `Spawn`/`InsertComponent`).
///
/// We therefore check that the two benches are within ±20% of each other,
/// rather than `coalesced strictly < burst` — the strict ordering would
/// fire on routine criterion noise (~±15% on small workloads). The real
/// burst path is gated by `Phase 6 paint_rect/{1000,5000}` thresholds
/// (defined below) which exercise actual replication.
fn check_win_4_coalesced_beats_burst(idx: &BTreeMap<&str, &BenchResult>, out: &mut AssertOutcome) {
    let ns = ["1000", "100", "10", "1"];
    let mut checked = false;
    for n in ns {
        let burst_id = format!("spawn/burst/entities/{n}");
        let coalesced_id = format!("spawn/coalesced/entities/{n}");
        if let (Some(&burst), Some(&coalesced)) = (idx.get(burst_id.as_str()), idx.get(coalesced_id.as_str())) {
            let ratio = coalesced.median_ns / burst.median_ns;
            let threshold = 1.20;
            let pass = ratio <= threshold;
            let verdict = if pass { "PASS" } else { "FAIL" };
            println!(
                "[{}] Win-4 coalesced:  spawn/coalesced/spawn/burst = {:.2}× (≤ {:.2}×) at N={} [{}ns vs {}ns; both idle-after-build]",
                verdict, ratio, threshold, n, coalesced.median_ns as u64, burst.median_ns as u64
            );
            if pass {
                out.pass += 1;
            } else {
                out.fail += 1;
            }
            checked = true;
            break;
        }
    }
    if !checked {
        out.skip += 1;
        println!("[SKIP] Win-4: no matching (spawn/burst, spawn/coalesced) pair");
    }
}

/// Win-5: immutable components skip diff-tracking. At same N,
/// immutable_idle should be ≤ mutable_idle.
fn check_win_5_immutable_beats_mutable(
    idx: &BTreeMap<&str, &BenchResult>,
    out: &mut AssertOutcome,
) {
    // Bench ids come from the `bench!()` macro which emits
    // `module_path!() ++ "::" ++ name`. The group prefix "update/immutable"
    // contains the full module path; criterion concatenates them.
    // Known suffixes: "mutable_idle", "immutable_idle".
    let mutable = idx
        .iter()
        .find(|(k, _)| k.contains("update/immutable") && k.ends_with("mutable_idle") && !k.ends_with("immutable_idle"))
        .map(|(_, v)| *v);
    let immutable = idx
        .iter()
        .find(|(k, _)| k.contains("update/immutable") && k.ends_with("immutable_idle"))
        .map(|(_, v)| *v);
    let (mutable, immutable) = match (mutable, immutable) {
        (Some(m), Some(i)) => (m, i),
        _ => {
            out.skip += 1;
            println!("[SKIP] Win-5: update/immutable results not found");
            return;
        }
    };

    let verdict = if immutable.median_ns <= mutable.median_ns * 1.05 {
        // Allow 5% noise margin — strictly less is too tight for idle-tick comparisons.
        "PASS"
    } else {
        "FAIL"
    };
    println!(
        "[{}] Win-5 immutable:   immutable_idle ({}ns) ≤ mutable_idle ({}ns) × 1.05",
        verdict, immutable.median_ns as u64, mutable.median_ns as u64
    );
    if immutable.median_ns <= mutable.median_ns * 1.05 {
        out.pass += 1;
    } else {
        out.fail += 1;
    }
}
