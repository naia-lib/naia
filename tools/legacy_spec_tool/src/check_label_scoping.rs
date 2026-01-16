use std::path::PathBuf;
use std::fs;
use anyhow::Result;
use regex::Regex;
use walkdir::WalkDir;
use crate::util::{print_header, print_success, print_error};
use crate::label_extractor::extract_labels_from_file;

/// Check that labels in tests only reference contracts declared in their annotations
pub fn run_check_label_scoping(root: &PathBuf) -> Result<usize> {
    print_header("Checking Label Scoping (no cross-contract labels)");

    let tests_dir = root.join("test/tests");
    let mut error_count = 0;

    // Regex patterns
    let test_attr_re = Regex::new(r"^\s*#\[test\]\s*$").unwrap();
    let contract_annotation_re = Regex::new(r"^\s*///\s*Contract:\s*(.+)").unwrap();
    let contract_id_re = Regex::new(r"\[([a-z][a-z0-9-]*-[0-9]+(?:-[a-z]+|[a-z]*))\]").unwrap();
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

        // Parse line-by-line to find test annotations
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

                // Look backward for /// Contract: annotation
                let mut contract_ids = Vec::new();
                let mut j = i;
                while j > 0 {
                    j -= 1;
                    let line = lines[j].trim();

                    // If we hit a non-comment/non-attribute line, stop
                    if !line.starts_with("///") && !line.starts_with("#[") && !line.is_empty() {
                        break;
                    }

                    // Check for Contract annotation
                    if let Some(caps) = contract_annotation_re.captures(lines[j]) {
                        let contracts_part = caps.get(1).unwrap().as_str();

                        // Extract all contract IDs from brackets
                        for m in contract_id_re.find_iter(contracts_part) {
                            let contract_id = m.as_str().trim_matches(|c| c == '[' || c == ']').to_string();
                            contract_ids.push(contract_id);
                        }
                        break;
                    }

                    // Don't search too far back
                    if i - j > 20 {
                        break;
                    }
                }

                // Now check if labels in this test reference only annotated contracts
                if let Some(labels) = label_result.labels_by_test.get(&fn_name) {
                    for label in labels {
                        // Check if label starts with a contract ID prefix (format: <cid>. or <cid>:)
                        if let Some(label_contract_id) = extract_contract_id_from_label(label) {
                            // Check if this contract ID is in the test's annotation
                            if !contract_ids.contains(&label_contract_id) {
                                print_error(&format!(
                                    "{}:{} - Test function '{}' has label '{}' referencing contract '{}' which is not in its annotation. Allowed contracts: [{}]",
                                    filename,
                                    i + 1, // Line number of #[test]
                                    fn_name,
                                    label,
                                    label_contract_id,
                                    contract_ids.join(", ")
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
        print_success("All labels properly scoped to annotated contracts");
    } else {
        print_error(&format!("Label scoping check failed with {} errors", error_count));
    }

    Ok(error_count)
}

/// Extract contract ID from a label if it follows the format <cid>. or <cid>:
/// Examples:
///   "connection-01.t1: connects" -> Some("connection-01")
///   "messaging-15-a: sends message" -> Some("messaging-15-a")
///   "just a plain label" -> None
fn extract_contract_id_from_label(label: &str) -> Option<String> {
    // Pattern matches: contract-id followed by either '.' or ':'
    // Supports formats like "connection-01", "messaging-15-a", "common-02a"
    let re = Regex::new(r"^([a-z][a-z0-9-]*-[0-9]+(?:-[a-z]+|[a-z]*))[.:]").unwrap();

    re.captures(label)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_fixture(test_content: &str) -> (TempDir, PathBuf) {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_path_buf();

        // Create directory structure
        let tests_dir = root.join("test/tests");
        std::fs::create_dir_all(&tests_dir).unwrap();

        // Write test file
        let test_file = tests_dir.join("01_test.rs");
        let mut file = std::fs::File::create(&test_file).unwrap();
        file.write_all(test_content.as_bytes()).unwrap();

        (temp_dir, root)
    }

    #[test]
    fn test_label_references_annotated_contract() {
        let content = r#"
/// Contract: [connection-01]
#[test]
fn test_connection() {
    scenario.spec_expect("connection-01.t1: connects", |ctx| Some(()));
}
"#;
        let (_temp, root) = create_test_fixture(content);
        let result = run_check_label_scoping(&root).unwrap();
        assert_eq!(result, 0, "Label referencing annotated contract should pass");
    }

    #[test]
    fn test_label_references_non_annotated_contract() {
        let content = r#"
/// Contract: [connection-01]
#[test]
fn test_connection() {
    scenario.spec_expect("messaging-01.t1: sends message", |ctx| Some(()));
}
"#;
        let (_temp, root) = create_test_fixture(content);
        let result = run_check_label_scoping(&root).unwrap();
        assert_eq!(result, 1, "Label referencing non-annotated contract should fail");
    }

    #[test]
    fn test_multiple_contracts_all_valid() {
        let content = r#"
/// Contract: [connection-01], [messaging-01]
#[test]
fn test_multiple() {
    scenario.spec_expect("connection-01.t1: connects", |ctx| Some(()));
    scenario.spec_expect("messaging-01.t1: sends message", |ctx| Some(()));
}
"#;
        let (_temp, root) = create_test_fixture(content);
        let result = run_check_label_scoping(&root).unwrap();
        assert_eq!(result, 0, "All labels referencing annotated contracts should pass");
    }

    #[test]
    fn test_multiple_contracts_one_invalid() {
        let content = r#"
/// Contract: [connection-01], [messaging-01]
#[test]
fn test_mixed() {
    scenario.spec_expect("connection-01.t1: connects", |ctx| Some(()));
    scenario.spec_expect("transport-01.t1: sends packet", |ctx| Some(()));
}
"#;
        let (_temp, root) = create_test_fixture(content);
        let result = run_check_label_scoping(&root).unwrap();
        assert_eq!(result, 1, "One invalid label should cause failure");
    }

    #[test]
    fn test_label_without_contract_prefix() {
        let content = r#"
/// Contract: [connection-01]
#[test]
fn test_plain_label() {
    scenario.spec_expect("just a plain label", |ctx| Some(()));
}
"#;
        let (_temp, root) = create_test_fixture(content);
        let result = run_check_label_scoping(&root).unwrap();
        assert_eq!(result, 0, "Labels without contract prefix should be allowed");
    }

    #[test]
    fn test_colon_format_label() {
        let content = r#"
/// Contract: [messaging-15-a]
#[test]
fn test_colon_format() {
    scenario.spec_expect("messaging-15-a: sends message", |ctx| Some(()));
}
"#;
        let (_temp, root) = create_test_fixture(content);
        let result = run_check_label_scoping(&root).unwrap();
        assert_eq!(result, 0, "Colon format labels should work");
    }

    #[test]
    fn test_extract_contract_id_from_label() {
        assert_eq!(
            extract_contract_id_from_label("connection-01.t1: connects"),
            Some("connection-01".to_string())
        );
        assert_eq!(
            extract_contract_id_from_label("messaging-15-a: sends"),
            Some("messaging-15-a".to_string())
        );
        assert_eq!(
            extract_contract_id_from_label("common-02a.t1: test"),
            Some("common-02a".to_string())
        );
        assert_eq!(
            extract_contract_id_from_label("just a plain label"),
            None
        );
    }
}
