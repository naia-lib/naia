use std::path::PathBuf;
use std::fs;
use anyhow::Result;
use regex::Regex;
use walkdir::WalkDir;
use crate::util::{print_header, print_success, print_error};

/// Check that every #[test] function has a valid /// Contract: [...] annotation
pub fn run_check_annotations(root: &PathBuf) -> Result<usize> {
    print_header("Checking Per-Test Contract Annotations");

    let tests_dir = root.join("test/tests");
    let mut error_count = 0;

    // Regex patterns
    let test_attr_re = Regex::new(r"^\s*#\[test\]\s*$").unwrap();
    let contract_annotation_re = Regex::new(r"^\s*///\s*Contract:\s*(.+)").unwrap();
    // Pattern supports both "common-02a" and "messaging-15-a" formats
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
        let lines: Vec<&str> = content.lines().collect();

        let mut i = 0;
        while i < lines.len() {
            // Look for #[test] attribute
            if test_attr_re.is_match(lines[i]) {
                // Found a test attribute, now validate it has a proper contract annotation
                let test_line = i + 1; // 1-based line number

                // Look backward for /// Contract: annotation (should be immediately before #[test])
                let mut found_annotation = false;
                let mut annotation_line_idx = 0;
                let mut contract_ids = Vec::new();

                // Search backwards through doc comments
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
                        found_annotation = true;
                        annotation_line_idx = j;
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

                // Now find the function name
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

                // Validate the annotation
                if !found_annotation {
                    print_error(&format!(
                        "{}:{} - Test function '{}' missing /// Contract: [...] annotation",
                        filename, test_line, fn_name
                    ));
                    error_count += 1;
                } else if contract_ids.is_empty() {
                    print_error(&format!(
                        "{}:{} - Test function '{}' has empty contract list (must have at least one contract ID)",
                        filename, annotation_line_idx + 1, fn_name
                    ));
                    error_count += 1;
                } else {
                    // Check for duplicates
                    let mut seen = std::collections::HashSet::new();
                    let mut duplicates = Vec::new();

                    for contract_id in &contract_ids {
                        if !seen.insert(contract_id) {
                            duplicates.push(contract_id.clone());
                        }
                    }

                    if !duplicates.is_empty() {
                        print_error(&format!(
                            "{}:{} - Test function '{}' has duplicate contract IDs: [{}]",
                            filename, annotation_line_idx + 1, fn_name,
                            duplicates.join(", ")
                        ));
                        error_count += 1;
                    }
                }
            }
            i += 1;
        }
    }

    println!();
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    if error_count == 0 {
        print_success("All test functions have valid contract annotations");
    } else {
        print_error(&format!("Contract annotation check failed with {} errors", error_count));
    }

    Ok(error_count)
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
    fn test_valid_annotation() {
        let content = r#"
/// Contract: [connection-01]
#[test]
fn test_connection() {
    assert!(true);
}
"#;
        let (_temp, root) = create_test_fixture(content);
        let result = run_check_annotations(&root).unwrap();
        assert_eq!(result, 0, "Valid annotation should pass");
    }

    #[test]
    fn test_multiple_contracts() {
        let content = r#"
/// Contract: [connection-01], [connection-02]
#[test]
fn test_multiple() {
    assert!(true);
}
"#;
        let (_temp, root) = create_test_fixture(content);
        let result = run_check_annotations(&root).unwrap();
        assert_eq!(result, 0, "Multiple contracts should pass");
    }

    #[test]
    fn test_missing_annotation() {
        let content = r#"
#[test]
fn test_no_annotation() {
    assert!(true);
}
"#;
        let (_temp, root) = create_test_fixture(content);
        let result = run_check_annotations(&root).unwrap();
        assert_eq!(result, 1, "Missing annotation should fail");
    }

    #[test]
    fn test_empty_annotation() {
        let content = r#"
/// Contract:
#[test]
fn test_empty() {
    assert!(true);
}
"#;
        let (_temp, root) = create_test_fixture(content);
        let result = run_check_annotations(&root).unwrap();
        assert_eq!(result, 1, "Empty annotation should fail");
    }

    #[test]
    fn test_duplicate_contracts() {
        let content = r#"
/// Contract: [connection-01], [connection-01]
#[test]
fn test_duplicates() {
    assert!(true);
}
"#;
        let (_temp, root) = create_test_fixture(content);
        let result = run_check_annotations(&root).unwrap();
        assert_eq!(result, 1, "Duplicate contracts should fail");
    }

    #[test]
    fn test_contract_with_hyphen_suffix() {
        let content = r#"
/// Contract: [messaging-15-a]
#[test]
fn test_hyphen_suffix() {
    assert!(true);
}
"#;
        let (_temp, root) = create_test_fixture(content);
        let result = run_check_annotations(&root).unwrap();
        assert_eq!(result, 0, "Contract with hyphen-letter suffix should pass");
    }
}
