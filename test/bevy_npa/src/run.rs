use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::Args;
use namako_engine::codegen::{inventory, StepConstructor, WorldInventory};
use namako_engine::npap::{
    BindingSignature, ResolvedPlan, RunReport, ScenarioResult, ScenarioStatus, SemanticBinding,
    StepResult, StepStatus,
};
use namako_engine::step::{Context as StepContext, Step};

use crate::world::BevyTestWorld;

#[derive(Args, Debug)]
pub struct RunArgs {
    #[arg(short, long)]
    pub plan: PathBuf,
    #[arg(short, long, default_value = "run_report.json")]
    pub output: PathBuf,
}

struct StepEntry<W> {
    func: Step<W>,
    impl_hash: String,
    regex: regex::Regex,
}

fn build_dispatch_table<W: WorldInventory>() -> HashMap<String, StepEntry<W>> {
    let mut table = HashMap::new();
    for step in inventory::iter::<W::Given> {
        let meta = StepConstructor::<W>::npap_metadata(step);
        let (_, regex_fn, func) = step.inner();
        table.insert(
            meta.binding_id.to_string(),
            StepEntry { func, impl_hash: meta.impl_hash.to_string(), regex: regex_fn() },
        );
    }
    for step in inventory::iter::<W::When> {
        let meta = StepConstructor::<W>::npap_metadata(step);
        let (_, regex_fn, func) = step.inner();
        table.insert(
            meta.binding_id.to_string(),
            StepEntry { func, impl_hash: meta.impl_hash.to_string(), regex: regex_fn() },
        );
    }
    for step in inventory::iter::<W::Then> {
        let meta = StepConstructor::<W>::npap_metadata(step);
        let (_, regex_fn, func) = step.inner();
        table.insert(
            meta.binding_id.to_string(),
            StepEntry { func, impl_hash: meta.impl_hash.to_string(), regex: regex_fn() },
        );
    }
    table
}

fn collect_bindings<W: WorldInventory>() -> Vec<SemanticBinding> {
    let mut b = Vec::new();
    for step in inventory::iter::<W::Given> {
        let meta = StepConstructor::<W>::npap_metadata(step);
        b.push(SemanticBinding {
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
    for step in inventory::iter::<W::When> {
        let meta = StepConstructor::<W>::npap_metadata(step);
        b.push(SemanticBinding {
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
    for step in inventory::iter::<W::Then> {
        let meta = StepConstructor::<W>::npap_metadata(step);
        b.push(SemanticBinding {
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
    b
}

pub fn run(args: RunArgs) -> Result<()> {
    let plan_json = std::fs::read_to_string(&args.plan)
        .with_context(|| format!("Failed to read plan: {}", args.plan.display()))?;
    let plan: ResolvedPlan =
        serde_json::from_str(&plan_json).context("Failed to parse resolved plan JSON")?;

    let current_bindings = collect_bindings::<BevyTestWorld>();
    let current_registry = namako_engine::npap::SemanticStepRegistry::new(current_bindings);

    if plan.header.step_registry_hash != current_registry.step_registry_hash {
        bail!(
            "Plan step_registry_hash ({}) does not match current manifest ({})",
            plan.header.step_registry_hash,
            current_registry.step_registry_hash
        );
    }

    let dispatch_table = build_dispatch_table::<BevyTestWorld>();
    let mut scenario_results = Vec::with_capacity(plan.scenarios.len());

    for scenario in &plan.scenarios {
        let mut step_results = Vec::with_capacity(scenario.steps.len());
        let mut scenario_status = ScenarioStatus::Passed;
        let mut world = BevyTestWorld::default();

        for step in &scenario.steps {
            let entry = dispatch_table.get(&step.binding_id);
            let step_result = match entry {
                Some(e) => {
                    let mut captures = e.regex.capture_locations();
                    let names = e.regex.capture_names();
                    let matched = e.regex.captures_read(&mut captures, &step.step_text);
                    let matches: Vec<(Option<String>, String)> = if matched.is_some() {
                        names
                            .zip(
                                std::iter::once(step.step_text.clone()).chain(
                                    (1..captures.len()).map(|g| {
                                        captures
                                            .get(g)
                                            .map_or(String::new(), |(s, end)| {
                                                step.step_text[s..end].to_string()
                                            })
                                    }),
                                ),
                            )
                            .map(|(n, v)| (n.map(String::from), v))
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

        scenario_results.push(ScenarioResult {
            scenario_key: scenario.scenario_key.clone(),
            status: scenario_status,
            steps: step_results,
        });
    }

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
        let failed = run_report
            .scenarios
            .iter()
            .filter(|s| s.status == ScenarioStatus::Failed)
            .count();
        eprintln!("✗ Run complete with {} failed scenario(s)", failed);
        bail!("Run failed: {} scenario(s) did not pass", failed);
    }

    eprintln!("✓ Run complete. Output: {}", args.output.display());
    Ok(())
}
