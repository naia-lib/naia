use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use anyhow::Result;
use regex::Regex;
use walkdir::WalkDir;

use crate::types::{Contract, SpecFile, TestFile, TestFunction};

pub struct Index {
    pub root: PathBuf,
    pub specs_dir: PathBuf,
    pub tests_dir: PathBuf,
    
    pub specs: BTreeMap<String, SpecFile>,     // path relative to contracts dir -> SpecFile
    pub contracts: BTreeMap<String, Contract>, // Contract ID -> Contract
    pub tests: BTreeMap<String, TestFile>,     // path relative to test/tests dir -> TestFile
    
    // Derived mappings
    // Using BTreeMap for deterministic output
    pub contract_obligations: BTreeMap<String, Vec<String>>, // Contract ID -> List of obligation IDs (t1, t2)
    pub contract_to_test_files: BTreeMap<String, HashSet<String>>, // Contract ID -> Set of Test Filenames
    pub contract_to_labels: BTreeMap<String, HashSet<String>>, // Contract ID -> Set of labels found in tests
}

impl Index {
    pub fn build(root: PathBuf) -> Result<Self> {
        let specs_dir = root.join("specs/contracts");
        let tests_dir = root.join("test/tests");

        let mut index = Index {
            root,
            specs_dir: specs_dir.clone(),
            tests_dir: tests_dir.clone(),
            specs: BTreeMap::new(),
            contracts: BTreeMap::new(),
            tests: BTreeMap::new(),
            contract_obligations: BTreeMap::new(),
            contract_to_test_files: BTreeMap::new(),
            contract_to_labels: BTreeMap::new(),
        };

        index.scan_specs()?;
        index.scan_tests()?;

        Ok(index)
    }

    fn scan_specs(&mut self) -> Result<()> {
        let _contract_id_re = Regex::new(r"[a-z][a-z0-9-]*-[0-9]+[a-z]*").unwrap();
        let contract_def_re = Regex::new(r"^###\s+\[([a-z][a-z0-9-]*-[0-9]+[a-z]*)\]").unwrap();
        let obligations_header_re = Regex::new(r"^\*\*Obligations:\*\*").unwrap();
        let obligation_item_re = Regex::new(r"^\-\s+\*\*(t[0-9]+)\*\*:").unwrap();

        for entry in WalkDir::new(&self.specs_dir).min_depth(1).max_depth(1) {
            let entry = entry?;
            if !entry.file_type().is_file() || !entry.path().extension().map_or(false, |e| e == "md") {
                continue;
            }

            let path = entry.path();
            let relative_path = path.strip_prefix(&self.specs_dir)?.to_path_buf();
            let filename = relative_path.to_string_lossy().to_string();
            
            // Skip non-numbered specs if necessary
            if !filename.chars().next().map_or(false, |c| c.is_numeric()) {
                continue;
            }

            let content = fs::read_to_string(path)?;
            let mut title = String::new();
            let mut contracts_in_file = Vec::new();
            
            let mut current_contract: Option<String> = None;
            let mut in_obligations = false;
            let mut start_line;

            for (idx, line) in content.lines().enumerate() {
                let line_num = idx + 1;
                let line_trim = line.trim();
                
                // Title
                if title.is_empty() && line.starts_with("# ") {
                    title = line.trim_start_matches("# ").trim_start_matches("Spec: ").trim().to_string();
                }

                // Contract Definition
                if let Some(caps) = contract_def_re.captures(line) {
                    let contract_id = caps.get(1).unwrap().as_str().to_string();
                    
                    // Close previous contract if any
                     if let Some(prev_id) = current_contract.take() {
                         if let Some(c) = self.contracts.get_mut(&prev_id) {
                             c.end_line = line_num - 1;
                         }
                     }

                    current_contract = Some(contract_id.clone());
                    contracts_in_file.push(contract_id.clone());
                    start_line = line_num;
                    
                    self.contracts.insert(contract_id.clone(), Contract {
                        id: contract_id.clone(),
                        file_path: relative_path.clone(),
                        start_line,
                        end_line: 0, // Placeholder
                    });
                    
                    in_obligations = false;
                    continue;
                }
                
                // Reset context on other headers
                if (line.starts_with("### ") || line.starts_with("## ")) && current_contract.is_some() {
                    // Check if it's NOT a contract def (already handled)
                    if !contract_def_re.is_match(line) {
                         if let Some(prev_id) = current_contract.take() {
                             if let Some(c) = self.contracts.get_mut(&prev_id) {
                                 c.end_line = line_num - 1;
                             }
                         }
                         current_contract = None;
                         in_obligations = false;
                    }
                }

                if let Some(contract_id) = &current_contract {
                    if obligations_header_re.is_match(line) {
                        in_obligations = true;
                        continue;
                    }

                    if in_obligations {
                        if let Some(caps) = obligation_item_re.captures(line) {
                            let obl_id = caps.get(1).unwrap().as_str().to_string();
                            self.contract_obligations.entry(contract_id.clone())
                                .or_default()
                                .push(obl_id);
                        } else if !line_trim.is_empty() && !line_trim.starts_with('-') {
                            in_obligations = false;
                        }
                    }
                }
            }
            
            // Close last contract
            if let Some(prev_id) = current_contract {
                if let Some(c) = self.contracts.get_mut(&prev_id) {
                     c.end_line = content.lines().count();
                }
            }

            self.specs.insert(filename.clone(), SpecFile {
                path: relative_path,
                filename,
                title,
                contracts: contracts_in_file,
            });
        }
        Ok(())
    }

