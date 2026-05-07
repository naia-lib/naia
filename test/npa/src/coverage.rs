//! `coverage` subcommand — Naia-specific BDD coverage report.
//!
//! Walks `test/specs/features/*.feature` for `[contract-id]` brackets and
//! `@Deferred` / `@PolicyOnly` tags. Reports per-feature scenario counts and
//! contract-ID coverage status (active / deferred-only / policy-only).
//!
//! Replaces the prior `_AGENTS/scripts/coverage_diff.py` (deleted 2026-05-07).
//! The contract-ID logic is Naia-specific; namako (the generic SDD framework)
//! does not know about contract IDs.
//!
//! Usage:
//!
//! ```bash
//! cargo run -p naia_npa -- coverage
//! cargo run -p naia_npa -- coverage --json
//! cargo run -p naia_npa -- coverage --fail-on-deferred-non-policy
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::Args;
use regex::Regex;
use serde::Serialize;

#[derive(Args, Debug)]
pub struct CoverageArgs {
    /// Root of the specs tree (defaults to `test/specs/` relative to CWD).
    #[arg(long, default_value = "test/specs")]
    pub specs_root: PathBuf,

    /// Emit machine-readable JSON instead of the human summary.
    #[arg(long)]
    pub json: bool,

    /// Exit non-zero if any `@Deferred` Scenario lacks a `@PolicyOnly` tag.
    /// Used as a CI gate after Q4/Q5 quality-debt cleanup lands.
    #[arg(long)]
    pub fail_on_deferred_non_policy: bool,
}

#[derive(Debug, Serialize)]
struct FeatureReport {
    file: String,
    total_scenarios: usize,
    active: usize,
    deferred_total: usize,
    deferred_policy_only: usize,
    deferred_non_policy: usize,
}

#[derive(Debug, Serialize)]
struct CoverageReport {
    features: Vec<FeatureReport>,
    total_scenarios: usize,
    active_scenarios: usize,
    deferred_scenarios: usize,
    deferred_non_policy_scenarios: usize,
    contracts_with_active_coverage: Vec<String>,
    contracts_deferred_only: Vec<String>,
    deferred_non_policy_offenders: Vec<String>,
}

pub fn run(args: CoverageArgs) -> Result<()> {
    let features_dir = args.specs_root.join("features");
    if !features_dir.exists() {
        anyhow::bail!(
            "features dir not found: {}",
            features_dir.display()
        );
    }

    let scenario_re = Regex::new(r"^\s*Scenario(?:\s+Outline)?:\s*(.*)$").unwrap();
    let contract_re = Regex::new(r"\[([a-z][a-z0-9-]*-[0-9a-z]+)\]").unwrap();
    let tag_re = Regex::new(r"^\s*@").unwrap();

    let mut features: Vec<FeatureReport> = Vec::new();
    let mut active_contracts: BTreeSet<String> = BTreeSet::new();
    let mut deferred_contracts: BTreeSet<String> = BTreeSet::new();
    let mut offenders: Vec<String> = Vec::new();

    let mut entries: Vec<PathBuf> = fs::read_dir(&features_dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().map(|e| e == "feature").unwrap_or(false))
        .collect();
    entries.sort();

    for path in entries {
        let report = scan_feature(&path, &scenario_re, &contract_re, &tag_re,
            &mut active_contracts, &mut deferred_contracts, &mut offenders)?;
        features.push(report);
    }

    let total = features.iter().map(|f| f.total_scenarios).sum();
    let active = features.iter().map(|f| f.active).sum();
    let deferred = features.iter().map(|f| f.deferred_total).sum();
    let deferred_non_policy = features.iter().map(|f| f.deferred_non_policy).sum();

    let deferred_only: Vec<String> = deferred_contracts
        .difference(&active_contracts)
        .cloned()
        .collect();

    let report = CoverageReport {
        features,
        total_scenarios: total,
        active_scenarios: active,
        deferred_scenarios: deferred,
        deferred_non_policy_scenarios: deferred_non_policy,
        contracts_with_active_coverage: active_contracts.into_iter().collect(),
        contracts_deferred_only: deferred_only,
        deferred_non_policy_offenders: offenders,
    };

    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_human(&report);
    }

    if args.fail_on_deferred_non_policy && report.deferred_non_policy_scenarios > 0 {
        eprintln!(
            "\n❌ {} @Deferred Scenario(s) lack @PolicyOnly tag (gate failed)",
            report.deferred_non_policy_scenarios
        );
        std::process::exit(1);
    }

    Ok(())
}

