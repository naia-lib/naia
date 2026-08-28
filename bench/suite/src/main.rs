//! Naia BDD Suite Wall-Clock Timing Benchmark
//!
//! Runs the full resolved_plan.json (sequentially or in parallel), measures
//! per-scenario wall time, and reports statistics for before/after comparison.
//!
//! # Usage
//!
//! ```
//! # Build the release binary first (avoids measuring compile time):
//! cargo build -p naia-suite-bench --release
//!
//! # Run sequentially (baseline):
//! ./target/release/naia_suite_bench \
//!     --plan test/specs/resolved_plan.json \
//!     --output-json target/bench/baseline_sequential.json
//!
//! # Run in parallel (N threads):
//! ./target/release/naia_suite_bench \
//!     --plan test/specs/resolved_plan.json \
//!     --jobs 16 \
//!     --output-json target/bench/optimised_parallel.json
//! ```

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

use anyhow::{bail, Context, Result};
use clap::Parser;
use futures::executor::block_on;
use naia_tests::TestWorld;
use namako_engine::codegen::{inventory, StepConstructor, WorldInventory};
use namako_engine::npap::{BindingSignature, ResolvedPlan, SemanticBinding, SemanticStepRegistry};
use namako_engine::step::Context as StepContext;
use rayon::prelude::*;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::json;

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

#[derive(Parser, Debug)]
#[command(
    name = "naia-suite-bench",
    about = "Wall-clock timing benchmark for the naia BDD suite"
)]
struct Args {
    /// Path to resolved_plan.json
    #[arg(short, long, default_value = "test/specs/resolved_plan.json")]
    plan: PathBuf,

    /// Write per-scenario timing JSON to this path (for baseline comparison)
    #[arg(long)]
    output_json: Option<PathBuf>,

    /// Also run the bevy spec plan for completeness
    #[arg(long)]
    bevy_plan: Option<PathBuf>,

    /// Print per-scenario timing (verbose)
    #[arg(long, short)]
    verbose: bool,

    /// Number of parallel worker threads (1 = sequential baseline, default = num CPUs)
    #[arg(long, short = 'j')]
    jobs: Option<usize>,
}

// ---------------------------------------------------------------------------
// Dispatch table (same logic as naia-npa/src/run.rs)
// ---------------------------------------------------------------------------

struct StepEntry<W> {
    func: namako_engine::step::Step<W>,
    _impl_hash: String,
    regex: Regex,
}

// StepEntry<W> is Sync: Step<W> is a fn pointer (Sync), Regex is Sync
#[allow(dead_code)]
fn _assert_step_entry_sync<W>() {
    fn require_sync<T: Sync>() {}
    require_sync::<StepEntry<W>>();
}

fn build_dispatch_table<W: WorldInventory>() -> std::collections::HashMap<String, StepEntry<W>> {
    let mut table = std::collections::HashMap::new();

    macro_rules! insert_steps {
        ($iter:expr) => {
            for step in $iter {
                let meta = StepConstructor::<W>::npap_metadata(step);
                let (_, regex_fn, func) = step.inner();
                table.insert(
                    meta.binding_id.to_string(),
                    StepEntry {
                        func,
                        _impl_hash: meta.impl_hash.to_string(),
                        regex: regex_fn(),
                    },
                );
            }
        };
    }

    insert_steps!(inventory::iter::<W::Given>);
    insert_steps!(inventory::iter::<W::When>);
    insert_steps!(inventory::iter::<W::Then>);

    table
}

fn collect_bindings<W: WorldInventory>() -> Vec<SemanticBinding> {
    let mut bindings = Vec::new();

    macro_rules! push_bindings {
        ($iter:expr) => {
            for step in $iter {
                let meta = StepConstructor::<W>::npap_metadata(step);
                bindings.push(SemanticBinding {
                    binding_id: meta.binding_id.to_string(),
                    kind: meta.kind.to_string(),
                    expression: meta.expression.to_string(),
                    signature: BindingSignature {
                        captures_arity: meta.captures_arity,
                        accepts_docstring: meta.accepts_docstring,
                        accepts_datatable: meta.accepts_datatable,
                    },
                    impl_hash: meta.impl_hash.to_string(),
                    source_symbol: Some(meta.source_symbol.to_string()),
                });
            }
        };
    }

    push_bindings!(inventory::iter::<W::Given>);
    push_bindings!(inventory::iter::<W::When>);
    push_bindings!(inventory::iter::<W::Then>);

    bindings
}

