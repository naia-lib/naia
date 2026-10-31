use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::Args;
use namako_engine::codegen::{inventory, StepConstructor, WorldInventory};
use namako_engine::evidence::{dispatch_step, scenario_result};
use namako_engine::npap::{
    BindingSignature, PlannedStep, ResolvedPlan, RunReport, ScenarioResult, ScenarioStatus,
    SemanticBinding, StepResult,
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
    binding: SemanticBinding,
    regex: regex::Regex,
}

fn build_dispatch_table<W: WorldInventory>() -> HashMap<String, StepEntry<W>> {
    let bindings: HashMap<String, SemanticBinding> = collect_bindings::<W>()
        .into_iter()
        .map(|b| (b.binding_id.clone(), b))
        .collect();
    let mut table = HashMap::new();
    for step in inventory::iter::<W::Given> {
        let meta = StepConstructor::<W>::npap_metadata(step);
        let (_, regex_fn, func) = step.inner();
        table.insert(
            meta.binding_id.to_string(),
            StepEntry {
                func,
                impl_hash: meta.impl_hash.to_string(),
                regex: regex_fn(),
                binding: bindings[meta.binding_id].clone(),
            },
        );
    }
    for step in inventory::iter::<W::When> {
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
    for step in inventory::iter::<W::Then> {
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

/// Capture values the binding's own matcher extracts from `text`, in group
/// order; empty when the text does not match.
fn matched_captures(regex: &regex::Regex, text: &str) -> Vec<String> {
    regex.captures(text).map_or_else(Vec::new, |caps| {
        caps.iter()
            .skip(1)
            .map(|m| m.map_or_else(String::new, |m| m.as_str().to_owned()))
            .collect()
    })
}

/// Dispatches one planned step through namako's faithful dispatch boundary:
/// the step function receives exactly the planned captures, docstring and
/// table, and the executed identity is derived from those values.
fn dispatch<W>(world: &mut W, step: &PlannedStep, entry: Option<&StepEntry<W>>) -> StepResult {
    let captures = entry.map_or_else(Vec::new, |e| matched_captures(&e.regex, &step.step_text));
    dispatch_step(step, entry.map(|e| &e.binding), &captures, |payload| {
        let Some(e) = entry else {
            return Err(format!("no step function for {}", payload.binding_id));
        };
        let context = StepContext {
            step: namako_engine::gherkin::Step {
                keyword: payload.effective_kind.to_owned(),
                ty: match payload.effective_kind {
                    "Given" => namako_engine::gherkin::StepType::Given,
                    "When" => namako_engine::gherkin::StepType::When,
                    _ => namako_engine::gherkin::StepType::Then,
                },
                value: payload.step_text.to_owned(),
                docstring: payload.docstring.map(str::to_owned),
                table: payload.datatable.map(|rows| namako_engine::gherkin::Table {
                    rows: rows.to_vec(),
                    span: namako_engine::gherkin::Span { start: 0, end: 0 },
                    position: namako_engine::gherkin::LineCol { line: 0, col: 1 },
                }),
                span: namako_engine::gherkin::Span { start: 0, end: 0 },
                position: namako_engine::gherkin::LineCol { line: 0, col: 1 },
            },
            matches: e
                .regex
                .capture_names()
                .zip(std::iter::once(payload.step_text.to_owned()).chain(payload.captures.iter().cloned()))
                .map(|(name, value)| (name.map(String::from), value))
                .collect(),
        };
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            futures::executor::block_on((e.func)(world, context));
        }))
        .map_err(|panic_info| {
            panic_info
                .downcast_ref::<&str>()
                .map(|s| (*s).to_owned())
                .or_else(|| panic_info.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "Step panicked".to_owned())
        })
    })
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
        let mut world = BevyTestWorld::default();
        let steps = scenario
            .steps
            .iter()
            .map(|step| dispatch(&mut world, step, dispatch_table.get(&step.binding_id)))
            .collect();
        scenario_results.push(scenario_result(scenario.scenario_key.clone(), steps));
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
