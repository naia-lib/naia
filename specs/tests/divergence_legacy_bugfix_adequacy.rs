use assert_cmd::Command;
use std::fs;
use std::path::PathBuf;

#[test]
fn divergence_legacy_bugfix_adequacy() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let legacy_baseline_path = manifest_dir.join("tests/baseline/output/adequacy/default.stdout");
    
    let workspace_root = manifest_dir
        .parent()
        .expect("Could not find workspace root")
        .to_path_buf();

    let legacy_output = fs::read_to_string(&legacy_baseline_path)
        .expect("Failed to read legacy baseline");

    let mut cmd = Command::cargo_bin("spec_tool").unwrap();
    cmd.current_dir(&workspace_root)
       .arg("adequacy");

    let assert = cmd.assert();
    let output = assert.get_output();
    let rust_stdout = String::from_utf8_lossy(&output.stdout);

    // 1. Assert Divergence: The Rust tool should NOT match the legacy output (which falsely reports missing tests)
    assert_ne!(rust_stdout, legacy_output, "Rust tool matched legacy output, but we expected a divergence fix!");

    // 2. Assert Correctness: Rust tool should report 0 missing tests
    assert!(rust_stdout.contains("Missing tests:               0"));

    // Should NOT output Priority 1 section if count is 0
    assert!(!rust_stdout.contains("Priority 1: Missing Tests"));
    
    assert.success();
}
