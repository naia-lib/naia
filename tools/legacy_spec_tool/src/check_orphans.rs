use std::path::PathBuf;
use std::fs;
use regex::Regex;
use crate::util::{print_header, print_warning, print_success, basename};

pub fn run_check_orphans(root: &PathBuf) -> anyhow::Result<usize> {
    print_header("Checking for Orphan MUST/MUST NOT Statements");

    let mut orphans = 0;

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
         // Same sort as lint
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

    let re_must = Regex::new(r"\bMUST\b").unwrap();
    let re_ignore_start = Regex::new(r"^(```|<!--|    )").unwrap();
    let re_contract_id = Regex::new(r"\[[a-z-]+-[0-9]+[a-z]*\]|[a-z-]+-[0-9]+[a-z]* —|> [a-z-]+-[0-9]+[a-z]*|\*\*[a-z-]+-[0-9]+[a-z]*\*\*:").unwrap();
    let re_section = Regex::new(r"^## ").unwrap();
    let re_allowed_section = Regex::new(r"(?i)(glossary|vocabulary|definition|scope|normative)").unwrap();
    let re_normative_kw = Regex::new(r"(?i)^Normative keywords:").unwrap();

    for file in &spec_files {
        let basename_file = basename(file);
        let mut file_orphans = 0;
        
        let content = fs::read_to_string(file)?;
        let lines: Vec<&str> = content.lines().collect();
        
        // Track current section
        let mut sections: Vec<(usize, String)> = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            if re_section.is_match(line) {
                sections.push((i, line.to_string()));
            }
        }

        for (i, line) in lines.iter().enumerate() {
            let line_num = i + 1;
            
            // Check if line contains MUST (and not ignored)
            if re_must.is_match(line) && !re_ignore_start.is_match(line) {
                
                // Check for contract ID in last 10 lines (inclusive of current line?)
                // Bash `context_start=$((line_num - 10))`. `sed -n "${context_start},${line_num}p"`
                // line_num is 1-based.
                // If line_num=12, start=2. lines 2..12.
                // Inclusive.
                
                let start_idx = if i >= 10 { i - 10 } else { 0 };
                let end_idx = i; // Inclusive check on `lines` slice?
                // lines[start_idx..=end_idx] contains 11 lines max.
                
                let context = &lines[start_idx..=end_idx];
                let mut has_contract_id = false;
                for ctx_line in context {
                    if re_contract_id.is_match(ctx_line) {
                        has_contract_id = true;
                        break;
                    }
                }

                if !has_contract_id {
                    // Check section
                    // Find last section before or at this line.
                    let mut current_section = "";
                    for (sec_idx, sec_title) in &sections {
                        if *sec_idx <= i {
                            current_section = sec_title;
                        } else {
                            break;
                        }
                    }
                    
                    if re_normative_kw.is_match(line) {
                        continue;
                    }
                    
                    if !re_allowed_section.is_match(current_section) {
                         if file_orphans == 0 {
                             println!("\n{}:", basename_file);
                         }
                         
                         // Truncate line to 80 bytes equivalent (bash head -c 80)
                         let bytes = line.as_bytes();
                         let limit = std::cmp::min(bytes.len(), 80);
                         let truncated_line = String::from_utf8_lossy(&bytes[..limit]).to_string();
                         
                         print_warning(&format!("  Line {}: {}...", line_num, truncated_line));
                         file_orphans += 1;
                         orphans += 1;
                    }
                }
            }
        }
    }

    println!();
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    if orphans == 0 {
        print_success("No orphan MUST/MUST NOT statements found!");
    } else {
        println!("\x1b[1;33m⚠\x1b[0m {} potential orphan statements found (review manually)", orphans);
    }
    
    Ok(orphans)
}
