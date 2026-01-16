//! `naia_namako run` command implementation.
//!
//! Executes a resolved plan and outputs a run report.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::Args;
use namako::npap::{
    ResolvedPlan, RunReport, ScenarioResult, StepResult,
    StepStatus, ScenarioStatus,
};

use crate::bindings::smoke_bindings;

/// Arguments for the run command.
#[derive(Args, Debug)]
pub struct RunArgs {
    /// Path to the resolved_plan.json file
    #[arg(short, long)]
    pub plan: PathBuf,

    /// Output path for run_report.json
    #[arg(short, long, default_value = "run_report.json")]
    pub output: PathBuf,
}

/// Run the run command.
pub fn run(args: RunArgs) -> Result<()> {
    // Step 1: Read and parse the resolved plan
    let plan_json = std::fs::read_to_string(&args.plan)
        .with_context(|| format!("Failed to read plan: {}", args.plan.display()))?;
    let plan: ResolvedPlan = serde_json::from_str(&plan_json)
        .context("Failed to parse resolved plan JSON")?;

    // Step 2: Validate step_registry_hash matches current manifest
    let current_bindings = smoke_bindings();
    let current_registry = namako::npap::SemanticStepRegistry::new(current_bindings);

    if plan.header.step_registry_hash != current_registry.step_registry_hash {
        bail!(
            "Plan step_registry_hash ({}) does not match current manifest ({}). \
             The adapter has changed since the plan was created.",
            plan.header.step_registry_hash,
            current_registry.step_registry_hash
        );
    }

    // Step 3: Execute each scenario
    let mut scenario_results = Vec::with_capacity(plan.scenarios.len());

    for scenario in &plan.scenarios {
        let mut step_results = Vec::with_capacity(scenario.steps.len());
        let mut scenario_status = ScenarioStatus::Passed;

        for step in &scenario.steps {
            // Look up binding by binding_id
            let binding = current_registry.bindings.iter()
                .find(|b| b.binding_id == step.binding_id);

            let step_result = match binding {
                Some(b) => {
                    // Execute the step (stub: all pass for now)
                    // In real implementation, dispatch to actual step function
                    StepResult {
                        planned_binding_id: step.binding_id.clone(),
                        executed_binding_id: step.binding_id.clone(),
                        planned_payload_hash: step.payload_hash.clone(),
                        // Echo planned values for now (real impl would compute executed)
                        executed_payload_hash: step.payload_hash.clone(),
                        executed_impl_hash: b.impl_hash.clone(),
                        status: StepStatus::Passed,
                        error_message: None,
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
                        error_message: Some(format!(
                            "Unknown binding_id: {}",
                            step.binding_id
                        )),
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

    // Step 4: Build and output run report
    let run_report = RunReport::new(
        plan.header.feature_fingerprint_hash.clone(),
        plan.header.step_registry_hash.clone(),
        plan.header.resolved_plan_hash.clone(),
        scenario_results,
    );

    let json = serde_json::to_string_pretty(&run_report)?;
    std::fs::write(&args.output, &json)
        .with_context(|| format!("Failed to write {}", args.output.display()))?;

    eprintln!("✓ Run complete. Output: {}", args.output.display());
    Ok(())
}
