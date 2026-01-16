use std::collections::HashSet;
use std::path::PathBuf;
use walkdir::WalkDir;
use crate::util::{print_header, print_success, print_error};

/// Check that spec files and test files are properly paired 1:1 by stem
pub fn run_check_pairing(root: &PathBuf) -> anyhow::Result<usize> {
    print_header("Checking Spec/Test Pairing");

    let specs_dir = root.join("specs/contracts");
    let tests_dir = root.join("test/tests");

    // Collect spec stems
    let mut spec_stems = HashSet::new();
    for entry in WalkDir::new(&specs_dir).min_depth(1).max_depth(1) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }

        let path = entry.path();
        if let Some(ext) = path.extension() {
            if ext != "md" {
                continue;
            }
        } else {
            continue;
        }

        // Extract stem from filename like "01_connection_lifecycle.spec.md"
        if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
            // Skip non-numbered files
            if !filename.chars().next().map_or(false, |c| c.is_ascii_digit()) {
                continue;
            }

            // Remove .spec.md extension to get stem
            let stem = filename
                .strip_suffix(".spec.md")
                .unwrap_or(filename)
                .to_string();

            spec_stems.insert(stem);
        }
    }

    // Collect test stems
    let mut test_stems = HashSet::new();
    for entry in WalkDir::new(&tests_dir).min_depth(1).max_depth(1) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }

        let path = entry.path();
        if let Some(ext) = path.extension() {
            if ext != "rs" {
                continue;
            }
        } else {
            continue;
        }

        if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
            // Skip helper files
            if filename.starts_with('_') {
                continue;
            }

            // Skip non-numbered files
            if !filename.chars().next().map_or(false, |c| c.is_ascii_digit()) {
                continue;
            }

            // Remove .rs extension to get stem
            let stem = filename
                .strip_suffix(".rs")
                .unwrap_or(filename)
                .to_string();

            test_stems.insert(stem);
        }
    }

    // Find mismatches
    let mut error_count = 0;

    // Check for orphaned specs (no matching test)
    for spec_stem in &spec_stems {
        if !test_stems.contains(spec_stem) {
            print_error(&format!("Orphaned spec: {} (no matching test file test/tests/{}.rs)", spec_stem, spec_stem));
            error_count += 1;
        }
    }

    // Check for orphaned tests (no matching spec)
    for test_stem in &test_stems {
        if !spec_stems.contains(test_stem) {
            print_error(&format!("Orphaned test: {} (no matching spec file specs/contracts/{}.spec.md)", test_stem, test_stem));
            error_count += 1;
        }
    }

    println!();
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    if error_count == 0 {
        print_success(&format!("All {} spec/test pairs matched correctly!", spec_stems.len()));
    } else {
        print_error(&format!("Pairing check failed with {} errors", error_count));
    }

    Ok(error_count)
}
