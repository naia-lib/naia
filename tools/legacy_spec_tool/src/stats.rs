use anyhow::Result;
use std::fs;
use regex::Regex;
use crate::index::Index;

const BLUE: &str = "\x1b[0;34m";
const NC: &str = "\x1b[0m";

pub fn run_stats(index: &Index) -> Result<()> {
    // Header
    let bar = "═══════════════════════════════════════════════════════════════";
    println!("\n{}{}{}\n{}  Specification Statistics{}\n{}{}{}\n", BLUE, bar, NC, BLUE, NC, BLUE, bar, NC);

    println!("Per-file statistics:\n");
    println!("{:<35} {:>8} {:>8} {:>10}", "File", "Lines", "Words", "Contracts");
    println!("{:<35} {:>8} {:>8} {:>10}", "----", "-----", "-----", "---------");

    let mut total_lines = 0;
    let mut total_words = 0;
    let mut total_contracts = 0;
    
    let contract_re = Regex::new(r"(\[[a-z-]+-[0-9]+[a-z]*\]|^### [a-z]+(-[a-z]+)*-[0-9]+[a-z]* |> [a-z-]+-[0-9]+[a-z]* \(MUST|\*\*[a-z-]+-[0-9]+[a-z]*\*\*)").unwrap();

    let mut specs: Vec<_> = index.specs.values().collect();
    specs.sort_by(|a, b| {
         // Parse leading number for natural sort (matching sort -V)
         let num_a = a.filename.split('_').next().and_then(|s| s.parse::<u32>().ok()).unwrap_or(u32::MAX);
         let num_b = b.filename.split('_').next().and_then(|s| s.parse::<u32>().ok()).unwrap_or(u32::MAX);
         
         if num_a != num_b {
             return num_a.cmp(&num_b);
         }
         a.filename.cmp(&b.filename)
    });

    for spec in specs {
        let abs_path = index.specs_dir.join(&spec.path);
        let content = fs::read_to_string(&abs_path)?;
        
        let lines = content.bytes().filter(|&b| b == b'\n').count();
        let words = content.split_whitespace().count();
        
        // grep -cE counts matching lines
        let contracts = content.lines()
            .filter(|line| contract_re.is_match(line))
            .count();

        println!("{:<35} {:>8} {:>8} {:>10}", spec.filename, lines, words, contracts);

        total_lines += lines;
        total_words += words;
        total_contracts += contracts;
    }

    println!("\n{:<35} {:>8} {:>8} {:>10}", "----", "-----", "-----", "---------");
    println!("{:<35} {:>8} {:>8} {:>10}", "TOTAL", total_lines, total_words, total_contracts);
    
    println!("\nAdditional metrics:");
    let num_specs = index.specs.len();
    println!("  - Spec files: {}", num_specs);
    if num_specs > 0 {
        println!("  - Average lines per spec: {}", total_lines / num_specs);
        println!("  - Average contracts per spec: {}", total_contracts / num_specs);
    }

    Ok(())
}
