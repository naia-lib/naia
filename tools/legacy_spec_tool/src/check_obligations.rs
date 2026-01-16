use std::path::PathBuf;
use std::fs;
use std::collections::HashMap;
use anyhow::Result;
use regex::Regex;
use walkdir::WalkDir;
use crate::util::{print_header, print_success, print_error};
use crate::label_extractor::extract_labels_from_file;

/// Check that labels referencing obligations (format: <cid>.tN:) actually exist in specs
pub fn run_check_obligations(root: &PathBuf) -> Result<usize> {
    print_header("Checking Obligation Existence");

    let tests_dir = root.join("test/tests");
    let specs_dir = root.join("specs/contracts");
    let mut error_count = 0;

    // Build a map of contract -> obligations from all spec files
    let obligation_map = build_obligation_map(&specs_dir)?;

    // Regex patterns for parsing test files
    let test_attr_re = Regex::new(r"^\s*#\[test\]\s*$").unwrap();
    let fn_def_re = Regex::new(r"^\s*(?:(?:pub|async|unsafe|extern)\s+)*fn\s+([a-z_][a-z0-9_]*)\s*\(").unwrap();

    for entry in WalkDir::new(&tests_dir).into_iter().filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() || !entry.path().extension().map_or(false, |e| e == "rs") {
            continue;
        }

        // Skip helper files (start with '_')
        let filename = entry.file_name().to_string_lossy().to_string();
        if filename.starts_with('_') {
            continue;
        }

        let path = entry.path();
        let content = fs::read_to_string(path)?;

        // Extract labels using AST parser
        let label_result = extract_labels_from_file(&content);

        // Parse line-by-line to find test functions
        let lines: Vec<&str> = content.lines().collect();
        let mut i = 0;

        while i < lines.len() {
            // Look for #[test] attribute
            if test_attr_re.is_match(lines[i]) {
                // Find the function name
                let mut fn_name = String::from("<unknown>");
                let mut k = i + 1;
                while k < lines.len() {
                    if let Some(fn_caps) = fn_def_re.captures(lines[k]) {
                        fn_name = fn_caps.get(1).unwrap().as_str().to_string();
                        break;
                    }
                    k += 1;
                    if k - i > 10 {
                        break;
                    }
                }

                // Check if labels in this test reference existing obligations
                if let Some(labels) = label_result.labels_by_test.get(&fn_name) {
                    for label in labels {
                        // Check if label is in obligation format: <cid>.tN:
                        if let Some((contract_id, obligation_id)) = extract_obligation_reference(label) {
                            // Check if this obligation exists in the spec
                            if let Some(obligations) = obligation_map.get(&contract_id) {
                                if !obligations.contains(&obligation_id) {
                                    print_error(&format!(
                                        "{}:{} - Test function '{}' has label '{}' referencing obligation '{}' which does not exist in contract '{}'. Available obligations: [{}]",
                                        filename,
                                        i + 1, // Line number of #[test]
                                        fn_name,
                                        label,
                                        obligation_id,
                                        contract_id,
                                        obligations.join(", ")
                                    ));
                                    error_count += 1;
                                }
                            } else {
                                print_error(&format!(
                                    "{}:{} - Test function '{}' has label '{}' referencing contract '{}' which does not exist in any spec",
                                    filename,
                                    i + 1,
                                    fn_name,
                                    label,
                                    contract_id
                                ));
                                error_count += 1;
                            }
                        }
                    }
                }
            }
            i += 1;
        }
    }

    println!();
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    if error_count == 0 {
        print_success("All obligation references are valid");
    } else {
        print_error(&format!("Obligation check failed with {} errors", error_count));
    }

    Ok(error_count)
}

/// Build a map of contract_id -> list of obligation IDs from spec files
fn build_obligation_map(specs_dir: &PathBuf) -> Result<HashMap<String, Vec<String>>> {
    let mut map: HashMap<String, Vec<String>> = HashMap::new();

    let contract_heading_re = Regex::new(r"^###\s+\[([a-z][a-z0-9-]*-[0-9]+(?:-[a-z]+|[a-z]*))\]").unwrap();
    let obligations_heading_re = Regex::new(r"^\*\*Obligations:\*\*").unwrap();
    let obligation_item_re = Regex::new(r"^-\s+\*\*(t[0-9]+)\*\*:").unwrap();

    for entry in WalkDir::new(specs_dir).into_iter().filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() || !entry.path().extension().map_or(false, |e| e == "md") {
            continue;
        }

        let content = fs::read_to_string(entry.path())?;
        let lines: Vec<&str> = content.lines().collect();

        let mut current_contract: Option<String> = None;
        let mut in_obligations_section = false;

        for line in lines {
            // Check for contract heading
            if let Some(caps) = contract_heading_re.captures(line) {
                current_contract = Some(caps.get(1).unwrap().as_str().to_string());
                in_obligations_section = false;
                continue;
            }

            // Check for Obligations section
            if obligations_heading_re.is_match(line) {
                in_obligations_section = true;
                continue;
            }

            // If we're in an obligations section, extract obligation IDs
            if in_obligations_section {
                if let Some(caps) = obligation_item_re.captures(line) {
                    let obligation_id = caps.get(1).unwrap().as_str().to_string();
                    if let Some(contract_id) = &current_contract {
                        map.entry(contract_id.clone())
                            .or_insert_with(Vec::new)
                            .push(obligation_id);
                    }
                } else if line.starts_with("##") || line.starts_with("**") && !line.starts_with("- ") {
                    // Hit a new section heading, exit obligations section
                    in_obligations_section = false;
                }
            }
        }
    }

    Ok(map)
}