// ---------------------------------------------------------------------------
// Per-scenario execution
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ScenarioTiming {
    scenario_key: String,
    duration_ms: u64,
    passed: bool,
    error: Option<String>,
}

fn run_scenario_timed(
    scenario: &namako_engine::npap::ResolvedScenario,
    dispatch_table: &std::collections::HashMap<String, StepEntry<TestWorld>>,
) -> ScenarioTiming {
    let start = Instant::now();
    let mut world = TestWorld::default();
    let mut passed = true;
    let mut first_error: Option<String> = None;

    for step in &scenario.steps {
        let Some(entry) = dispatch_table.get(&step.binding_id) else {
            passed = false;
            first_error = Some(format!("Unknown binding_id: {}", step.binding_id));
            break;
        };

        // Build captures from the step text using the precompiled regex
        let mut capture_locs = entry.regex.capture_locations();
        let names = entry.regex.capture_names();
        let matched = entry
            .regex
            .captures_read(&mut capture_locs, &step.step_text);

        let matches: Vec<(Option<String>, String)> =
            if matched.is_some() {
                names
                    .zip(std::iter::once(step.step_text.clone()).chain(
                        (1..capture_locs.len()).map(|gid| {
                            capture_locs
                                .get(gid)
                                .map_or(String::new(), |(s, e)| step.step_text[s..e].to_string())
                        }),
                    ))
                    .map(|(n, v)| (n.map(String::from), v))
                    .collect()
            } else {
                vec![]
            };

        let ctx = StepContext {
            step: namako_engine::gherkin::Step {
                keyword: step.effective_kind.clone(),
                ty: match step.effective_kind.as_str() {
                    "Given" => namako_engine::gherkin::StepType::Given,
                    "When" => namako_engine::gherkin::StepType::When,
                    _ => namako_engine::gherkin::StepType::Then,
                },
                value: step.step_text.clone(),
                docstring: None,
                table: None,
                span: namako_engine::gherkin::Span { start: 0, end: 0 },
                position: namako_engine::gherkin::LineCol { line: 0, col: 1 },
            },
            matches,
        };

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            block_on((entry.func)(&mut world, ctx))
        }));

        if let Err(panic_val) = result {
            let msg = if let Some(s) = panic_val.downcast_ref::<&str>() {
                (*s).to_string()
            } else if let Some(s) = panic_val.downcast_ref::<String>() {
                s.clone()
            } else {
                "Step panicked".to_string()
            };
            passed = false;
            first_error = Some(msg);
            break;
        }
    }

    ScenarioTiming {
        scenario_key: scenario.scenario_key.clone(),
        duration_ms: start.elapsed().as_millis() as u64,
        passed,
        error: first_error,
    }
}

// ---------------------------------------------------------------------------
// Statistics helpers
// ---------------------------------------------------------------------------

fn percentile(sorted: &[u64], p: usize) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    sorted[(sorted.len() - 1) * p / 100]
}

