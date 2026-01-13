use assert_cmd::Command;
use std::fs;
use std::path::{Path, PathBuf};
use regex::Regex;
use tempfile::tempdir;

fn get_workspace_root() -> PathBuf {
     let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
     // manifest_dir is .../specs. Parent is workspace root.
     manifest_dir.parent().expect("Failed to find workspace root").to_path_buf() 
}

fn get_all_contracts(root: &Path) -> Vec<String> {
    let mut contracts = Vec::new();
    // Match strict format ### [id]
    // Also match > id (MUST
    // Also match **id**:
    // Also match ### id —
    
    // Actually, let's just use the strict brackets one for the test, 
    // because logic in registry.rs is complex and handles all forms.
    // If the test only checks for strict brackets, we might miss testing the other forms.
    // BUT common-02a is `### [common-02a] — ...` so it should match the bracket regex.
    
    // The previous regex was `### \[(.*?)\]`.
    // In registry.rs we updated it to `### \[[a-z-]+-[0-9]+[a-z-]*\]`.
    // Let's use the same more permissive regex here to capture what we expect.
    let re = Regex::new(r"### \[(.*?)\]").unwrap();
    
    let contracts_dir = root.join("specs/contracts");
    for entry in fs::read_dir(contracts_dir).expect("Failed to read specs/contracts") {
        let entry = entry.unwrap();
        if entry.path().extension().map_or(false, |e| e == "md") {
            let content = fs::read_to_string(entry.path()).unwrap();
            for cap in re.captures_iter(&content) {
                contracts.push(cap[1].to_string());
            }
        }
    }
    contracts
}

#[test]
fn registry_includes_all_contracts() {
    let root = get_workspace_root();
    let contracts = get_all_contracts(&root);
    assert!(!contracts.is_empty(), "Found no contracts in specs directory");

    let temp = tempdir().unwrap();
    let out_path = temp.path().join("registry.md");

    Command::cargo_bin("spec_tool")
        .unwrap()
        .current_dir(&root)
        .arg("registry")
        .arg(out_path.to_str().unwrap())
        .assert()
        .success();

    let content = fs::read_to_string(&out_path).unwrap();
    
    for contract in contracts {
        assert!(content.contains(&contract), "Registry missing contract: {}", contract);
    }
}

#[test]
fn traceability_includes_all_contracts() {
    let root = get_workspace_root();
    let contracts = get_all_contracts(&root);
    
    // Traceability requires CONTRACT_REGISTRY.md to exist to know which contracts to include.
    // The `spec_tool traceability` command reads `specs/generated/CONTRACT_REGISTRY.md`.
    // Since we are running in the workspace root, it will read the REAL file.
    // We should ensure the real file is up to date or at least exists.
    // Ideally, we'd mock it, but the tool is hardcoded to read "specs/generated/CONTRACT_REGISTRY.md".
    
    // Let's ensure registry is fresh befor running traceability test
    Command::cargo_bin("spec_tool")
        .unwrap()
        .current_dir(&root)
        .arg("registry")
        .assert()
        .success();

    let temp = tempdir().unwrap();
    let out_path = temp.path().join("trace.md");

    Command::cargo_bin("spec_tool")
        .unwrap()
        .current_dir(&root)
        .arg("traceability")
        .arg(out_path.to_str().unwrap())
        .assert()
        .success();

    let content = fs::read_to_string(&out_path).unwrap();
    
    for contract in contracts {
        assert!(content.contains(&contract), "Traceability missing contract: {}", contract);
    }
}

#[test]
fn guardrail_no_legacy_scripts() {
    let root = get_workspace_root();
    let forbidden = [
        "specs/spec_tool.sh",
        "specs/spec_tool_test.sh",
        "specs/spec_index.py",
        "specs/tests/baseline",
        "specs/tests/common",
    ];
    
    for path in forbidden {
        assert!(!root.join(path).exists(), "Legacy file/folder must not exist: {}", path);
    }
}

#[test]
fn verify_failures() {
    let root = get_workspace_root();
    
    // We can't easily induce failure in the real workspace without modifying files.
    // But we check that verify runs successfully on clean state.
    // If we want to test failure, we need a separate "failing" workspace fixture.
    // The plan says: "Verify exits nonzero if lint/refs/orphans fail (force a small controlled failing fixture for one test)"
    
    // Creating a failing fixture is complicated (needs folder structure).
    // Let's create a minimal structure in temp dir.
    
    let temp = tempdir().unwrap();
    let spec_dir = temp.path().join("specs/contracts");
    fs::create_dir_all(&spec_dir).unwrap();
    
    // Create invalid spec (bad title format "Spec:")
    // NOTE: lint warnings do not cause failure in the tool currently (only errors do, and "Spec:" title is a warning)
    // To force failure, we need to trigger an error. But lint only has warnings?
    // Let's check src/lint.rs. It seems issues vs warnings.
    // "Spec:" is a warning.
    // "Mixed terminology" is a warning.
    // Let's create a "validate" failure using orphan check or refs?
    // 'validate' calls 'lint' then 'check-refs'.
    // 'check-refs' produces ERRORS on missing file.
    
    fs::write(spec_dir.join("bad.md"), "# Spec: Bad Title\n\nRefers to `missing.md`").unwrap();
    
    Command::cargo_bin("spec_tool")
        .unwrap()
        .current_dir(temp.path())
        .arg("validate")
        .assert()
        .failure(); // Should fail due to missing reference in check-refs
}
