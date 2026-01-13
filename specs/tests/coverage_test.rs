use assert_cmd::Command;
use std::fs;
use std::path::PathBuf;

#[test]
fn test_coverage_output_matches_baseline() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // Baseline is in specs/tests/... 
    let baseline_path = manifest_dir.join("tests/baseline/output/coverage/default.stdout");
    
    // The workspace root is the parent of specs (manifest_dir)
    let workspace_root = manifest_dir
        .parent()
        .expect("Could not find workspace root")
        .to_path_buf();

    let expected_output = fs::read_to_string(&baseline_path)
        .unwrap_or_else(|_| panic!("Failed to read baseline at {:?}", baseline_path));

    let mut cmd = Command::cargo_bin("spec_tool").unwrap();
    // Run from workspace root so it finds specs/ folder correctly
    cmd.current_dir(&workspace_root)
       .arg("coverage");

    // Capture output
    let assert = cmd.assert();
    let output = assert.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);

    if stdout != expected_output {
        println!("--- EXPECTED ---\n{}", expected_output);
        println!("--- ACTUAL ---\n{}", stdout);
    }

    assert.success().stdout(expected_output);
}
