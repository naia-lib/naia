//! `naia_namako run` command implementation.
//!
//! Executes a resolved plan and outputs a run report with real step dispatch.
//! Scenarios run in parallel (one per rayon worker thread) since each
//! scenario creates fully isolated world state with no shared mutable
//! globals. TestClock is thread_local, LocalTransportHub is per-instance,
//! and every step function is a plain fn pointer (Send + Sync).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use clap::Args;
use namako_engine::codegen::{inventory, StepConstructor, WorldInventory};
use namako_engine::npap::{
    BindingSignature, ResolvedPlan, RunReport, ScenarioResult, ScenarioStatus, SemanticBinding,
    StepResult, StepStatus,
};
use namako_engine::step::{Context as StepContext, Step};
use rayon::prelude::*;

use naia_tests::TestWorld;

/// Arguments for the run command.
#[derive(Args, Debug)]
pub struct RunArgs {
    /// Path to the resolved_plan.json file
    #[arg(short, long)]
    pub plan: PathBuf,

    /// Output path for run_report.json
    #[arg(short, long, default_value = "run_report.json")]
    pub output: PathBuf,

    /// Run only the scenario with this exact `scenario_key` (e.g.
    /// `"authority:Rule(03):Scenario(08)"`). The full plan is loaded and
    /// validated as usual; only the scenario list is filtered before
    /// dispatch. Errors with the closest 3 keys (by edit distance) when
    /// the requested key is not present.
    #[arg(long)]
    pub scenario_key: Option<String>,

    /// Maximum number of scenarios to run in parallel.
    /// Defaults to the number of logical CPUs.
    #[arg(long, short = 'j')]
    pub jobs: Option<usize>,
}

/// Step dispatcher entry — contains the step function and metadata.
struct StepEntry<W> {
    func: Step<W>,
    impl_hash: String,
    regex: regex::Regex,
}

// StepEntry<W> is Sync because:
//   - Step<W> = fn pointer → Send + Sync
//   - String    → Send + Sync
//   - Regex     → Send + Sync
unsafe impl<W> Sync for StepEntry<W> {}

