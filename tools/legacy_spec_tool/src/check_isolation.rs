use std::path::PathBuf;
use crate::index::Index;
use crate::util::{print_header, print_success, print_error};

/// Check that test suites only reference contracts from their paired spec file (suite isolation)
pub fn run_check_isolation(root: &PathBuf) -> anyhow::Result<usize> {
    print_header("Checking Suite Isolation");

    let index = Index::build(root.clone())?;
    let mut error_count = 0;

    // For each test file, check that it only references contracts from its paired spec
    for (test_filename, test_file) in &index.tests {
        // Skip helper files (start with '_')
        if test_filename.starts_with('_') {
            continue;
        }

        // Extract stem from test filename (e.g., "01_connection_lifecycle.rs" -> "01_connection_lifecycle")
        let test_stem = test_filename
            .strip_suffix(".rs")
            .unwrap_or(test_filename);

        // Find the paired spec file (e.g., "01_connection_lifecycle.spec.md")
        let spec_filename = format!("{}.spec.md", test_stem);

        if let Some(spec_file) = index.specs.get(&spec_filename) {
            // Get all contracts defined in the paired spec file
            let allowed_contracts: std::collections::HashSet<_> = spec_file.contracts.iter().cloned().collect();

            // Check each contract referenced in the test file
            for (contract_id, _test_fns) in &test_file.covered_contracts {
                if !allowed_contracts.contains(contract_id) {
                    print_error(&format!(
                        "Suite isolation violation in {}: references contract [{}] which is not defined in paired spec {}",
                        test_filename, contract_id, spec_filename
                    ));
                    error_count += 1;
                }
            }
        } else {
            // This would be caught by pairing check, but report it here too
            print_error(&format!(
                "Test file {} has no paired spec file {}",
                test_filename, spec_filename
            ));
            error_count += 1;
        }
    }

    println!();
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    if error_count == 0 {
        print_success("All test suites respect isolation (only reference contracts from paired specs)");
    } else {
        print_error(&format!("Suite isolation check failed with {} errors", error_count));
    }

    Ok(error_count)
}