    fn scan_tests(&mut self) -> Result<()> {
        let bracketed_re = Regex::new(r"\[([a-z][a-z0-9-]*-[0-9]+[a-z]*)\]").unwrap();
        let annotation_re = Regex::new(r"///\s*Contract:\s*(.+)").unwrap();
        let label_re = Regex::new(r#"(?:spec_expect|expect_msg)\(\s*"([^"]+)"#).unwrap();
        let fn_re = Regex::new(r"^\s*(?:(?:pub|async|unsafe|extern)\s+)*fn\s+([a-z_][a-z0-9_]*)\s*").unwrap();

        for entry in WalkDir::new(&self.tests_dir).into_iter().filter_map(|e| e.ok()) {
            if !entry.file_type().is_file() || !entry.path().extension().map_or(false, |e| e == "rs") {
                continue;
            }

            let path = entry.path();
            let relative_path = path.strip_prefix(&self.tests_dir)?.to_path_buf();
            let filename = relative_path.to_string_lossy().to_string();
            
            let content = fs::read_to_string(path)?;
            let lines: Vec<&str> = content.lines().collect();

            let mut covered_contracts = HashMap::new();
            
            let mut i = 0;
            while i < lines.len() {
                let line = lines[i];
                if let Some(caps) = annotation_re.captures(line) {
                    let contracts_part = caps.get(1).unwrap().as_str();
                    let mut current_contracts = Vec::new();
                    
                    for m in bracketed_re.find_iter(contracts_part) {
                        let contract_id = m.as_str().trim_matches(|c| c == '[' || c == ']').to_string();
                        current_contracts.push(contract_id);
                    }

                    // Look ahead for function definition
                    let mut j = i + 1;
                    let mut found_fn = false;
                    
                    while j < lines.len() {
                        let next_line = lines[j].trim();
                        if next_line.starts_with("fn") || next_line.starts_with("async fn") || next_line.starts_with("pub fn") {
                            found_fn = true;
                            break;
                        }
                        if j - i > 15 { 
                            break; 
                        }
                        j += 1;
                    }

                    if found_fn {
                        if let Some(fn_caps) = fn_re.captures(lines[j]) {
                            let fn_name = fn_caps.get(1).unwrap().as_str().to_string();
                            let start_line = j + 1;
                            
                            // Extract body
                            let (end_line, labels) = extract_fn_body(&lines, j, &label_re);
                            
                            // Extract context
                            let context: Vec<String> = lines[i..j].iter().map(|s| s.to_string()).collect();

                            let test_fn = TestFunction {
                                name: fn_name,
                                line: start_line,
                                end_line,
                                labels: labels.clone(),
                                context,
                            };
                            
                            for contract_id in current_contracts {
                                self.contract_to_test_files.entry(contract_id.clone())
                                    .or_default()
                                    .insert(filename.clone());
                                    
                                for label in &labels {
                                     if label.starts_with(&format!("{}.", contract_id)) || 
                                        label.starts_with(&format!("{}:", contract_id)) {
                                        self.contract_to_labels.entry(contract_id.clone())
                                            .or_default()
                                            .insert(label.clone());
                                     }
                                }

                                covered_contracts.entry(contract_id).or_insert_with(Vec::new).push(test_fn.clone());
                            }
                        }
                    }
                }
                i += 1;
            }

            self.tests.insert(filename.clone(), TestFile {
                path: relative_path,
                filename,
                covered_contracts,
            });
        }
        Ok(())
    }
}

fn extract_fn_body(lines: &[&str], start_idx: usize, label_re: &Regex) -> (usize, Vec<String>) {
    let mut depth = 0;
    let mut started = false;
    let mut labels = Vec::new();
    let mut end_idx = start_idx;

    for (k, line) in lines.iter().enumerate().skip(start_idx) {
        // Collect labels
        for cap in label_re.captures_iter(line) {
            labels.push(cap.get(1).unwrap().as_str().to_string());
        }

        // Brace counting
        for c in line.chars() {
            match c {
                '{' => {
                    depth += 1;
                    started = true;
                }
                '}' => {
                    depth -= 1;
                }
                _ => {}
            }
        }

        if started && depth == 0 {
            end_idx = k;
            break;
        }
    }
    
    (end_idx + 1, labels)
}
