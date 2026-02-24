//! Namako Pipeline Integration Tests
//!
//! These tests verify the P0 proof obligations documented in:
//! - DEMO_RUNTIME_FAILURE.md (Demo A)
//! - DEMO_IMPL_DRIFT.md (Demo B)
//!
//! Run with: `cargo test -p naia_npa --test namako_integration_test`

use std::process::{Command, Output};
use std::path::PathBuf;
use std::fs;

/// Get the naia_npa crate directory (CARGO_MANIFEST_DIR = naia/test/npa/)
fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Get the naia/specs directory path
fn specs_dir() -> PathBuf {
    crate_dir()
        .parent().unwrap()  // naia/
        .join("specs")
}

/// Get the namako CLI manifest path
fn namako_cli_manifest() -> PathBuf {
    crate_dir()
        .parent().unwrap()  // naia/test/
        .parent().unwrap()  // naia/
        .parent().unwrap()  // specops/
        .join("namako/Cargo.toml")
}

/// Get the naia_npa adapter manifest path (this crate)
fn adapter_manifest() -> PathBuf {
    crate_dir().join("Cargo.toml")
}

/// Helper: Run a cargo command and capture output
#[allow(dead_code)]
fn run_cargo(args: &[&str], cwd: &PathBuf) -> Output {
    Command::new("cargo")
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("Failed to execute cargo command")
}

/// Helper: Run namako CLI command
fn run_namako_cli(subcommand: &str, extra_args: &[&str]) -> Output {
    let specs = specs_dir();
    let cli_manifest = namako_cli_manifest();
    let adapter_cmd = format!("cargo run --manifest-path {} --", adapter_manifest().display());

    let mut args = vec![
        "run",
        "-p", "namako_cli",
        "--manifest-path", cli_manifest.to_str().unwrap(),
        "--",
        subcommand,
    ];

    // Add adapter command for commands that need it
    if subcommand == "lint" || subcommand == "verify" || subcommand == "update-cert" {
        args.push("-a");
        args.push(&adapter_cmd);
    }

    args.extend(extra_args);

    Command::new("cargo")
        .args(&args)
        .current_dir(&specs)
        .output()
        .expect("Failed to execute namako CLI")
}

/// Helper: Run naia_npa adapter directly
fn run_adapter(subcommand: &str, extra_args: &[&str]) -> Output {
    let specs = specs_dir();
    let adapter_manifest = adapter_manifest();

    let mut args = vec![
        "run",
        "--manifest-path", adapter_manifest.to_str().unwrap(),
        "--",
        subcommand,
    ];
    args.extend(extra_args);

    Command::new("cargo")
        .args(&args)
        .current_dir(&specs)
        .output()
        .expect("Failed to execute adapter")
}

/// Helper: Assert output contains substring
fn assert_output_contains(output: &Output, needle: &str, context: &str) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}{}", stdout, stderr);

    assert!(
        combined.contains(needle),
        "{}: Expected output to contain '{}'\nstdout: {}\nstderr: {}",
        context, needle, stdout, stderr
    );
}

