use std::path::PathBuf;
use std::fs;
use regex::Regex;
use std::collections::HashSet;
use crate::util::{print_header, print_warning, print_error, print_success, basename};

pub fn run_check_refs(root: &PathBuf) -> anyhow::Result<usize> {
    print_header("Checking Cross-References");

    let mut errors: usize = 0;

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
    // Sort spec files for deterministic iteration in main loop
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

    // Build valid specs set
    // Bash: keys are filename AND filename-without-prefix.
    let mut valid_specs: HashSet<String> = HashSet::new();
    // Also keep list of full names for fuzzy matching?
    // Bash iterates "${!valid_specs[@]}" which includes shorts.
    
    // Replicating bash: 
    // for file in spec_files:
    //    valid_specs[basename] = 1
    //    no_prefix = ...
    //    valid_specs[no_prefix] = 1
    
    for file in &spec_files {
        let name = basename(file);
        valid_specs.insert(name.clone());
        
        let re_prefix = Regex::new(r"^[0-9]+_").unwrap();
        let no_prefix = re_prefix.replace(&name, "").to_string();
        valid_specs.insert(no_prefix);
    }

    println!("Validating cross-references in {} files...", spec_files.len());
    println!();

    let re_ref = Regex::new(r"`[a-zA-Z0-9_]+\.md`").unwrap();

    for file in &spec_files {
        let basename_file = basename(file);
        
        let content = fs::read_to_string(file)?;
        
        // Extract references
        let mut refs: Vec<String> = Vec::new();
        for cap in re_ref.find_iter(&content) {
            // strip backticks
            let s = cap.as_str();
            let r = &s[1..s.len()-1];
            refs.push(r.to_string());
        }
        refs.sort();
        refs.dedup();

        for r in refs {
            if !valid_specs.contains(&r) {
                // Fuzzy match
                let mut found = false;
                
                // Bash iterates valid_specs keys in random order.
                // We must iterate in SAME random order or sort?
                // If we sort, we might pick differently than bash.
                // But usually we just want ANY match?
                // If consistent output required, I should probably sort the candidates.
                
                let mut candidates: Vec<String> = valid_specs.iter().cloned().collect();
                candidates.sort(); // Sort so at least consistent in Rust. Parity with bash randomness is impossible if multiple match.
                
                for valid in candidates {
                    if valid.ends_with(&r) { // "$valid" == *"$ref"
                        found = true;
                        print_warning(&format!("{}: '{}' should be '{}'", basename_file, r, valid));
                        break;
                    }
                }
                
                if !found {
                    print_error(&format!("{}: Invalid reference '{}' (file not found)", basename_file, r));
                    errors += 1;
                }
            }
        }
    }

    println!();
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    if errors == 0 {
        print_success("All cross-references valid!");
    } else {
        print_error(&format!("{} invalid references found", errors));
    }
    
    Ok(errors)
}
