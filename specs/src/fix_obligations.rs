use std::path::PathBuf;
use std::fs;
use anyhow::Result;
use regex::Regex;
use walkdir::WalkDir;
use crate::util::{print_header, print_success};

/// Automatically add Obligations section with t1 to contracts that are missing them
pub fn run_fix_obligations(root: &PathBuf) -> Result<usize> {
    print_header("Adding Obligations to Contracts (Policy B)");

    let specs_dir = root.join("specs/contracts");
    let mut fixed_count = 0;

    let contract_heading_re = Regex::new(r"^(###\s+\[([a-z][a-z0-9-]*-[0-9]+(?:-[a-z]+|[a-z]*))\]\s+—\s*(.*))$").unwrap();
    let obligations_heading_re = Regex::new(r"^\*\*Obligations:\*\*").unwrap();
    let obligation_t1_re = Regex::new(r"^-\s+\*\*t1\*\*:").unwrap();

    for entry in WalkDir::new(&specs_dir).into_iter().filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() || !entry.path().extension().map_or(false, |e| e == "md") {
            continue;
        }

        let path = entry.path();
        let content = fs::read_to_string(path)?;
        let lines: Vec<&str> = content.lines().collect();

        let mut new_lines: Vec<String> = Vec::new();
        let mut i = 0;
        let mut file_modified = false;

        while i < lines.len() {
            let line = lines[i];

            // Check for contract heading
            if let Some(caps) = contract_heading_re.captures(line) {
                let contract_id = caps.get(2).unwrap().as_str();
                let contract_title = caps.get(3).unwrap().as_str();

                // Add the contract heading
                new_lines.push(line.to_string());

                // Search forward to see if Obligations section exists
                let mut found_obligations = false;
                let mut found_t1 = false;
                let mut next_contract_idx = None;
                let mut j = i + 1;

                while j < lines.len() {
                    let check_line = lines[j];

                    // Stop if we hit the next contract or major section
                    if check_line.starts_with("###") || check_line.starts_with("## ") {
                        next_contract_idx = Some(j);
                        break;
                    }

                    // Check for Obligations heading
                    if obligations_heading_re.is_match(check_line) {
                        found_obligations = true;
                    }

                    // Check for t1 obligation
                    if found_obligations && obligation_t1_re.is_match(check_line) {
                        found_t1 = true;
                        break;
                    }

                    j += 1;
                }

                // If contract is missing obligations, add them
                if !found_obligations || !found_t1 {
                    println!("Fixing contract [{}]", contract_id);

                    // Add a blank line, then Obligations section
                    new_lines.push(String::new());
                    new_lines.push("**Obligations:**".to_string());

                    // Generate t1 from contract title
                    let t1_desc = generate_t1_from_title(contract_title);
                    new_lines.push(format!("- **t1**: {}", t1_desc));

                    file_modified = true;
                    fixed_count += 1;

                    // Skip ahead to where we found the next section or continue from current position
                    if let Some(next_idx) = next_contract_idx {
                        // Copy lines from i+1 to next_idx-1
                        for k in (i + 1)..next_idx {
                            new_lines.push(lines[k].to_string());
                        }
                        i = next_idx - 1; // Will be incremented at end of loop
                    } else {
                        // Copy remaining lines
                        for k in (i + 1)..lines.len() {
                            new_lines.push(lines[k].to_string());
                        }
                        break;
                    }
                } else {
                    // Contract already has obligations, just continue normally
                }
            } else {
                // Not a contract heading, just copy the line
                new_lines.push(line.to_string());
            }

            i += 1;
        }

        // Write back if modified
        if file_modified {
            let new_content = new_lines.join("\n");
            fs::write(path, new_content)?;
            println!("  Updated: {}", path.file_name().unwrap().to_str().unwrap());
        }
    }

    println!();
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    if fixed_count > 0 {
        print_success(&format!("Added Obligations to {} contracts", fixed_count));
    } else {
        print_success("All contracts already have Obligations");
    }

    Ok(fixed_count)
}

/// Generate a t1 obligation description from the contract title
/// This creates a minimal, testable statement derived from the contract heading
fn generate_t1_from_title(title: &str) -> String {
    // Clean up the title
    let cleaned = title.trim();

    // If title is empty, return a generic placeholder
    if cleaned.is_empty() {
        return "Contract behavior is correct".to_string();
    }

    // Common patterns:
    // "Client connects successfully" -> "Client connects successfully"
    // "Message routing" -> "Messages are routed correctly"
    // "Entity replication" -> "Entities replicate correctly"

    // If it's already a sentence (ends with punctuation or contains "must"), use as-is
    if cleaned.ends_with('.') || cleaned.ends_with('!') ||
       cleaned.to_lowercase().contains("must") || cleaned.to_lowercase().contains("should") {
        return cleaned.to_string();
    }

    // If it's a noun phrase, convert to assertion
    // For simplicity, we'll use the title as-is with "works correctly" appended
    format!("{} works correctly", cleaned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_t1_from_title() {
        assert_eq!(
            generate_t1_from_title("Client connects successfully"),
            "Client connects successfully"
        );

        assert_eq!(
            generate_t1_from_title("Message routing"),
            "Message routing works correctly"
        );

        assert_eq!(
            generate_t1_from_title("Server MUST reject invalid auth"),
            "Server MUST reject invalid auth"
        );
    }
}