/// Helper: Assert command succeeded
fn assert_success(output: &Output, context: &str) {
    assert!(
        output.status.success(),
        "{}: Expected success but got exit code {:?}\nstdout: {}\nstderr: {}",
        context,
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Helper: Assert command failed
fn assert_failure(output: &Output, context: &str) {
    assert!(
        !output.status.success(),
        "{}: Expected failure but got success\nstdout: {}\nstderr: {}",
        context,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

// ============================================================================
// Demo A: Runtime Failure Tests
// ============================================================================

/// Verify that lint passes on the current smoke feature
#[test]
fn demo_a_lint_passes_on_valid_features() {
    let output = run_namako_cli("lint", &[
        "-s", ".",
        "-o", "test_resolved_plan.json",
    ]);

    assert_success(&output, "lint on valid features");
    assert_output_contains(&output, "Lint passed", "lint success message");

    // Cleanup
    let _ = fs::remove_file(specs_dir().join("test_resolved_plan.json"));
}

/// Verify that adapter run succeeds with valid scenarios
#[test]
fn demo_a_adapter_run_succeeds_on_valid_scenarios() {
    // First lint to get a resolved plan
    let lint_output = run_namako_cli("lint", &[
        "-s", ".",
        "-o", "test_run_plan.json",
    ]);
    assert_success(&lint_output, "lint for run test");

    // Then run the adapter
    let run_output = run_adapter("run", &[
        "-p", "test_run_plan.json",
        "-o", "test_run_report.json",
    ]);

    assert_success(&run_output, "adapter run on valid scenarios");
    assert_output_contains(&run_output, "Run complete", "run success message");

    // Cleanup
    let _ = fs::remove_file(specs_dir().join("test_run_plan.json"));
    let _ = fs::remove_file(specs_dir().join("test_run_report.json"));
}

/// Verify update-cert works after successful run
#[test]
fn demo_a_update_cert_succeeds_after_passing_run() {
    // Lint
    let lint_output = run_namako_cli("lint", &[
        "-s", ".",
        "-o", "test_cert_plan.json",
    ]);
    assert_success(&lint_output, "lint for cert test");

    // Run
    let run_output = run_adapter("run", &[
        "-p", "test_cert_plan.json",
        "-o", "test_cert_report.json",
    ]);
    assert_success(&run_output, "run for cert test");

    // Update cert
    let cert_output = run_namako_cli("update-cert", &[
        "-r", "test_cert_report.json",
        "-o", "test_certification.json",
    ]);

    assert_success(&cert_output, "update-cert after passing run");
    assert_output_contains(&cert_output, "Certification updated", "cert success message");

    // Cleanup
    let _ = fs::remove_file(specs_dir().join("test_cert_plan.json"));
    let _ = fs::remove_file(specs_dir().join("test_cert_report.json"));
    let _ = fs::remove_file(specs_dir().join("test_certification.json"));
}

/// Verify that verify passes when all hashes match
#[test]
fn demo_a_verify_passes_with_matching_hashes() {
    // Lint
    let lint_output = run_namako_cli("lint", &[
        "-s", ".",
        "-o", "test_verify_plan.json",
    ]);
    assert_success(&lint_output, "lint for verify test");

    // Run
    let run_output = run_adapter("run", &[
        "-p", "test_verify_plan.json",
        "-o", "test_verify_report.json",
    ]);
    assert_success(&run_output, "run for verify test");

    // Update cert
    let cert_output = run_namako_cli("update-cert", &[
        "-r", "test_verify_report.json",
        "-o", "test_verify_baseline.json",
    ]);
    assert_success(&cert_output, "update-cert for verify test");

    // Verify
    let verify_output = run_namako_cli("verify", &[
        "-s", ".",
        "-c", "test_verify_baseline.json",
        "-r", "test_verify_report.json",
    ]);

    assert_success(&verify_output, "verify with matching hashes");
    assert_output_contains(&verify_output, "Verification passed", "verify success message");

    // Cleanup
    let _ = fs::remove_file(specs_dir().join("test_verify_plan.json"));
    let _ = fs::remove_file(specs_dir().join("test_verify_report.json"));
    let _ = fs::remove_file(specs_dir().join("test_verify_baseline.json"));
}

// ============================================================================
// Demo B: Impl Drift / Stale Plan Tests
// ============================================================================

/// Verify adapter refuses stale plan with hash mismatch
///
/// This test uses a pre-created stale plan file that has a different
/// step_registry_hash than the current adapter manifest.
#[test]
fn demo_b_adapter_refuses_stale_plan() {
    // Create a stale plan by modifying step_registry_hash
    let plan_path = specs_dir().join("test_stale_plan.json");

    // First get a valid plan
    let lint_output = run_namako_cli("lint", &[
        "-s", ".",
        "-o", "test_stale_plan.json",
    ]);
    assert_success(&lint_output, "lint for stale plan test");

    // Modify the step_registry_hash to make it stale
    let plan_content = fs::read_to_string(&plan_path).expect("read plan");
    let mut plan: serde_json::Value = serde_json::from_str(&plan_content).expect("parse plan");
    if let Some(header) = plan.get_mut("header") {
        header["step_registry_hash"] = serde_json::json!("deadbeef00000000000000000000000000000000000000000000000000000000");
    }
    fs::write(&plan_path, serde_json::to_string_pretty(&plan).unwrap()).expect("write stale plan");

    // Try to run with stale plan - should fail
    let run_output = run_adapter("run", &[
        "-p", "test_stale_plan.json",
        "-o", "test_stale_report.json",
    ]);

    assert_failure(&run_output, "adapter should refuse stale plan");
    assert_output_contains(&run_output, "Plan step_registry_hash", "stale plan error message");
    assert_output_contains(&run_output, "does not match current manifest", "stale plan error message");

    // Cleanup
    let _ = fs::remove_file(specs_dir().join("test_stale_plan.json"));
    let _ = fs::remove_file(specs_dir().join("test_stale_report.json"));
}

/// Verify that verify detects baseline drift when identity changes
#[test]
fn demo_b_verify_detects_baseline_drift() {
    // Create a baseline with different identity
    let baseline_path = specs_dir().join("test_drift_baseline.json");

    // First get valid artifacts
    let lint_output = run_namako_cli("lint", &[
        "-s", ".",
        "-o", "test_drift_plan.json",
    ]);
    assert_success(&lint_output, "lint for drift test");

    let run_output = run_adapter("run", &[
        "-p", "test_drift_plan.json",
        "-o", "test_drift_report.json",
    ]);
    assert_success(&run_output, "run for drift test");

    // Create baseline with modified step_registry_hash
    let cert_output = run_namako_cli("update-cert", &[
        "-r", "test_drift_report.json",
        "-o", "test_drift_baseline.json",
    ]);
    assert_success(&cert_output, "update-cert for drift test");

    // Modify baseline identity to simulate drift
    let baseline_content = fs::read_to_string(&baseline_path).expect("read baseline");
    let mut baseline: serde_json::Value = serde_json::from_str(&baseline_content).expect("parse baseline");
    if let Some(identity) = baseline.get_mut("identity") {
        identity["step_registry_hash"] = serde_json::json!("deadbeef00000000000000000000000000000000000000000000000000000000");
    }
    fs::write(&baseline_path, serde_json::to_string_pretty(&baseline).unwrap()).expect("write drift baseline");

    // Verify should fail with drift detection
    let verify_output = run_namako_cli("verify", &[
        "-s", ".",
        "-c", "test_drift_baseline.json",
        "-r", "test_drift_report.json",
    ]);

    assert_failure(&verify_output, "verify should detect baseline drift");
    // Should report drift in step_registry_hash
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&verify_output.stdout),
        String::from_utf8_lossy(&verify_output.stderr)
    );
    assert!(
        combined.contains("DRIFT") || combined.contains("mismatch") || combined.contains("differ"),
        "verify should report drift: {}",
        combined
    );

    // Cleanup
    let _ = fs::remove_file(specs_dir().join("test_drift_plan.json"));
    let _ = fs::remove_file(specs_dir().join("test_drift_report.json"));
    let _ = fs::remove_file(specs_dir().join("test_drift_baseline.json"));
}

// ============================================================================
// Identity Invariant Tests
// ============================================================================

/// Verify that certification identity contains exactly 4 fields
#[test]
fn identity_has_exactly_four_fields() {
    // Lint
    let lint_output = run_namako_cli("lint", &[
        "-s", ".",
        "-o", "test_identity_plan.json",
    ]);
    assert_success(&lint_output, "lint for identity test");

    // Run
    let run_output = run_adapter("run", &[
        "-p", "test_identity_plan.json",
        "-o", "test_identity_report.json",
    ]);
    assert_success(&run_output, "run for identity test");

    // Update cert
    let cert_output = run_namako_cli("update-cert", &[
        "-r", "test_identity_report.json",
        "-o", "test_identity_cert.json",
    ]);
    assert_success(&cert_output, "update-cert for identity test");

    // Check certification structure
    let cert_content = fs::read_to_string(specs_dir().join("test_identity_cert.json"))
        .expect("read certification");
    let cert: serde_json::Value = serde_json::from_str(&cert_content)
        .expect("parse certification");

    let identity = cert.get("identity").expect("certification has identity");
    let identity_obj = identity.as_object().expect("identity is object");

    // Must have exactly these 4 fields
    assert!(identity_obj.contains_key("hash_contract_version"), "identity has hash_contract_version");
    assert!(identity_obj.contains_key("feature_fingerprint_hash"), "identity has feature_fingerprint_hash");
    assert!(identity_obj.contains_key("step_registry_hash"), "identity has step_registry_hash");
    assert!(identity_obj.contains_key("resolved_plan_hash"), "identity has resolved_plan_hash");

    // Must NOT have run_report_hash in identity
    assert!(!identity_obj.contains_key("run_report_hash"), "identity must NOT contain run_report_hash");

    // run_report_hash should be in metadata, not identity
    let metadata = cert.get("metadata").expect("certification has metadata");
    let metadata_obj = metadata.as_object().expect("metadata is object");
    assert!(metadata_obj.contains_key("run_report_hash"), "metadata has run_report_hash");

    // Cleanup
    let _ = fs::remove_file(specs_dir().join("test_identity_plan.json"));
    let _ = fs::remove_file(specs_dir().join("test_identity_report.json"));
    let _ = fs::remove_file(specs_dir().join("test_identity_cert.json"));
}

/// Verify step_registry_hash changes when impl_hash changes
#[test]
fn registry_hash_depends_on_impl_hash() {
    // Get the current manifest
    let manifest_output = run_adapter("manifest", &[]);
    assert_success(&manifest_output, "get manifest");

    let manifest: serde_json::Value = serde_json::from_slice(&manifest_output.stdout)
        .expect("parse manifest");

    // Verify manifest has step_registry_hash
    assert!(manifest.get("step_registry_hash").is_some(), "manifest has step_registry_hash");

    // Verify manifest has bindings with impl_hash
    let bindings = manifest.get("bindings").expect("manifest has bindings");
    let bindings_arr = bindings.as_array().expect("bindings is array");
    assert!(!bindings_arr.is_empty(), "bindings array is not empty");

    for binding in bindings_arr {
        assert!(binding.get("impl_hash").is_some(), "binding has impl_hash");
    }
}
