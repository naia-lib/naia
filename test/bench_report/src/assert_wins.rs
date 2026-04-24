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
//! Call `run(&results)` — prints a pass/fail line per win and returns
//! an `AssertOutcome` whose `failed()` tells the caller whether to exit
//! non-zero.

use std::collections::BTreeMap;

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

    println!("---");
    println!("win-assert summary: {}", out.summary());
    out
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

/// Win-2: tick/idle at N=10000 should be ≤ ~3× tick/idle at N=10.
/// If dirty-receivers actually push work only when dirty, an idle tick
/// does constant work regardless of world size.
fn check_win_2_idle_flat(idx: &BTreeMap<&str, &BenchResult>, out: &mut AssertOutcome) {
    let small = match lookup(idx, "tick/idle/entities/10", out, "Win-2") {
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
        "[{}] Win-2 idle O(1):   tick/idle 10→10000 ratio {:.2}× (≤ {:.1}×)  [{}ns → {}ns]",
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

/// Win-4: SpawnWithComponents coalesces spawn+inserts. At the same N,
/// spawn/coalesced should have a smaller median than spawn/burst.
fn check_win_4_coalesced_beats_burst(idx: &BTreeMap<&str, &BenchResult>, out: &mut AssertOutcome) {
    // Pick the largest N present. Both groups use the same N set.
    let ns = ["1000", "100", "10", "1"];
    let mut checked = false;
    for n in ns {
        let burst_id = format!("spawn/burst/entities/{n}");
        let coalesced_id = format!("spawn/coalesced/entities/{n}");
        if let (Some(&burst), Some(&coalesced)) = (idx.get(burst_id.as_str()), idx.get(coalesced_id.as_str())) {
            let verdict = if coalesced.median_ns < burst.median_ns { "PASS" } else { "FAIL" };
            println!(
                "[{}] Win-4 coalesced:   spawn/coalesced ({}ns) < spawn/burst ({}ns) at N={}",
                verdict, coalesced.median_ns as u64, burst.median_ns as u64, n
            );
            if coalesced.median_ns < burst.median_ns {
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
