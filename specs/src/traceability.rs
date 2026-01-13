use std::path::Path;
use std::fs;
use std::collections::{BTreeMap, HashSet};
use chrono::Utc;
use regex::Regex;
use crate::util::{print_header, print_success, basename};

pub fn run_traceability(root: &Path, output: Option<String>, silent: bool, deterministic: bool) -> anyhow::Result<()> {
    let output_file = output.unwrap_or_else(|| {
        root.join("specs/generated/TRACEABILITY.md")
            .to_string_lossy()
            .to_string()
    });

    let registry_file = root.join("specs/generated/CONTRACT_REGISTRY.md");
    let test_dir = root.join("test/tests");

    if !silent {
        print_header("Generating Traceability Matrix");
    }

    // 1. Get all contracts from registry
    let mut all_contracts = Vec::new();
    if registry_file.exists() {
        let content = fs::read_to_string(&registry_file)?;
        let id_re = Regex::new(r"`([a-z-]+-[0-9]+[a-z-]*)`").unwrap();
        for cap in id_re.captures_iter(&content) {
            all_contracts.push(cap[1].to_string());
        }
    }
    all_contracts.sort();
    all_contracts.dedup();

    // 2. Map contracts to tests
    let mut contract_to_test = BTreeMap::new();
    let mut test_to_contracts = BTreeMap::new();

    let mut test_files = Vec::new();
    if let Ok(entries) = fs::read_dir(&test_dir) {
        for entry in entries {
            if let Ok(entry) = entry {
                let path = entry.path();
                if path.is_file() && path.extension().map_or(false, |e| e == "rs") {
                    test_files.push(path);
                }
            }
        }
    }
    test_files.sort_by_key(|a| basename(a));

    // Buggy regex for Table 2 and Summary to match bash exactly
    let buggy_contract_re = Regex::new(r"\[([a-z][a-z0-9-]*-[0-9]+)\]").unwrap();

    for test_file in &test_files {
        let content = fs::read_to_string(test_file)?;
        let fname = basename(test_file);
        let mut file_contracts = HashSet::new();

        for line in content.lines() {
            if line.contains("Contract:") {
                // For Table 2, bash uses the buggy regex
                for cap in buggy_contract_re.captures_iter(line) {
                    file_contracts.insert(cap[1].to_string());
                }
            }
        }
        
        if !file_contracts.is_empty() {
            let mut sorted_contracts: Vec<_> = file_contracts.into_iter().collect();
            sorted_contracts.sort();
            test_to_contracts.insert(fname, sorted_contracts);
        }
    }

    // Pass for Table 1: Contracts -> Tests
    for id in &all_contracts {
        // Regex for EXACT contract ID in brackets
        let exact_re = Regex::new(&format!(r"Contract:.*\[{}\]", regex::escape(id))).unwrap();
        
        for test_file in &test_files {
            let content = fs::read_to_string(test_file)?;
            if exact_re.is_match(&content) {
                let fname = basename(test_file);
                let mut fn_name = String::from("(manual check)");
                
                let lines: Vec<&str> = content.lines().collect();
                for i in 0..lines.len() {
                    if exact_re.is_match(lines[i]) {
                        // Check next 5 lines for "fn name" (A5)
                        for j in 1..=5 {
                            if i + j < lines.len() {
                                let next_line = lines[i+j].trim();
                                if next_line.starts_with("fn ") {
                                    if let Some(end) = next_line[3..].find(|c: char| !c.is_alphanumeric() && c != '_') {
                                        fn_name = next_line[3..3+end].to_string();
                                    } else {
                                        fn_name = next_line[3..].to_string();
                                    }
                                    break;
                                }
                            }
                        }
                        if fn_name != "(manual check)" { break; }
                    }
                }
                contract_to_test.insert(id.clone(), (fname, fn_name));
                break; // First file wins (head -1)
            }
        }
    }

    let mut out = String::new();
    let now = if deterministic {
        "1970-01-01 00:00 UTC".to_string()
    } else {
        Utc::now().format("%Y-%m-%d %H:%M UTC").to_string()
    };

    out.push_str("# Contract Traceability Matrix\n\n");
    out.push_str(&format!("**Generated:** {}\n\n", now));
    out.push_str("This matrix shows the bidirectional mapping between contracts and tests.\n\n---\n\n");
    out.push_str("## Contracts → Tests\n\n");
    out.push_str("| Contract | Test Function | Test File | Status |\n");
    out.push_str("|----------|---------------|-----------|--------|\n");

    for id in &all_contracts {
        if let Some((fname, fn_name)) = contract_to_test.get(id) {
            out.push_str(&format!("| `{}` | `{}` | {} | COVERED |\n", id, fn_name, fname));
        } else {
            out.push_str(&format!("| `{}` | - | - | **UNCOVERED** |\n", id));
        }
    }

    out.push_str("\n---\n\n## Tests → Contracts\n\n");
    out.push_str("| Test File | Test Function | Contracts Verified |\n");
    out.push_str("|-----------|---------------|--------------------|\n");

    for test_file in &test_files {
        let fname = basename(test_file);
        if let Some(contracts) = test_to_contracts.get(&fname) {
             // Bash buggy join: id1,id2,id3,
             let mut c_str = String::new();
             for c in contracts {
                 c_str.push_str(c);
                 c_str.push_str(",");
             }

             // Get function names for this file
             let mut fn_names = Vec::new();
             let content = fs::read_to_string(test_file)?;
             let lines: Vec<&str> = content.lines().collect();
             for i in 0..lines.len() {
                 if lines[i].contains("Contract: [") {
                     for j in 1..=5 {
                         if i >= j {
                             let prev_line = lines[i-j].trim();
                             if prev_line.starts_with("fn ") {
                                if let Some(end) = prev_line[3..].find(|c: char| !c.is_alphanumeric() && c != '_') {
                                    fn_names.push(prev_line[3..3+end].to_string());
                                } else {
                                    fn_names.push(prev_line[3..].to_string());
                                }
                                break;
                             }
                         }
                     }
                 }
             }
             fn_names.sort();
             fn_names.dedup();
             let fn_names_subset: Vec<_> = fn_names.into_iter().take(5).collect();
             let f_str = if fn_names_subset.is_empty() { 
                 String::from("(check manually)") 
             } else {
                 let mut s = String::new();
                 for f in fn_names_subset {
                     s.push_str(&f);
                     s.push_str(",");
                 }
                 s
             };
             
             out.push_str(&format!("| {} | {} | {} |\n", fname, f_str, c_str));
        }
    }

    out.push_str("\n---\n\n## Summary\n\n");
    let total = all_contracts.len();
    
    // Covered count for summary uses the buggy regex on ALL test files
    let mut covered_set = HashSet::new();
    for test_file in &test_files {
        let content = fs::read_to_string(test_file)?;
        for line in content.lines() {
            if line.contains("Contract:") {
                for cap in buggy_contract_re.captures_iter(line) {
                    covered_set.insert(cap[1].to_string());
                }
            }
        }
    }
    let covered = covered_set.len();
    let pct = if total > 0 { covered * 100 / total } else { 0 };

    out.push_str(&format!("- **Total Contracts:** {}\n", total));
    out.push_str(&format!("- **Contracts with Tests:** {}\n", covered));
    out.push_str(&format!("- **Coverage:** {}%\n", pct));

    fs::write(&output_file, out)?;
    if !silent {
        print_success(&format!("Generated: {}", output_file));
    }

    Ok(())
}
