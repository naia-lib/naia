use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;
use crate::index::Index;

pub fn generate_packet(index: &Index, contract_id: &str, full_tests: bool, output: Option<String>, deterministic: bool) -> Result<()> {
    // 1. Find contract
    let contract = index.contracts.get(contract_id)
        .context(format!("Contract [{}] not found", contract_id))?;
    
    let spec_file = index.specs.get(&contract.file_path.to_string_lossy().to_string())
        .context("Spec file not found in index")?;

    println!("Generating Contract Review Packet: {}", contract_id);
    println!("Found in: {}", spec_file.filename);

    // 2. Read spec excerpt
    let spec_full_path = index.specs_dir.join(&contract.file_path);
    let spec_content = fs::read_to_string(&spec_full_path)?;
    let spec_lines: Vec<&str> = spec_content.lines().collect();
    
    let end_line = if contract.end_line > 0 { contract.end_line } else { spec_lines.len() };
    let start_idx = contract.start_line.saturating_sub(1);
    let end_idx = end_line.min(spec_lines.len());
    
    let excerpt = spec_lines[start_idx..end_idx].join("\n");

    // 3. Find tests
    let mut test_files = Vec::new(); // List of filenames
    let mut matching_tests = Vec::new(); // (filename, TestFunction)

    if let Some(files) = index.contract_to_test_files.get(contract_id) {
        for filename in files {
            test_files.push(filename.clone());
            if let Some(test_file) = index.tests.get(filename) {
                if let Some(test_fns) = test_file.covered_contracts.get(contract_id) {
                    for tf in test_fns {
                         matching_tests.push((filename.clone(), tf));
                    }
                }
            }
        }
    }
    test_files.sort();
    
    println!("Found tests in {} file(s)", test_files.len());

    // 4. Output Path
    let output_path = output.map(PathBuf::from).unwrap_or_else(|| {
        index.root.join("specs/generated/packets").join(format!("{}.md", contract_id))
    });
    
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }
    println!("Writing packet to: {:?}", output_path);

    // 5. Generate Content
    let timestamp = if deterministic {
        "1970-01-01 00:00 UTC".to_string()
    } else {
        chrono::Utc::now().format("%Y-%m-%d %H:%M UTC").to_string()
    };
    let mut md = String::new();
    
    md.push_str(&format!("# Contract Review Packet: {}\n\n", contract_id));
    md.push_str(&format!("**Generated:** {}\n", timestamp));
    md.push_str(&format!("**Spec File:** {}\n", spec_file.filename));
    md.push_str(&format!("**Test Files:** {}\n\n", test_files.len()));
    md.push_str("---\n\n");
    
    md.push_str(&format!("## Spec: {}\n\n", spec_file.title));
    md.push_str(&format!("**Source:** `{}`\n\n", spec_file.filename));
    
    md.push_str("```\n");
    md.push_str(&excerpt);
    md.push_str("\n```\n\n");
    md.push_str("---\n\n");
    
    md.push_str("## Obligation Mapping\n\n");
    
    // Obligations from spec
    let obligations = index.contract_obligations.get(contract_id);
    if let Some(obls) = obligations {
        md.push_str(&format!("**Obligations (from spec):** {} detected\n\n", obls.len()));
        for obl in obls {
            md.push_str(&format!("- {}\n", obl));
        }
    } else {
        md.push_str("**Obligations (from spec):** (none detected)\n\n");
        md.push_str("> NOTE: If this contract has multiple testable behaviors, consider adding obligation IDs like **t1**, **t2** to the spec.\n");
    }
    md.push_str("\n");
    
    // Assertion Labels
    md.push_str("**Assertion Labels (from tests):**\n\n");
    
    // All labels found in tests for this contract
    let mut all_labels = Vec::new();
    if let Some(labels) = index.contract_to_labels.get(contract_id) {
        all_labels.extend(labels.iter().cloned());
    }
    all_labels.sort(); 
    all_labels.dedup();
    
    if all_labels.is_empty() {
        md.push_str("- (none found)\n\n");
        md.push_str("> NOTE: Tests exist but have no labeled assertions. Add spec_expect(...) to enable obligation mapping.\n\n");
    } else {
        for label in &all_labels {
            md.push_str(&format!("- `{}`\n", label));
        }
        md.push_str("\n");
    }

    // Mapping Report
    md.push_str("**Mapping Report:**\n\n");
    
    let obligations = index.contract_obligations.get(contract_id);
    match obligations {
        Some(obls) if !obls.is_empty() => {
             let mut missing_obligations = Vec::new();
             for obl in obls {
                 let pattern1 = format!("{}.{}:", contract_id, obl);
                 // spec_tool.sh uses "$pattern" where pattern="${contract_id}.${obl}:"
                 // grep -F checks literal match.
                 
                 let covered = all_labels.iter().find(|l| l.contains(&pattern1));
                 
                 if let Some(label) = covered {
                     md.push_str(&format!("- ✅ **{}** → covered by: `{}`\n", obl, label));
                 } else {
                     md.push_str(&format!("- ❌ **{}** → MISSING (no label matching `{}`)\n", obl, pattern1));
                     missing_obligations.push(obl);
                 }
             }
             
             md.push_str("\n");
             if missing_obligations.is_empty() {
                 md.push_str("✅ **OK:** All obligations mapped to labeled assertions.\n");
             } else {
                 let refs: Vec<String> = missing_obligations.iter().map(|s| s.to_string()).collect();
                 md.push_str(&format!("⚠️ **MISSING:** {}\n\n", refs.join(", ")));
                 md.push_str(&format!("> Add spec_expect(`{}.tN: ...`) calls to cover these obligations.\n", contract_id));
             }
        }
        _ => {
            // No obligations
             if !all_labels.is_empty() {
                 let has_contract_label = all_labels.iter().any(|l| l.starts_with(&format!("{}.", contract_id)) || l.starts_with(&format!("{}:", contract_id)));
                 if has_contract_label {
                     md.push_str("✅ **OK:** No obligations defined; at least one contract-level label present.\n");
                 } else {
                     md.push_str(&format!("⚠️ **NEEDS LABELS:** Tests have labels but none match `{}:` or `{}.` prefix.\n", contract_id, contract_id));
                 }
             } else {
                 md.push_str("⚠️ **NEEDS LABELS:** No obligations defined and no labeled assertions found.\n");
             }
        }
    }
    md.push_str("\n---\n\n");

    if test_files.is_empty() {
        md.push_str("**⚠️ WARNING:** No tests found for this contract.\n\n");
    } else {
        md.push_str("## Tests\n\n");

        for filename in &test_files {
             md.push_str(&format!("### Test File: `{}`\n\n", filename));
             
             if let Some(test_file) = index.tests.get(filename) {
                if let Some(test_fns) = test_file.covered_contracts.get(contract_id) {
                    for test_fn in test_fns {
                         md.push_str("```rust\n");
                         
                         for line in &test_fn.context {
                             md.push_str(line);
                             md.push_str("\n");
                         }
                         
                         let file_path = index.tests_dir.join(filename);
                         if let Ok(content) = fs::read_to_string(file_path) {
                             let lines: Vec<&str> = content.lines().collect();
                             let sig_line_idx = test_fn.line.saturating_sub(1);
                             
                             if sig_line_idx < lines.len() {
                                 md.push_str(lines[sig_line_idx]);
                                 md.push_str("\n");
                             }
                             
                             if full_tests {
                                 let start_body = sig_line_idx + 1;
                                 let end_body = test_fn.end_line.min(lines.len());
                                 if start_body < end_body {
                                      let body = lines[start_body..end_body].join("\n");
                                      md.push_str(&body);
                                      md.push_str("\n");
                                 }
                             } else {
                                 md.push_str("\n    // Assertion Index:\n");
                                 let mut labels = test_fn.labels.clone();
                                 labels.sort();
                                 labels.dedup();
                                 
                                 if labels.is_empty() {
                                     md.push_str("    // (no explicit labels)\n");
                                 } else {
                                     for label in labels {
                                         md.push_str(&format!("    - `spec_expect(\"{}\")`\n", label));
                                     }
                                 }
                             }
                         }
                         md.push_str("```\n\n");
                    }
                }
             }
        }
    }
    
    fs::write(output_path, md)?;
    
    Ok(())
}
