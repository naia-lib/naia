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
fn determinism_only_affects_metadata() {
    let temp = tempdir().unwrap();
    let out_normal = temp.path().join("normal.md");
    let out_det = temp.path().join("det.md");

    Command::cargo_bin("spec_tool").unwrap()
        .arg("registry")
        .arg(out_normal.to_str().unwrap())
        .assert()
        .success();

    Command::cargo_bin("spec_tool").unwrap()
        .arg("registry")
        .arg(out_det.to_str().unwrap())
        .arg("--deterministic")
        .assert()
        .success();

    let content_normal = fs::read_to_string(&out_normal).unwrap();
    let content_det = fs::read_to_string(&out_det).unwrap();

    let lines_normal: Vec<&str> = content_normal.lines().collect();
    let lines_det: Vec<&str> = content_det.lines().collect();
    
    // We expect line count equality
    assert_eq!(lines_normal.len(), lines_det.len(), "Line count mismatch");
    
    let mut diffs = 0;
    for (l_n, l_d) in lines_normal.into_iter().zip(lines_det.into_iter()) {
        if l_n != l_d {
            diffs += 1;
            if !l_n.contains("**Generated:**") {
                 panic!("Unexpected difference: \nN: {}\nD: {}", l_n, l_d);
            }
        }
    }
    
    assert!(diffs <= 1, "Expected only timestamp difference");
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