/// Build a dispatch table mapping binding_id → step entry from inventory.
fn build_dispatch_table<W: WorldInventory>() -> HashMap<String, StepEntry<W>> {
    let mut table = HashMap::new();

    macro_rules! insert_steps {
        ($iter:expr) => {
            for step in $iter {
                let meta = StepConstructor::<W>::npap_metadata(step);
                let (_, regex_fn, func) = step.inner();
                table.insert(
                    meta.binding_id.to_string(),
                    StepEntry {
                        func,
                        impl_hash: meta.impl_hash.to_string(),
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

/// Collect bindings from inventory for registry hash computation.
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

/// Return up to `n` `scenario_key`s from `scenarios`, ordered by ascending
/// Levenshtein distance from `target`. Used to give a helpful error when
/// `--scenario-key` is misspelled.
fn closest_scenario_keys(
    target: &str,
    scenarios: &[namako_engine::npap::ResolvedScenario],
    n: usize,
) -> Vec<String> {
    let mut scored: Vec<(usize, &str)> = scenarios
        .iter()
        .map(|s| (levenshtein(target, &s.scenario_key), s.scenario_key.as_str()))
        .collect();
    scored.sort_by_key(|(d, _)| *d);
    scored.into_iter().take(n).map(|(_, k)| k.to_string()).collect()
}

/// Standard Levenshtein edit distance.
fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr: Vec<usize> = vec![0; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        curr[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            curr[j + 1] = (curr[j] + 1)
                .min(prev[j + 1] + 1)
                .min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b.len()]
}

/// Execute a single scenario against the dispatch table.
/// Each call creates a fresh `TestWorld`; no state is shared between calls.
fn run_scenario(
    scenario: &namako_engine::npap::ResolvedScenario,
    dispatch_table: &HashMap<String, StepEntry<TestWorld>>,
) -> ScenarioResult {
    let mut step_results = Vec::with_capacity(scenario.steps.len());
    let mut scenario_status = ScenarioStatus::Passed;
    let mut world = TestWorld::default();

    for step in &scenario.steps {
        let entry = dispatch_table.get(&step.binding_id);

        let step_result = match entry {
            Some(e) => {
                let mut captures = e.regex.capture_locations();
                let names = e.regex.capture_names();
                let matched = e.regex.captures_read(&mut captures, &step.step_text);

                let matches: Vec<(Option<String>, String)> = if matched.is_some() {
                    names
                        .zip(std::iter::once(step.step_text.clone()).chain(
                            (1..captures.len()).map(|group_id| {
                                captures.get(group_id).map_or(String::new(), |(s, end)| {
                                    step.step_text[s..end].to_string()
                                })
                            }),
                        ))
                        .map(|(name, val)| (name.map(String::from), val))
                        .collect()
                } else {
                    vec![]
                };

                let context = StepContext {
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

                let exec_result =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        futures::executor::block_on((e.func)(&mut world, context))
                    }));

                match exec_result {
                    Ok(()) => StepResult {
                        planned_binding_id: step.binding_id.clone(),
                        executed_binding_id: step.binding_id.clone(),
                        planned_payload_hash: step.payload_hash.clone(),
                        executed_payload_hash: step.payload_hash.clone(),
                        executed_impl_hash: e.impl_hash.clone(),
                        status: StepStatus::Passed,
                        error_message: None,
                    },
                    Err(panic_info) => {
                        let msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                            s.to_string()
                        } else if let Some(s) = panic_info.downcast_ref::<String>() {
                            s.clone()
                        } else {
                            "Step panicked".to_string()
                        };
                        scenario_status = ScenarioStatus::Failed;
                        StepResult {
                            planned_binding_id: step.binding_id.clone(),
                            executed_binding_id: step.binding_id.clone(),
                            planned_payload_hash: step.payload_hash.clone(),
                            executed_payload_hash: step.payload_hash.clone(),
                            executed_impl_hash: e.impl_hash.clone(),
                            status: StepStatus::Failed,
                            error_message: Some(msg),
                        }
                    }
                }
            }
            None => {
                scenario_status = ScenarioStatus::Failed;
                StepResult {
                    planned_binding_id: step.binding_id.clone(),
                    executed_binding_id: String::new(),
                    planned_payload_hash: step.payload_hash.clone(),
                    executed_payload_hash: String::new(),
                    executed_impl_hash: String::new(),
                    status: StepStatus::Failed,
                    error_message: Some(format!("Unknown binding_id: {}", step.binding_id)),
                }
            }
        };

        if step_result.status == StepStatus::Failed {
            scenario_status = ScenarioStatus::Failed;
        }

        step_results.push(step_result);
    }

    ScenarioResult {
        scenario_key: scenario.scenario_key.clone(),
        status: scenario_status,
        steps: step_results,
    }
}

/// Run the run command.
pub fn run(args: RunArgs) -> Result<()> {
    // Step 1: Read and parse the resolved plan
    let plan_json = std::fs::read_to_string(&args.plan)
        .with_context(|| format!("Failed to read plan: {}", args.plan.display()))?;
    let mut plan: ResolvedPlan =
        serde_json::from_str(&plan_json).context("Failed to parse resolved plan JSON")?;

    // Optional single-scenario filter (--scenario-key)
    if let Some(target_key) = args.scenario_key.as_ref() {
        let matched = plan
            .scenarios
            .iter()
            .any(|s| s.scenario_key == *target_key);
        if !matched {
            let suggestions = closest_scenario_keys(target_key, &plan.scenarios, 3);
            let hint = if suggestions.is_empty() {
                String::new()
            } else {
                format!("\n  Closest matches:\n    {}", suggestions.join("\n    "))
            };
            bail!(
                "--scenario-key not found in plan: {:?}{}",
                target_key,
                hint
            );
        }
        plan.scenarios.retain(|s| s.scenario_key == *target_key);
    }

    // Step 2: Validate step_registry_hash matches current manifest
    let current_bindings = collect_bindings::<TestWorld>();
    let current_registry = namako_engine::npap::SemanticStepRegistry::new(current_bindings);

    let skip_hash_check = std::path::Path::new(".tesaki/skip_hash_check").exists();
    if skip_hash_check {
        eprintln!("[naia_npa] .tesaki/skip_hash_check present — skipping step_registry_hash validation");
    } else if plan.header.step_registry_hash != current_registry.step_registry_hash {
        bail!(
            "Plan step_registry_hash ({}) does not match current manifest ({}). \
             The adapter has changed since the plan was created. \
             Touch .tesaki/skip_hash_check to bypass during development.",
            plan.header.step_registry_hash,
            current_registry.step_registry_hash
        );
    }

    // Step 3: Build dispatch table (once, shared read-only across all threads)
    let dispatch_table = Arc::new(build_dispatch_table::<TestWorld>());

    // Step 4: Execute scenarios in parallel.
    //
    // Safety: Each scenario gets its own fresh `TestWorld` (no shared mutable
    // state). TestClock uses thread_local storage so parallel threads never
    // interfere with each other's clock. The dispatch table is read-only
    // after construction. StepEntry<TestWorld>: Sync (see unsafe impl above).
    let total = plan.scenarios.len();
    let completed = Arc::new(AtomicUsize::new(0));

    // Build the rayon thread pool respecting --jobs (default: num CPUs)
    let thread_pool = {
        let mut builder = rayon::ThreadPoolBuilder::new();
        if let Some(j) = args.jobs {
            builder = builder.num_threads(j);
        }
        builder.build().context("Failed to build rayon thread pool")?
    };

    let dt = Arc::clone(&dispatch_table);
    let ctr = Arc::clone(&completed);

    let scenario_results: Vec<ScenarioResult> = thread_pool.install(|| {
        plan.scenarios
            .par_iter()
            .map(|scenario| {
                let result = run_scenario(scenario, &dt);
                let done = ctr.fetch_add(1, Ordering::Relaxed) + 1;
                let status = if result.status == ScenarioStatus::Passed { "✓" } else { "✗" };
                eprintln!("[{done}/{total} {status}] {}", scenario.scenario_key);
                result
            })
            .collect()
    });

    // Step 5: Build and output run report
    let run_report = RunReport::new(
        plan.header.feature_fingerprint_hash.clone(),
        plan.header.step_registry_hash.clone(),
        plan.header.resolved_plan_hash.clone(),
        scenario_results,
    );

    let has_failures = run_report
        .scenarios
        .iter()
        .any(|s| s.status == ScenarioStatus::Failed);

    let json = serde_json::to_string_pretty(&run_report)?;
    std::fs::write(&args.output, &json)
        .with_context(|| format!("Failed to write {}", args.output.display()))?;

    if has_failures {
        let failed_count = run_report
            .scenarios
            .iter()
            .filter(|s| s.status == ScenarioStatus::Failed)
            .count();
        eprintln!(
            "✗ Run complete with {} failed scenario(s). Output: {}",
            failed_count,
            args.output.display()
        );
        bail!("Run failed: {} scenario(s) did not pass", failed_count);
    }

    eprintln!("✓ Run complete ({total} scenarios). Output: {}", args.output.display());
    Ok(())
}
