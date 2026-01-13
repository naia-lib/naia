use assert_cmd::Command;
use std::fs;

#[test]
fn test_packet_generation_connection_01() {
    let mut cmd = Command::cargo_bin("spec_tool").unwrap();
    
    // Output path relative to package root when running tests? 
    // Usually cargo test runs in package root.
    // spec_tool uses relative paths from CWD.
    
    // We want to generate to a temp file or checking the default location
    let output_path = "generated/packets/connection-01.md";
    
    cmd.arg("packet")
       .arg("connection-01")
       .assert()
       .success();

    assert!(fs::metadata(output_path).is_ok());

    let content = fs::read_to_string(output_path).unwrap();
    
    assert!(content.contains("# Contract Review Packet: connection-01"));
    assert!(content.contains("**Spec File:** 1_connection_lifecycle.md"));
    assert!(content.contains("### Test File: `01_connection_lifecycle.rs`"));
    
    // Check for context extraction
    assert!(content.contains("/// Contract: [connection-01]"));
    assert!(content.contains("#[test]"));
    assert!(content.contains("fn basic_connect_disconnect_lifecycle"));
    
    // Check for assertion index
    assert!(content.contains("// Assertion Index:"));
    assert!(content.contains("// (no explicit labels)"));
}