fn scan_feature(
    path: &Path,
    scenario_re: &Regex,
    contract_re: &Regex,
    tag_re: &Regex,
    active_contracts: &mut BTreeSet<String>,
    deferred_contracts: &mut BTreeSet<String>,
    offenders: &mut Vec<String>,
) -> Result<FeatureReport> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("reading {}", path.display()))?;

    let mut total = 0;
    let mut active = 0;
    let mut deferred_total = 0;
    let mut deferred_policy = 0;
    let mut deferred_non_policy = 0;

    let mut pending_tags: Vec<String> = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if tag_re.is_match(line) {
            for tok in trimmed.split_whitespace() {
                if tok.starts_with('@') {
                    pending_tags.push(tok.to_string());
                }
            }
            continue;
        }

        if let Some(caps) = scenario_re.captures(line) {
            total += 1;
            let title = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            let is_deferred = pending_tags.iter().any(|t| t.starts_with("@Deferred"));
            let is_policy = pending_tags.iter().any(|t| t == "@PolicyOnly");

            let contracts: Vec<String> = contract_re
                .captures_iter(title)
                .map(|c| c[1].to_string())
                .collect();

            if is_deferred {
                deferred_total += 1;
                for c in &contracts { deferred_contracts.insert(c.clone()); }
                if is_policy {
                    deferred_policy += 1;
                } else {
                    deferred_non_policy += 1;
                    offenders.push(format!(
                        "{}: {}",
                        path.file_name().and_then(|s| s.to_str()).unwrap_or("?"),
                        title.trim()
                    ));
                }
            } else {
                active += 1;
                for c in &contracts { active_contracts.insert(c.clone()); }
            }

            pending_tags.clear();
            continue;
        }

        // Non-tag, non-scenario, non-blank line clears pending tags only when
        // it isn't a Rule/Feature/Background structural keyword.
        if !trimmed.starts_with("Rule:")
            && !trimmed.starts_with("Feature:")
            && !trimmed.starts_with("Background:")
            && !trimmed.starts_with("Examples:")
        {
            // Steps inside a scenario/background may follow tags from the next
            // scenario; clear conservatively only on blank lines (handled
            // above) — leave pending_tags alone here.
        }
    }

    Ok(FeatureReport {
        file: path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("?")
            .to_string(),
        total_scenarios: total,
        active,
        deferred_total,
        deferred_policy_only: deferred_policy,
        deferred_non_policy,
    })
}

fn print_human(report: &CoverageReport) {
    println!("=== Naia BDD Coverage ===\n");
    println!(
        "{:<32}  {:>5}  {:>6}  {:>8}  {:>6}  {:>6}",
        "feature", "total", "active", "deferred", "policy", "junk"
    );
    println!("{}", "-".repeat(74));
    for f in &report.features {
        println!(
            "{:<32}  {:>5}  {:>6}  {:>8}  {:>6}  {:>6}",
            f.file,
            f.total_scenarios,
            f.active,
            f.deferred_total,
            f.deferred_policy_only,
            f.deferred_non_policy
        );
    }
    println!("{}", "-".repeat(74));
    println!(
        "{:<32}  {:>5}  {:>6}  {:>8}  {:>6}  {:>6}",
        "TOTAL",
        report.total_scenarios,
        report.active_scenarios,
        report.deferred_scenarios,
        report.deferred_scenarios - report.deferred_non_policy_scenarios,
        report.deferred_non_policy_scenarios
    );

    println!("\n{} active scenario(s), {} deferred, {} policy-only",
        report.active_scenarios,
        report.deferred_scenarios,
        report.deferred_scenarios - report.deferred_non_policy_scenarios);
    println!("Contracts with active (non-deferred) coverage: {}",
        report.contracts_with_active_coverage.len());
    println!("Contracts deferred-only (no active coverage): {}",
        report.contracts_deferred_only.len());

    if !report.contracts_deferred_only.is_empty() {
        let by_area: BTreeMap<String, Vec<String>> = group_by_area(&report.contracts_deferred_only);
        println!("\nDeferred-only contracts by area:");
        for (area, ids) in &by_area {
            println!("  {} ({}): {}", area, ids.len(), ids.join(", "));
        }
    }

    if report.deferred_non_policy_scenarios > 0 {
        println!("\n⚠️  {} @Deferred Scenario(s) lack @PolicyOnly tag.",
            report.deferred_non_policy_scenarios);
        println!("    These are quality-debt items (Category B/C) — see SDD_QUALITY_DEBT_PLAN.md.");
    }
}

fn group_by_area(contracts: &[String]) -> BTreeMap<String, Vec<String>> {
    let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for c in contracts {
        let area = c
            .rsplit_once('-')
            .map(|(prefix, _)| prefix.to_string())
            .unwrap_or_else(|| c.clone());
        groups.entry(area).or_default().push(c.clone());
    }
    groups
}
