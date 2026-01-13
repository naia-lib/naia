use std::path::Path;
use std::fs;
use std::process::Command;
use regex::Regex;
use crate::util::{print_header, print_info, print_success, print_error};
use crate::{validate, check_orphans, coverage, traceability, index::Index};

pub fn run_verify(
    root: &Path, 
    contract: Option<String>, 
    strict_orphans: bool, 
    strict_coverage: bool,
    _full_report: bool,
    write_report: Option<String>,
    deterministic: bool
) -> anyhow::Result<usize> {
    print_header("Naia Verification Pipeline");

    let mut total_errors: usize = 0;
    let test_status: &str; // Initialized later

    // Step 1: Validate spec structure
    print_info("Running: validate (spec structure)");
    let v_errors = validate::run_validate(&root.to_path_buf())?;
    if v_errors > 0 {
        print_error("Spec validation failed");
        return Ok(1); // Stop early with error count 1 if validation fails
    }

    // Step 2: Check orphans
    println!("");
    print_info("Running: check-orphans");
    let orphan_count = check_orphans::run_check_orphans(&root.to_path_buf())?;
    
    if strict_orphans && orphan_count > 0 {
        print_error(&format!("Strict orphan check failed ({} orphans)", orphan_count));
        return Ok(1); // Stop early
    }

    // Step 3: Run tests (targeted or full)
    println!("");
    if let Some(target_contract) = &contract {
        print_info(&format!("Running: targeted tests for contract [{}]", target_contract));
        
        // Find test files containing this contract
        let test_files = find_test_files_for_contract(root, target_contract);
        if test_files.is_empty() {
             print_error(&format!("No test files found for contract [{}]", target_contract));
             print_info("Contract may be uncovered. Run './spec_tool.sh coverage' to check.");
             return Ok(1);
        }

        print_info(&format!("Found contract in test files: {}", test_files.join(", ")));

        let mut test_failed = false;
        for test_file in &test_files {
            println!("");
            print_info(&format!("Running: cargo test -p naia-test --test {} -- --nocapture --test-threads=1", test_file));
            let status = Command::new("cargo")
                .args(["test", "-p", "naia-test", "--test", test_file, "--", "--nocapture", "--test-threads=1"])
                .status()?;
            if !status.success() {
                test_failed = true;
            }
        }

        if test_failed {
            test_status = "FAIL";
            total_errors += 1;
        } else {
            test_status = "PASS";
        }
        
        // Skip coverage/traceability unless full report?
        // Wait, bash script had return $total_errors here if full_report=0.
        // We handle full_report implicitly by continuing.
    } else {
        print_info("Running: cargo test -p naia-test -- --nocapture --test-threads=1");
        let status = Command::new("cargo")
            .args(["test", "-p", "naia-test", "--", "--nocapture", "--test-threads=1"])
            .status()?;
        if status.success() {
            test_status = "PASS";
        } else {
            test_status = "FAIL";
            total_errors += 1;
        }
    }

    // Step 4: Coverage analysis
    println!("");
    print_info("Running: coverage");
    let index = Index::build(root.to_path_buf())?;
    let (covered_count, total_count) = coverage::run_coverage(&index)?;
    let coverage_pct = if total_count > 0 { covered_count * 100 / total_count } else { 0 };

    if strict_coverage {
        let uncovered_count = total_count - covered_count;
        if uncovered_count > 0 {
            print_error(&format!("Strict coverage check failed ({} uncovered)", uncovered_count));
            total_errors += 1;
        }
    }

    // Step 5: Traceability
    println!("");
    print_info("Running: traceability (regenerating matrix)");
    traceability::run_traceability(root, None, true, deterministic)?;

    // Step 6: Final summary
    println!("");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    if total_errors == 0 {
        print_success("VERIFY: PASS");
    } else {
        print_error("VERIFY: FAIL");
    }

    println!("");
    println!("Summary:");
    println!("  tests:             {}", test_status);
    println!("  coverage:          {}% ({}/{})", coverage_pct, covered_count, total_count);

    let mut uncovered = Vec::new();
    let mut all_ids: Vec<_> = index.contracts.keys().collect();
    all_ids.sort();
    for id in all_ids {
        if !index.contract_to_test_files.contains_key(id) {
            uncovered.push(id.clone());
        }
    }

    if !uncovered.is_empty() {
        println!("  uncovered:         {} contracts", uncovered.len());
        if uncovered.len() <= 30 {
            println!("");
            println!("Uncovered contracts:");
            for id in &uncovered {
                println!("    {}", id);
            }
        }
    } else {
        println!("  uncovered:         0 contracts");
    }

    if orphan_count > 0 {
        if strict_orphans {
            println!("  orphans:           {} (strict mode - FAILED)", orphan_count);
        } else {
            println!("  orphans:           {} (non-strict)", orphan_count);
        }
    } else {
        println!("  orphans:           0");
    }

    if let Some(path) = write_report {
        use chrono::Utc;
        let overall = if total_errors == 0 { "PASS" } else { "FAIL" };
        let mut report = String::new();
        let timestamp = if deterministic {
            "1970-01-01 00:00 UTC".to_string()
        } else {
             Utc::now().format("%Y-%m-%d %H:%M UTC").to_string()
        };
        report.push_str("# Naia Verification Report\n\n");
        report.push_str(&format!("**Generated:** {}\n\n", timestamp));
        report.push_str("## Summary\n\n");
        report.push_str(&format!("- **Overall:** {}\n", overall));
        report.push_str(&format!("- **Tests:** {}\n", test_status));
        report.push_str(&format!("- **Coverage:** {}% ({}/{})\n", coverage_pct, covered_count, total_count));
        report.push_str(&format!("- **Uncovered:** {} contracts\n", uncovered.len()));
        report.push_str(&format!("- **Orphans:** {}\n\n", orphan_count));
        report.push_str("## Uncovered Contracts\n\n");
        
        if !uncovered.is_empty() {
            for id in &uncovered {
                report.push_str(&format!("- {}\n", id));
            }
        } else {
            report.push_str("All contracts covered!\n");
        }
        
        fs::write(path, report)?;
    }

    Ok(total_errors)
}

fn find_test_files_for_contract(root: &Path, id: &str) -> Vec<String> {
    let test_dir = root.join("test/tests");
    let mut files = Vec::new();
    if let Ok(entries) = fs::read_dir(test_dir) {
        for entry in entries {
            if let Ok(entry) = entry {
                let path = entry.path();
                if path.is_file() && path.extension().map_or(false, |e| e == "rs") {
                    if let Ok(content) = fs::read_to_string(&path) {
                        let re = Regex::new(&format!(r"Contract:.*\[{}\]", regex::escape(id))).unwrap();
                        if re.is_match(&content) {
                            if let Some(name) = path.file_stem() {
                                files.push(name.to_string_lossy().to_string());
                            }
                        }
                    }
                }
            }
        }
    }
    files.sort();
    files
}
