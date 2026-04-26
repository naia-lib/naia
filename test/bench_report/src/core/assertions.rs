//! Assert the Naia 0.25 Win invariants against a cargo-criterion run.
//!
//! The Wins are:
//!   2. idle tick is O(1) in world size.
//!   3. dirty-receiver push model: mutation cost is O(mutations × users).
//!   4. SpawnWithComponents coalesces spawn+inserts into one command.
//!   5. immutable components skip diff-tracking allocation.
//!
//! Phase 7 hardening adds absolute phase thresholds and baseline regression checks.
//!
//! Phase 10 adds halo capacity checks: idle tick budget and client keep-up.
//!
//! Call `run(&results)` — prints a pass/fail line per check and returns an
//! `AssertOutcome` whose `failed()` tells the caller whether to exit non-zero.

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::core::model::BenchResult;

pub struct AssertOutcome {
    pub pass: usize,
    pub fail: usize,
    pub skip: usize,
}

impl AssertOutcome {
    pub fn failed(&self) -> bool {
        self.fail > 0
    }
    pub fn summary(&self) -> String {
        format!("{} passed, {} failed, {} skipped", self.pass, self.fail, self.skip)
    }
}

fn index(results: &[BenchResult]) -> BTreeMap<&str, &BenchResult> {
    results.iter().map(|r| (r.id.as_str(), r)).collect()
}

pub fn run(results: &[BenchResult]) -> AssertOutcome {
    let idx = index(results);
    let mut out = AssertOutcome { pass: 0, fail: 0, skip: 0 };

    check_win_2_idle_flat(&idx, &mut out);
    check_win_3_dirty_receiver(&idx, &mut out);
    check_win_4_coalesced_beats_burst(&idx, &mut out);
    check_win_5_immutable_beats_mutable(&idx, &mut out);
    check_phase_thresholds(&idx, &mut out);
    check_halo_idle_budget(&idx, &mut out);
    check_halo_client_keepup(&idx, &mut out);
    check_baseline_regression(results, &mut out);

    println!("---");
    println!("win-assert summary: {}", out.summary());
    out
}

// ─── Phase 10 — halo capacity checks ─────────────────────────────────────────

/// Halo idle budget: steady_state_idle must be ≤ 5 ms.
/// Ensures the measured per-game tick cost leaves meaningful capacity headroom.
fn check_halo_idle_budget(idx: &BTreeMap<&str, &BenchResult>, out: &mut AssertOutcome) {
    check_threshold(
        idx,
        crate::core::capacity::ids::STEADY_STATE_IDLE,
        5_000_000.0, // 5 ms ceiling
        "Halo.idle_budget",
        out,
    );
}

/// Halo client keep-up: client_receive_active must be ≤ 4 ms (10% of 40 ms budget).
fn check_halo_client_keepup(idx: &BTreeMap<&str, &BenchResult>, out: &mut AssertOutcome) {
    check_threshold(
        idx,
        crate::core::capacity::ids::CLIENT_RECEIVE_ACTIVE,
        4_000_000.0, // 4 ms ceiling
        "Halo.client_keepup",
        out,
    );
}

// ─── Wins 2–5 ────────────────────────────────────────────────────────────────

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
    if ratio <= threshold { out.pass += 1; } else { out.fail += 1; }
}

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
    let threshold = 200.0;
    let verdict = if ratio <= threshold { "PASS" } else { "FAIL" };
    println!(
        "[{}] Win-3 push model:  tick/active 10→1000 mutations ratio {:.1}× (≤ {:.0}×)",
        verdict, ratio, threshold
    );
    if ratio <= threshold { out.pass += 1; } else { out.fail += 1; }
}

fn check_win_4_coalesced_beats_burst(idx: &BTreeMap<&str, &BenchResult>, out: &mut AssertOutcome) {
    let ns = ["1000", "100", "10", "1"];
    let mut checked = false;
    for n in ns {
        let burst_id = format!("spawn/burst/entities/{n}");
        let coalesced_id = format!("spawn/coalesced/entities/{n}");
        if let (Some(&burst), Some(&coalesced)) =
            (idx.get(burst_id.as_str()), idx.get(coalesced_id.as_str()))
        {
            let ratio = coalesced.median_ns / burst.median_ns;
            let threshold = 1.20;
            let pass = ratio <= threshold;
            let verdict = if pass { "PASS" } else { "FAIL" };
            println!(
                "[{}] Win-4 coalesced:  spawn/coalesced/spawn/burst = {:.2}× (≤ {:.2}×) at N={} [{}ns vs {}ns; both idle-after-build]",
                verdict, ratio, threshold, n, coalesced.median_ns as u64, burst.median_ns as u64
            );
            if pass { out.pass += 1; } else { out.fail += 1; }
            checked = true;
            break;
        }
    }
    if !checked {
        out.skip += 1;
        println!("[SKIP] Win-4: no matching (spawn/burst, spawn/coalesced) pair");
    }
}

fn check_win_5_immutable_beats_mutable(idx: &BTreeMap<&str, &BenchResult>, out: &mut AssertOutcome) {
    let mutable = idx
        .iter()
        .find(|(k, _)| {
            k.contains("update/immutable")
                && k.ends_with("mutable_idle")
                && !k.ends_with("immutable_idle")
        })
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
    let verdict = if immutable.median_ns <= mutable.median_ns * 1.05 { "PASS" } else { "FAIL" };
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

// ─── Phase thresholds ─────────────────────────────────────────────────────────

const PHASE_THRESHOLDS: &[(&str, f64, &str)] = &[
    ("tick/idle_matrix/u_x_n/16u_10000e",           3_000_000.0,   "Phase 3 mutable idle"),
    ("tick/idle_matrix_immutable/u_x_n/16u_10000e", 400_000.0,     "Phase 4 immutable idle"),
    ("spawn/paint_rect/entities/1000",               28_000_000.0,  "Phase 6 paint_rect/1000"),
    ("spawn/paint_rect/entities/5000",               220_000_000.0, "Phase 6 paint_rect/5000"),
];

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
        if pass { out.pass += 1; } else { out.fail += 1; }
    }
}

// ─── Baseline regression ──────────────────────────────────────────────────────

const BASELINE_REGRESSION_RATIO: f64 = 1.20;

fn check_baseline_regression(results: &[BenchResult], out: &mut AssertOutcome) {
    for r in results {
        let baseline_ns = match read_perf_v0_median_ns(&r.id) {
            Some(b) if b > 0.0 => b,
            _ => continue,
        };
        let ratio = r.median_ns / baseline_ns;
        let pass = ratio <= BASELINE_REGRESSION_RATIO;
        if pass {
            out.pass += 1;
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

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn check_threshold(
    idx: &BTreeMap<&str, &BenchResult>,
    bench_id: &str,
    threshold_ns: f64,
    label: &str,
    out: &mut AssertOutcome,
) {
    let r = match lookup(idx, bench_id, out, label) {
        Some(r) => r,
        None => return,
    };
    let pass = r.median_ns <= threshold_ns;
    let verdict = if pass { "PASS" } else { "FAIL" };
    println!(
        "[{}] {label:30}: {:>12.0} ns ≤ {:>12.0} ns",
        verdict, r.median_ns, threshold_ns,
    );
    if pass { out.pass += 1; } else { out.fail += 1; }
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
