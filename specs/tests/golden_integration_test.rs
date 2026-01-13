use assert_cmd::Command;
use std::fs;
use tempfile::tempdir;

fn get_golden_path(filename: &str) -> std::path::PathBuf {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.join("tests/golden").join(filename)
}

fn assert_output(cmd_args: &[&str], golden_filename: &str, capture_stderr: bool) {
    let output = Command::cargo_bin("spec_tool")
        .unwrap()
        .arg("--deterministic")
        .args(cmd_args)
        .output()
        .expect("Failed to run command");

    let mut actual = String::from_utf8_lossy(&output.stdout).to_string();
    if capture_stderr {
        actual.push_str(&String::from_utf8_lossy(&output.stderr));
    }

    let golden_path = get_golden_path(golden_filename);
    let expected = fs::read_to_string(&golden_path)
        .unwrap_or_else(|_| panic!("Failed to read golden file: {:?}", golden_path));

    // Normalize Windows/Unix newlines just in case
    let actual = actual.replace("\r\n", "\n");
    let expected = expected.replace("\r\n", "\n");

    if actual != expected {
        // Write actual to tmp for diffing (optional convenience)
        let _ = fs::write(format!("{}.actual", golden_path.display()), &actual);
        panic!("Output mismatch for {:?}. \nExpected first 100 chars:\n{}\nActual first 100 chars:\n{}", 
            golden_path, 
            &expected.chars().take(100).collect::<String>(),
            &actual.chars().take(100).collect::<String>()
        );
    }
}

fn assert_generated_file(cmd_args: &[&str], golden_filename: &str, output_arg_pos: Option<usize>) {
    let temp = tempdir().unwrap();
    let out_path = temp.path().join("output");
    let out_str = out_path.to_str().unwrap().to_string();

    let mut args = cmd_args.to_vec();
    if let Some(idx) = output_arg_pos {
        args.insert(idx, &out_str);
    } else {
        args.push(&out_str);
    }

    Command::cargo_bin("spec_tool")
        .unwrap()
        .arg("--deterministic")
        .args(&args)
        .assert(); // We don't check success here, just file generation

    if !out_path.exists() {
        panic!("Command failed to generate file at {}", out_str);
    }

    let actual = fs::read_to_string(&out_path).unwrap().replace("\r\n", "\n");
    let golden_path = get_golden_path(golden_filename);
    let expected = fs::read_to_string(&golden_path)
        .unwrap_or_else(|_| panic!("Failed to read golden file: {:?}", golden_path))
        .replace("\r\n", "\n");

    if actual != expected {
        panic!("File content mismatch for {:?}", golden_path);
    }
}

#[test]
fn golden_help() {
    assert_output(&["help"], "help.stdout", false);
}

#[test]
fn golden_registry() {
    // registry <out>
    assert_generated_file(&["registry"], "registry.md", Some(1));
}

#[test]
fn golden_traceability() {
    // traceability <out>
    assert_generated_file(&["traceability"], "traceability.md", Some(1));
}

#[test]
fn golden_bundle() {
    // bundle <out>
    assert_generated_file(&["bundle"], "NAIA_SPECS.md", Some(1));
}

#[test]
fn golden_stats() {
    assert_output(&["stats"], "stats.stdout", false);
}

#[test]
fn golden_coverage() {
    assert_output(&["coverage"], "coverage.stdout", false);
}

#[test]
fn golden_lint() {
    assert_output(&["lint"], "lint.stdout", true);
}

#[test]
fn golden_check_orphans() {
    assert_output(&["check-orphans"], "check-orphans.stdout", true);
}

#[test]
fn golden_check_refs() {
    assert_output(&["check-refs"], "check-refs.stdout", true);
}

#[test]
fn golden_validate() {
    assert_output(&["validate"], "validate.stdout", true);
}

#[test]
fn golden_adequacy() {
    assert_output(&["adequacy"], "adequacy.stdout", true);
}

#[test]
fn golden_verify_report() {
    // verify --contract connection-01 --write-report <out>
    assert_generated_file(
        &["verify", "--contract", "connection-01", "--write-report"], 
        "verify_report.md", 
        None // append
    );
}

// verify stdout is non-deterministic due to cargo output, so we don't check it against golden file.
// We only ensure report generation is correct.


