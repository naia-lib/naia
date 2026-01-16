use std::path::PathBuf;
use std::fs;
use anyhow::Result;
use walkdir::WalkDir;
use crate::util::{print_header, print_success, print_error};
use crate::label_extractor;

/// Check that all spec_expect/expect_msg calls use string literals for labels
pub fn run_check_label_literals(root: &PathBuf) -> Result<usize> {
    print_header("Checking Label String Literals");

    let tests_dir = root.join("test/tests");
    let mut error_count = 0;

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

        // Use AST-based extraction
        let result = label_extractor::extract_labels_from_file(&content);

        // Report any errors
        for error in result.errors {
            print_error(&format!(
                "{} - Test '{}': {}",
                filename, error.test_fn_name, error.message
            ));
            error_count += 1;
        }
    }

    println!();
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    if error_count == 0 {
        print_success("All labels use string literals");
    } else {
        print_error(&format!("Label literal check failed with {} errors", error_count));
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
    fn test_string_literal_passes() {
        let content = r#"
#[test]
fn test_example() {
    scenario.spec_expect("connection-01.t1: connects", |ctx| Some(()));
}
"#;
        let (_temp, root) = create_test_fixture(content);
        let result = run_check_label_literals(&root).unwrap();
        assert_eq!(result, 0, "String literal labels should pass");
    }

    #[test]
    fn test_non_literal_fails() {
        let content = r#"
#[test]
fn test_example() {
    let label = "connection-01.t1: connects";
    scenario.spec_expect(label, |ctx| Some(()));
}
"#;
        let (_temp, root) = create_test_fixture(content);
        let result = run_check_label_literals(&root).unwrap();
        assert_eq!(result, 1, "Non-literal label should fail");
    }

    #[test]
    fn test_multiline_string_literal() {
        let content = r#"
#[test]
fn test_example() {
    scenario.spec_expect(
        "connection-01.t1: connects",
        |ctx| Some(())
    );
}
"#;
        let (_temp, root) = create_test_fixture(content);
        let result = run_check_label_literals(&root).unwrap();
        assert_eq!(result, 0, "Multiline string literal should pass");
    }

    #[test]
    fn test_method_chain() {
        let content = r#"
#[test]
fn test_example() {
    scenario.until(Duration::from_secs(5))
        .spec_expect("connection-01.t1: connects", |ctx| Some(()));
}
"#;
        let (_temp, root) = create_test_fixture(content);
        let result = run_check_label_literals(&root).unwrap();
        assert_eq!(result, 0, "Method chain with string literal should pass");
    }
}
