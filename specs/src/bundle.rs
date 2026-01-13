use std::path::{Path, PathBuf};
use std::fs;
use chrono::Utc;
use crate::util::{print_header, print_info, print_success, basename};

pub fn run_bundle(root: &Path, output: Option<String>) -> anyhow::Result<()> {
    let output_path = output.unwrap_or_else(|| "specs/generated/NAIA_SPECS.md".to_string());
    let output_file = root.join(&output_path);

    print_header("Generating NAIA_SPECS.md Bundle");

    // Get spec files
    let contracts_dir = root.join("specs/contracts");
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

    // Sort numerically by prefix
    spec_files.sort_by_key(|path| {
        basename(path)
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect::<String>()
            .parse::<u32>()
            .unwrap_or(0)
    });

    if spec_files.is_empty() {
        return Err(anyhow::anyhow!("No specification files found in {:?}", contracts_dir));
    }

    print_info(&format!("Found {} specification files", spec_files.len()));

    let mut content = String::new();

    // Header
    content.push_str("# Naia Specifications Bundle\n\n");
    content.push_str("This document contains all normative specifications for the Naia networking engine, concatenated into a single reference.\n\n");
    content.push_str(&format!("**Generated:** {}\n", Utc::now().format("%Y-%m-%d %H:%M UTC")));
    content.push_str(&format!("**Spec Count:** {}\n\n", spec_files.len()));
    content.push_str("---\n\n");
    content.push_str("## Table of Contents\n\n");

    // TOC
    for file in &spec_files {
        let title = get_title(file);
        let anchor = make_anchor(&title);
        let filename = basename(file);
        let spec_num = filename.chars().take_while(|c| c.is_ascii_digit()).collect::<String>();
        content.push_str(&format!("- [{}. {}](#{})\n", spec_num, title, anchor));
    }

    content.push_str("\n---\n\n");

    // Concatenate
    for file in &spec_files {
        let filename = basename(file);
        let file_content = fs::read_to_string(file)?;

        content.push_str("<!-- ======================================================================== -->\n");
        content.push_str(&format!("<!-- Source: {} -->\n", filename));
        content.push_str("<!-- ======================================================================== -->\n\n");
        
        content.push_str(&file_content);
        
        content.push_str("\n\n---\n\n");
    }

    // Ensure directory exists
    if let Some(parent) = output_file.parent() {
        fs::create_dir_all(parent)?;
    }
    
    fs::write(&output_file, content)?;

    print_success(&format!("Generated: {}", output_path));
    println!("");
    println!("Included specifications:");
    for file in &spec_files {
        println!("  - {}", basename(file));
    }

    Ok(())
}

fn get_title(path: &Path) -> String {
    let content = fs::read_to_string(path).unwrap_or_default();
    for line in content.lines() {
        if line.starts_with("# ") {
            let title = line[2..].trim();
            if title.starts_with("Spec: ") {
                return title[6..].to_string();
            }
            return title.to_string();
        }
    }
    basename(path)
}

fn make_anchor(title: &str) -> String {
    title.to_lowercase()
        .replace(' ', "-")
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-')
        .collect()
}