/// Extract obligation reference from a label
/// Format: <cid>.tN: <description>
/// Returns: Some((contract_id, obligation_id)) or None
fn extract_obligation_reference(label: &str) -> Option<(String, String)> {
    // Pattern matches: contract-id.tN: ...
    let re = Regex::new(r"^([a-z][a-z0-9-]*-[0-9]+(?:-[a-z]+|[a-z]*))\.(t[0-9]+):").unwrap();

    re.captures(label).and_then(|caps| {
        let contract_id = caps.get(1)?.as_str().to_string();
        let obligation_id = caps.get(2)?.as_str().to_string();
        Some((contract_id, obligation_id))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_fixture(spec_content: &str, test_content: &str) -> (TempDir, PathBuf) {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_path_buf();

        // Create directory structure
        let specs_dir = root.join("specs/contracts");
        let tests_dir = root.join("test/tests");
        std::fs::create_dir_all(&specs_dir).unwrap();
        std::fs::create_dir_all(&tests_dir).unwrap();

        // Write spec file
        let spec_file = specs_dir.join("01_test.spec.md");
        let mut file = std::fs::File::create(&spec_file).unwrap();
        file.write_all(spec_content.as_bytes()).unwrap();

        // Write test file
        let test_file = tests_dir.join("01_test.rs");
        let mut file = std::fs::File::create(&test_file).unwrap();
        file.write_all(test_content.as_bytes()).unwrap();

        (temp_dir, root)
    }

    #[test]
    fn test_existing_obligation_passes() {
        let spec = r#"
### [connection-01] — Test contract

**Obligations:**
- **t1**: Client connects successfully
- **t2**: Server accepts connection
"#;
        let test = r#"
/// Contract: [connection-01]
#[test]
fn test_connection() {
    scenario.spec_expect("connection-01.t1: client connects", |ctx| Some(()));
}
"#;
        let (_temp, root) = create_test_fixture(spec, test);
        let result = run_check_obligations(&root).unwrap();
        assert_eq!(result, 0, "Existing obligation should pass");
    }

    #[test]
    fn test_missing_obligation_fails() {
        let spec = r#"
### [connection-01] — Test contract

**Obligations:**
- **t1**: Client connects successfully
"#;
        let test = r#"
/// Contract: [connection-01]
#[test]
fn test_connection() {
    scenario.spec_expect("connection-01.t2: server accepts", |ctx| Some(()));
}
"#;
        let (_temp, root) = create_test_fixture(spec, test);
        let result = run_check_obligations(&root).unwrap();
        assert_eq!(result, 1, "Missing obligation should fail");
    }

    #[test]
    fn test_label_without_obligation_format_passes() {
        let spec = r#"
### [connection-01] — Test contract

**Obligations:**
- **t1**: Client connects successfully
"#;
        let test = r#"
/// Contract: [connection-01]
#[test]
fn test_connection() {
    scenario.spec_expect("just a plain label", |ctx| Some(()));
}
"#;
        let (_temp, root) = create_test_fixture(spec, test);
        let result = run_check_obligations(&root).unwrap();
        assert_eq!(result, 0, "Labels without obligation format should pass");
    }

    #[test]
    fn test_colon_format_without_dot_passes() {
        let spec = r#"
### [connection-01] — Test contract

**Obligations:**
- **t1**: Client connects successfully
"#;
        let test = r#"
/// Contract: [connection-01]
#[test]
fn test_connection() {
    scenario.spec_expect("connection-01: general label", |ctx| Some(()));
}
"#;
        let (_temp, root) = create_test_fixture(spec, test);
        let result = run_check_obligations(&root).unwrap();
        assert_eq!(result, 0, "Labels with colon but no .tN should pass");
    }

    #[test]
    fn test_extract_obligation_reference() {
        assert_eq!(
            extract_obligation_reference("connection-01.t1: connects"),
            Some(("connection-01".to_string(), "t1".to_string()))
        );
        assert_eq!(
            extract_obligation_reference("messaging-15-a.t2: sends"),
            Some(("messaging-15-a".to_string(), "t2".to_string()))
        );
        assert_eq!(
            extract_obligation_reference("connection-01: plain label"),
            None
        );
        assert_eq!(
            extract_obligation_reference("just a label"),
            None
        );
    }

    #[test]
    fn test_multiple_obligations() {
        let spec = r#"
### [connection-01] — Test contract

**Obligations:**
- **t1**: First obligation
- **t2**: Second obligation
- **t3**: Third obligation
"#;
        let test = r#"
/// Contract: [connection-01]
#[test]
fn test_connection() {
    scenario.spec_expect("connection-01.t1: first", |ctx| Some(()));
    scenario.spec_expect("connection-01.t2: second", |ctx| Some(()));
    scenario.spec_expect("connection-01.t3: third", |ctx| Some(()));
}
"#;
        let (_temp, root) = create_test_fixture(spec, test);
        let result = run_check_obligations(&root).unwrap();
        assert_eq!(result, 0, "All existing obligations should pass");
    }
}