fn mean(values: &[u64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().sum::<u64>() as f64 / values.len() as f64
}

// ---------------------------------------------------------------------------
// Report types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
struct SuiteBenchReport {
    plan_path: String,
    plan_parse_ms: u64,
    dispatch_setup_ms: u64,
    total_scenarios: usize,
    passed: usize,
    failed: usize,
    total_suite_ms: u64,
    scenarios_per_sec: f64,
    mean_scenario_ms: f64,
    p50_scenario_ms: u64,
    p90_scenario_ms: u64,
    p95_scenario_ms: u64,
    p99_scenario_ms: u64,
    max_scenario_ms: u64,
    min_scenario_ms: u64,
    slow_scenarios: Vec<ScenarioTiming>,
    scenarios: Vec<ScenarioTiming>,
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn run_plan(plan_path: &PathBuf, verbose: bool, jobs: Option<usize>) -> Result<SuiteBenchReport> {
    let parallel = jobs.map(|j| j != 1).unwrap_or(true);
    let mode = if parallel {
        let n = jobs.unwrap_or_else(rayon::current_num_threads);
        format!("parallel ({n} threads)")
    } else {
        "sequential".to_string()
    };

    eprintln!("=== Naia BDD Suite Bench ({mode}) ===");
    eprintln!("Plan: {}", plan_path.display());

    // 1. Parse plan
    let parse_start = Instant::now();
    let plan_json = std::fs::read_to_string(plan_path)
        .with_context(|| format!("Cannot read plan: {}", plan_path.display()))?;
    let plan: ResolvedPlan =
        serde_json::from_str(&plan_json).context("Cannot parse resolved_plan.json")?;
    let plan_parse_ms = parse_start.elapsed().as_millis() as u64;
    eprintln!(
        "  Plan parsed: {}ms  ({} scenarios)",
        plan_parse_ms,
        plan.scenarios.len()
    );

    // 2. Validate hash (same logic as naia-npa)
    let current_bindings = collect_bindings::<TestWorld>();
    let current_registry = SemanticStepRegistry::new(current_bindings);
    let skip_check = std::path::Path::new(".tesaki/skip_hash_check").exists();
    if !skip_check && plan.header.step_registry_hash != current_registry.step_registry_hash {
        bail!(
            "step_registry_hash mismatch (plan {} vs current {}). \
             Touch .tesaki/skip_hash_check to bypass.",
            plan.header.step_registry_hash,
            current_registry.step_registry_hash
        );
    }

    // 3. Build dispatch table
    let setup_start = Instant::now();
    let dispatch_table = Arc::new(build_dispatch_table::<TestWorld>());
    let dispatch_setup_ms = setup_start.elapsed().as_millis() as u64;
    eprintln!(
        "  Dispatch table built: {}ms  ({} bindings)",
        dispatch_setup_ms,
        dispatch_table.len()
    );

    // 4. Run all scenarios (sequential or parallel), measuring each
    eprintln!("\nRunning {} scenarios ({mode})...", plan.scenarios.len());
    let suite_start = Instant::now();
    let total = plan.scenarios.len();
    let completed = Arc::new(AtomicUsize::new(0));

    let timings: Vec<ScenarioTiming> = if parallel {
        let pool = {
            let mut b = rayon::ThreadPoolBuilder::new();
            if let Some(j) = jobs {
                b = b.num_threads(j);
            }
            b.build().context("Failed to build thread pool")?
        };
        let dt = Arc::clone(&dispatch_table);
        let ctr = Arc::clone(&completed);
        pool.install(|| {
            plan.scenarios
                .par_iter()
                .map(|scenario| {
                    let timing = run_scenario_timed(scenario, &dt);
                    let done = ctr.fetch_add(1, Ordering::Relaxed) + 1;
                    let status = if timing.passed { "✓" } else { "✗" };
                    if verbose || !timing.passed {
                        eprintln!(
                            "  [{done}/{total} {status}] {}  ({}ms)",
                            timing.scenario_key, timing.duration_ms
                        );
                    } else if done.is_multiple_of(50) {
                        eprintln!("  ... {done}/{total} scenarios done");
                    }
                    timing
                })
                .collect()
        })
    } else {
        plan.scenarios
            .iter()
            .enumerate()
            .map(|(idx, scenario)| {
                let timing = run_scenario_timed(scenario, &dispatch_table);
                let status = if timing.passed { "✓" } else { "✗" };
                if verbose || !timing.passed {
                    eprintln!(
                        "  [{}/{total} {status}] {}  ({}ms)",
                        idx + 1,
                        timing.scenario_key,
                        timing.duration_ms
                    );
                } else if (idx + 1) % 50 == 0 {
                    eprintln!("  ... {}/{total} scenarios done", idx + 1);
                }
                timing
            })
            .collect()
    };

    let total_suite_ms = suite_start.elapsed().as_millis() as u64;

    // 5. Compute statistics
    let passed = timings.iter().filter(|t| t.passed).count();
    let failed = timings.iter().filter(|t| !t.passed).count();

    let mut durations_ms: Vec<u64> = timings.iter().map(|t| t.duration_ms).collect();
    durations_ms.sort_unstable();

    let p50 = percentile(&durations_ms, 50);
    let p90 = percentile(&durations_ms, 90);
    let p95 = percentile(&durations_ms, 95);
    let p99 = percentile(&durations_ms, 99);
    let max = durations_ms.last().copied().unwrap_or(0);
    let min = durations_ms.first().copied().unwrap_or(0);
    let avg = mean(&durations_ms);
    let scenarios_per_sec = plan.scenarios.len() as f64 / (total_suite_ms as f64 / 1000.0);

    // Top 10 slowest
    let mut slow = timings.clone();
    slow.sort_by_key(|t| std::cmp::Reverse(t.duration_ms));
    let slow_scenarios = slow.into_iter().take(10).collect::<Vec<_>>();

    eprintln!("\n=== Results ===");
    eprintln!(
        "  Scenarios:       {total} ({passed} passed, {failed} failed)",
        total = plan.scenarios.len(),
        passed = passed,
        failed = failed
    );
    eprintln!(
        "  Total time:      {:.1}s ({:.0}ms)",
        total_suite_ms as f64 / 1000.0,
        total_suite_ms
    );
    eprintln!("  Scenarios/sec:   {:.2}", scenarios_per_sec);
    eprintln!("\nPer-scenario timing:");
    eprintln!("  Min:    {:>6}ms", min);
    eprintln!("  Mean:   {:>6.0}ms", avg);
    eprintln!("  P50:    {:>6}ms", p50);
    eprintln!("  P90:    {:>6}ms", p90);
    eprintln!("  P95:    {:>6}ms", p95);
    eprintln!("  P99:    {:>6}ms", p99);
    eprintln!("  Max:    {:>6}ms", max);
    eprintln!("\nTop 10 slowest scenarios:");
    for (i, t) in slow_scenarios.iter().enumerate().take(10) {
        let status = if t.passed { "✓" } else { "✗" };
        eprintln!(
            "  {:2}. [{status}] {}ms  {}",
            i + 1,
            t.duration_ms,
            t.scenario_key
        );
    }

    if failed > 0 {
        eprintln!("\nFailed scenarios:");
        for t in timings.iter().filter(|t| !t.passed) {
            eprintln!("  ✗ {}: {:?}", t.scenario_key, t.error);
        }
    }

    Ok(SuiteBenchReport {
        plan_path: plan_path.display().to_string(),
        plan_parse_ms,
        dispatch_setup_ms,
        total_scenarios: plan.scenarios.len(),
        passed,
        failed,
        total_suite_ms,
        scenarios_per_sec,
        mean_scenario_ms: avg,
        p50_scenario_ms: p50,
        p90_scenario_ms: p90,
        p95_scenario_ms: p95,
        p99_scenario_ms: p99,
        max_scenario_ms: max,
        min_scenario_ms: min,
        slow_scenarios,
        scenarios: timings,
    })
}

fn main() -> Result<()> {
    let args = Args::parse();

    let report = run_plan(&args.plan, args.verbose, args.jobs)?;

    // JSON output (sentinel format for tooling, same as cyberlith_bench)
    let json_val = json!({
        "plan_path": report.plan_path,
        "plan_parse_ms": report.plan_parse_ms,
        "dispatch_setup_ms": report.dispatch_setup_ms,
        "total_scenarios": report.total_scenarios,
        "passed": report.passed,
        "failed": report.failed,
        "total_suite_ms": report.total_suite_ms,
        "scenarios_per_sec": report.scenarios_per_sec,
        "mean_scenario_ms": report.mean_scenario_ms,
        "p50_scenario_ms": report.p50_scenario_ms,
        "p90_scenario_ms": report.p90_scenario_ms,
        "p95_scenario_ms": report.p95_scenario_ms,
        "p99_scenario_ms": report.p99_scenario_ms,
        "max_scenario_ms": report.max_scenario_ms,
        "min_scenario_ms": report.min_scenario_ms,
    });
    println!("SUITE_BENCH_RESULT:{}", serde_json::to_string(&json_val)?);

    // Save full JSON if requested
    if let Some(out_path) = &args.output_json {
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let full_json = serde_json::to_string_pretty(&report)?;
        std::fs::write(out_path, &full_json)
            .with_context(|| format!("Cannot write output: {}", out_path.display()))?;
        eprintln!("\nFull timing report saved to: {}", out_path.display());
    }

    if report.failed > 0 {
        bail!(
            "{} scenario(s) failed — check timing report for details",
            report.failed
        );
    }

    Ok(())
}
