use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;

#[test]
fn test_help_matches_baseline() {
    let mut cmd = Command::cargo_bin("spec_tool").unwrap();
    // Assuming we run 'cargo test' from specs directory
    let baseline_path = "tests/baseline/output/help/default.stdout";
    let baseline = fs::read_to_string(baseline_path).expect("Failed to read baseline");
    
    let assert = cmd.arg("help").assert();
    let output = assert.get_output();
    let stdout = String::from_utf8(output.stdout.clone()).unwrap();
    
    if stdout != baseline {
        println!("Len Expected: {}, Len Actual: {}", baseline.len(), stdout.len());
        for (i, (a, b)) in stdout.chars().zip(baseline.chars()).enumerate() {
            if a != b {
                println!("Mismatch at index {}: '{}' vs '{}'", i, a.escape_debug(), b.escape_debug());
                let start = if i > 20 { i - 20 } else { 0 };
                let end = if i + 20 < stdout.len() { i + 20 } else { stdout.len() };
                println!("Context: {:?}", &stdout[start..end]);
                break;
            }
        }
        panic!("Output mismatch");
    }
}
