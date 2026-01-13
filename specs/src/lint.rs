use std::path::PathBuf;
use std::fs;
// use colored::*; // Not needed directly if using util
use regex::Regex;
use crate::util::{print_header, print_warning, print_success, print_error, basename};

// Removed local implementations of print_*, basename

pub fn run_lint(root: &PathBuf) -> anyhow::Result<usize> {
    print_header("Linting Specifications");

    let mut issues: usize = 0;
    let mut warnings: usize = 0;

    let contracts_dir = root.join("specs/contracts");
    
    // Scan for all .md files
    let mut spec_files: Vec<PathBuf> = Vec::new();
    if let Ok(entries) = fs::read_dir(&contracts_dir) {
        for entry in entries {
            if let Ok(entry) = entry {
                let path = entry.path();
                if path.is_file() && path.extension().map_or(false, |e| e == "md") {
                    spec_files.push(path);
                }
            }
        }
    }

    
    spec_files.sort_by(|a, b| {
        let name_a = basename(a);
        let name_b = basename(b);
        
        let extract_num = |name: &str| -> Option<u32> {
            name.split('_').next().and_then(|s| s.parse().ok())
        };

        match (extract_num(&name_a), extract_num(&name_b)) {
            (Some(num_a), Some(num_b)) => {
               if num_a != num_b {
                   return num_a.cmp(&num_b)
               }
               name_a.cmp(&name_b)
            },
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => name_a.cmp(&name_b),
        }
    });


    // Check 1: Title format (Spec: prefix)
    println!("Checking title format...");
    for file in &spec_files {
        let content = fs::read_to_string(file)?;
        // grep -q '^# Spec:'
        for line in content.lines() {
            if line.starts_with("# Spec:") {
                print_warning(&format!("{}: Has 'Spec:' prefix in title (should be removed)", basename(file)));
                warnings += 1;
                // grep -q stops at first match
                break;
            }
        }
    }

    // Check 2: Contract ID formats
    println!("Checking contract ID formats...");
    let re_format1 = Regex::new(r"> [a-z-]*-[0-9]* \(MUST").unwrap();
    let re_format2 = Regex::new(r"^### [a-z]+(-[a-z]+)*-[0-9]+ — ").unwrap();
    let re_format3 = Regex::new(r"^\*\*[a-z-]+-[0-9]+[a-z]*\*\*:").unwrap();

    for file in &spec_files {
        let content = fs::read_to_string(file)?;
        let fname = basename(file);
        
        let format1_count = re_format1.find_iter(&content).count();
        // format2 and 3 are anchored to start of line manually in loop
        let mut format2_count = 0;
        let mut format3_count = 0;
        
        for line in content.lines() {
            if re_format2.is_match(line) { format2_count += 1; }
            if re_format3.is_match(line) { format3_count += 1; }
        }

        if format1_count > 0 {
            print_warning(&format!("{}: Has {} contract IDs in format '> id (MUST):' (migrate to '### [id] —')", fname, format1_count));
            warnings += 1;
        }
        if format2_count > 0 {
            print_warning(&format!("{}: Has {} contract IDs without brackets (add brackets: '### [id] —')", fname, format2_count));
            warnings += 1;
        }
        if format3_count > 0 {
             print_warning(&format!("{}: Has {} contract IDs in bold format (migrate to '### [id] —')", fname, format3_count));
             warnings += 1;
        }
    }

    // Check 3: Cross-reference formats
    println!("Checking cross-reference formats...");
    let re_ref = Regex::new(r"`[a-z_]+\.md`").unwrap();
    let re_good_ref = Regex::new(r"[0-9]_").unwrap();
    
    for file in &spec_files {
        let content = fs::read_to_string(file)?;
        // grep -oE ... | grep -v ... | sort -u
        let mut bad_refs: Vec<String> = Vec::new();
        
        for cap in re_ref.find_iter(&content) {
            let refer = cap.as_str();
            if !re_good_ref.is_match(refer) {
                bad_refs.push(refer.to_string());
            }
        }
        bad_refs.sort();
        bad_refs.dedup();
        
        if !bad_refs.is_empty() {
             print_warning(&format!("{}: Cross-refs missing numeric prefix:", basename(file)));
             for r in bad_refs {
                 println!("    {}", r);
             }
             warnings += 1;
        }
    }

    // Check 4: Terminology consistency
    println!("Checking terminology consistency...");
    let mut debug_files = 0;
    let mut diag_files = 0;
    
    // Scan all files in contracts dir for terminology
    // `grep -l 'In Debug:'` returns count of FILES.
    for file in &spec_files {
        let content = fs::read_to_string(file)?;
        if content.contains("In Debug:") {
            debug_files += 1;
        }
        // grep 'diagnostics.*enabled'
        let re_diag = Regex::new(r"diagnostics.*enabled").unwrap();
        if re_diag.is_match(&content) {
            diag_files += 1;
        }
    }
    
    if debug_files > 0 && diag_files > 0 {
        print_warning(&format!("Mixed terminology: {} files use 'Debug', {} files use 'Diagnostics'", debug_files, diag_files));
        warnings += 1; // ((warnings++)) || true
    }

    // Check 5: Test obligations sections
    println!("Checking for test obligations sections...");
    let re_test_obs = Regex::new(r"(?i)^## ([0-9]+\) )?Test [Oo]bligations").unwrap();
    for file in &spec_files {
        let content = fs::read_to_string(file)?;
        let mut found = false;
        for line in content.lines() {
            if re_test_obs.is_match(line) {
                found = true;
                break;
            }
        }
        if !found {
            print_warning(&format!("{}: Missing '## Test obligations' section", basename(file)));
            warnings += 1;
        }
    }

    // Check 6: Policy B - Every contract must have Obligations with at least t1
    println!("Checking Policy B (every contract needs Obligations with t1)...");
    let contract_heading_re = Regex::new(r"^###\s+\[([a-z][a-z0-9-]*-[0-9]+(?:-[a-z]+|[a-z]*))\]\s+—").unwrap();
    let obligations_heading_re = Regex::new(r"^\*\*Obligations:\*\*").unwrap();
    let obligation_t1_re = Regex::new(r"^-\s+\*\*t1\*\*:").unwrap();

    for file in &spec_files {
        let content = fs::read_to_string(file)?;
        let lines: Vec<&str> = content.lines().collect();

        let mut i = 0;
        while i < lines.len() {
            // Look for contract heading
            if let Some(caps) = contract_heading_re.captures(lines[i]) {
                let contract_id = caps.get(1).unwrap().as_str();

                // Search forward for Obligations section in this contract
                let mut found_obligations = false;
                let mut found_t1 = false;
                let mut j = i + 1;

                while j < lines.len() {
                    let line = lines[j];

                    // Stop if we hit the next contract or major section
                    if line.starts_with("###") || line.starts_with("## ") {
                        break;
                    }

                    // Check for Obligations heading
                    if obligations_heading_re.is_match(line) {
                        found_obligations = true;
                    }

                    // Check for t1 obligation
                    if found_obligations && obligation_t1_re.is_match(line) {
                        found_t1 = true;
                        break;
                    }

                    j += 1;
                }

                if !found_obligations {
                    print_error(&format!("{}: Contract [{}] missing **Obligations:** section", basename(file), contract_id));
                    issues += 1;
                } else if !found_t1 {
                    print_error(&format!("{}: Contract [{}] missing **t1** obligation", basename(file), contract_id));
                    issues += 1;
                }
            }
            i += 1;
        }
    }

    // Check 7: No Placeholder t1 - t1 cannot contain generic text or TODOs
    println!("Checking for placeholder t1 obligations...");
    let forbidden_re = Regex::new(r"(?i)(works correctly|behavior is correct|is correct|functions properly|todo|tbd)").unwrap();

    for file in &spec_files {
        let content = fs::read_to_string(file)?;
        let lines: Vec<&str> = content.lines().collect();
        let mut current_contract_id: Option<String> = None;

        for line in lines {
            // Keep track of which contract we are in
            if let Some(caps) = contract_heading_re.captures(line) {
                current_contract_id = Some(caps.get(1).unwrap().as_str().to_string());
            }

            // Check t1 lines
            if obligation_t1_re.is_match(line) {
                if forbidden_re.is_match(line) {
                    let contract_display = current_contract_id.as_deref().unwrap_or("UNKNOWN");
                    print_error(&format!("{}: Contract [{}] t1 contains placeholder text: '{}'", basename(file), contract_display, line.trim()));
                    issues += 1;
                }
            }
        }
    }

    println!();
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    if issues == 0 && warnings == 0 {
        print_success("All checks passed!");
    } else {
        // Red, Yellow, NC
        let red = "\x1b[0;31m"; // RED='\033[0;31m'
        let yellow = "\x1b[1;33m";
        let nc = "\x1b[0m";
        println!("Results: {}{} errors{}, {}{} warnings{}", red, issues, nc, yellow, warnings, nc);
    }
    
    Ok(issues)
}
