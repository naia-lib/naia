use assert_cmd::Command;
use std::fs;
use tempfile::tempdir;

#[test]
fn registry_determinism() {
    let temp = tempdir().unwrap();
    let out1 = temp.path().join("out1.md");
    let out2 = temp.path().join("out2.md");

    let mut cmd1 = Command::cargo_bin("spec_tool").unwrap();
    cmd1.arg("--deterministic")
        .arg("registry")
        .arg(out1.to_str().unwrap())
        .assert()
        .success();

    let mut cmd2 = Command::cargo_bin("spec_tool").unwrap();
    cmd2.arg("--deterministic")
        .arg("registry")
        .arg(out2.to_str().unwrap())
        .assert()
        .success();

    let content1 = fs::read_to_string(&out1).unwrap();
    let content2 = fs::read_to_string(&out2).unwrap();

    assert_eq!(content1, content2);
    if !content1.contains("1970-01-01 00:00 UTC") {
        println!("Content1: \n{}", content1);
    }
    assert!(content1.contains("1970-01-01 00:00 UTC"));
}

#[test]
fn traceability_determinism() {
    let temp = tempdir().unwrap();
    let out1 = temp.path().join("trace1.md");
    let out2 = temp.path().join("trace2.md");

    let mut cmd1 = Command::cargo_bin("spec_tool").unwrap();
    cmd1.arg("--deterministic")
        .arg("traceability")
        .arg(out1.to_str().unwrap())
        .assert()
        .success();

    let mut cmd2 = Command::cargo_bin("spec_tool").unwrap();
    cmd2.arg("--deterministic")
        .arg("traceability")
        .arg(out2.to_str().unwrap())
        .assert()
        .success();

    let content1 = fs::read_to_string(&out1).unwrap();
    let content2 = fs::read_to_string(&out2).unwrap();

    assert_eq!(content1, content2);
    assert!(content1.contains("1970-01-01 00:00 UTC"));
}

#[test]
fn bundle_determinism() {
    let temp = tempdir().unwrap();
    let out1 = temp.path().join("NAIA_SPECS1.md");
    let out2 = temp.path().join("NAIA_SPECS2.md");

    let mut cmd1 = Command::cargo_bin("spec_tool").unwrap();
    cmd1.arg("--deterministic")
        .arg("bundle")
        .arg(out1.to_str().unwrap())
        .assert()
        .success();

    let mut cmd2 = Command::cargo_bin("spec_tool").unwrap();
    cmd2.arg("--deterministic")
        .arg("bundle")
        .arg(out2.to_str().unwrap())
        .assert()
        .success();

    let content1 = fs::read_to_string(&out1).unwrap();
    let content2 = fs::read_to_string(&out2).unwrap();

    assert_eq!(content1, content2);
    assert!(content1.contains("1970-01-01 00:00 UTC"));
}

#[test]
fn packet_determinism() {
    let temp = tempdir().unwrap();
    let out1 = temp.path().join("packet1.md");
    let out2 = temp.path().join("packet2.md");

    // We assume connection-01 exists
    let mut cmd1 = Command::cargo_bin("spec_tool").unwrap();
    cmd1.arg("--deterministic")
        .arg("packet")
        .arg("connection-01")
        .arg("--out")
        .arg(out1.to_str().unwrap())
        .assert()
        .success();

    let mut cmd2 = Command::cargo_bin("spec_tool").unwrap();
    cmd2.arg("--deterministic")
        .arg("packet")
        .arg("connection-01")
        .arg("--out")
        .arg(out2.to_str().unwrap())
        .assert()
        .success();

    let content1 = fs::read_to_string(&out1).unwrap();
    let content2 = fs::read_to_string(&out2).unwrap();

    assert_eq!(content1, content2);
    assert!(content1.contains("1970-01-01 00:00 UTC"));
}

#[test]
fn verify_determinism() {
    let temp = tempdir().unwrap();
    let out1 = temp.path().join("verify1.md");
    let out2 = temp.path().join("verify2.md");

    let mut cmd1 = Command::cargo_bin("spec_tool").unwrap();
    cmd1.arg("--deterministic")
        .arg("verify")
        .arg("--contract")
        .arg("connection-01")
        .arg("--write-report")
        .arg(out1.to_str().unwrap());
    
    // We don't check output status because tests might fail, but report generation should work
    let _ = cmd1.output(); 

    let mut cmd2 = Command::cargo_bin("spec_tool").unwrap();
    cmd2.arg("--deterministic")
        .arg("verify")
        .arg("--contract")
        .arg("connection-01")
        .arg("--write-report")
        .arg(out2.to_str().unwrap());
    
    let _ = cmd2.output();

    if out1.exists() && out2.exists() {
        let content1 = fs::read_to_string(&out1).unwrap();
        let content2 = fs::read_to_string(&out2).unwrap();
        assert_eq!(content1, content2);
        assert!(content1.contains("1970-01-01 00:00 UTC"));
    } else {
        panic!("Report files not generated");
    }
}
