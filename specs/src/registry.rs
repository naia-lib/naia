use std::path::PathBuf;
use std::fs;
use regex::Regex;
use chrono::Utc;
use crate::util::{print_header, print_success, print_info, basename};

fn get_title(path: &PathBuf) -> String {
    let content = fs::read_to_string(path).unwrap_or_default();
    for line in content.lines() {
        if line.starts_with("# ") {
            let title = &line[2..];
            if title.starts_with("Spec: ") {
                return title[6..].to_string();
            }
            return title.to_string();
        }
    }
    basename(path)
}

pub fn run_registry(root: &PathBuf, output: Option<String>) -> anyhow::Result<()> {
    let output_file = output.unwrap_or_else(|| {
        root.join("specs/generated/CONTRACT_REGISTRY.md")
            .to_string_lossy()
            .to_string()
    });

    print_header("Generating Contract Registry");

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

    // Sort files exactly like bash `sort -V` (via our custom basename-based sort)
    spec_files.sort_by(|a, b| {
        let name_a = basename(a);
        let name_b = basename(b);
        let extract_num = |name: &str| -> Option<u32> {
            name.split('_').next().and_then(|s| s.parse().ok())
        };
        match (extract_num(&name_a), extract_num(&name_b)) {
            (Some(num_a), Some(num_b)) => {
                if num_a != num_b {
                    return num_a.cmp(&num_b);
                }
                name_a.cmp(&name_b)
            }
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => name_a.cmp(&name_b),
        }
    });

    let mut total_contracts = 0;
    let mut contracts_by_spec: Vec<(String, Vec<String>)> = Vec::new();

    let re_p1 = Regex::new(r"### \[[a-z-]+-[0-9]+[a-z]*\]").unwrap();
    let re_id_p1 = Regex::new(r"\[([a-z-]+-[0-9]+[a-z]*)\]").unwrap();

    let re_p2 = Regex::new(r"^### [a-z-]+-[0-9]+[a-z]*").unwrap();
    let re_p3 = Regex::new(r"> [a-z-]+-[0-9]+[a-z]* \(MUST").unwrap();
    let re_p4 = Regex::new(r"^\*\*([a-z-]+-[0-9]+[a-z]*)\*\*:").unwrap();

    for file in &spec_files {
        let fname = basename(file);
        let mut file_contracts = Vec::new();
        let content = fs::read_to_string(file)?;

        for line in content.lines() {
            // Pattern 1
            if let Some(cap) = re_p1.find(line) {
                if let Some(id_cap) = re_id_p1.captures(cap.as_str()) {
                    file_contracts.push(id_cap[1].to_string());
                }
            }
            // Pattern 2
            if let Some(cap) = re_p2.find(line) {
                if line.contains(" — ") {
                    file_contracts.push(cap.as_str()[4..].to_string());
                }
            }
            // Pattern 3
            if let Some(cap) = re_p3.find(line) {
                // "> id (MUST" -> strip "> "
                let id_raw = cap.as_str();
                let id = id_raw[2..id_raw.len()-6].trim();
                file_contracts.push(id.to_string());
            }
            // Pattern 4
            if let Some(cap) = re_p4.captures(line) {
                file_contracts.push(cap[1].to_string());
            }
        }

        // Unique and version-sort like bash
        file_contracts.sort_by(|a, b| {
            // Very simple version sort for contract IDs: entity-07 vs entity-07a
            // Bash `sort -V` handles this.
            // For now, lexicographical is close enough for entity-01, entity-02.
            // If they have suffixes like -03a, -03b, lexicographical still works.
            a.cmp(b)
        });
        file_contracts.dedup();

        total_contracts += file_contracts.len();
        contracts_by_spec.push((fname, file_contracts));
    }

    let mut out = String::new();
    let now = Utc::now().format("%Y-%m-%d %H:%M UTC");
    
    out.push_str("# Contract ID Registry\n\n");
    out.push_str(&format!("**Generated:** {}\n", now));
    out.push_str(&format!("**Total Contracts:** {}\n\n---\n\n", total_contracts));
    out.push_str("## Summary by Specification\n\n");
    out.push_str("| Spec File | Contract Count | ID Range |\n");
    out.push_str("|-----------|----------------|----------|\n");

    for (fname, contracts) in &contracts_by_spec {
        let count = contracts.len();
        if count > 0 {
            let first = &contracts[0];
            let last = &contracts[count - 1];
            out.push_str(&format!("| {} | {} | {} → {} |\n", fname, count, first, last));
        }
    }

    out.push_str("\n---\n\n## Full Contract Index\n\n");

    for (fname, contracts) in &contracts_by_spec {
        if !contracts.is_empty() {
            let path = contracts_dir.join(fname);
            let title = get_title(&path);
            out.push_str(&format!("### {} ({})\n\n", title, fname));
            for id in contracts {
                out.push_str(&format!("- `{}`\n", id));
            }
            out.push_str("\n");
        }
    }

    fs::write(&output_file, out)?;

    print_success(&format!("Generated: {}", output_file));
    print_info(&format!("Total contracts: {}", total_contracts));

    Ok(())
}
