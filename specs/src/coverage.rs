use anyhow::Result;
use crate::index::Index;

const RED: &str = "\x1b[0;31m";
const GREEN: &str = "\x1b[0;32m";
const YELLOW: &str = "\x1b[1;33m";
const BLUE: &str = "\x1b[0;34m";
const NC: &str = "\x1b[0m";

pub fn run_coverage(index: &Index) -> Result<()> {
    println!("\n{}═══════════════════════════════════════════════════════════════{}\n{}  Contract Coverage Analysis{}\n{}═══════════════════════════════════════════════════════════════{}\n", 
        BLUE, NC, BLUE, NC, BLUE, NC);

    let mut all_contracts: Vec<_> = index.contracts.keys().collect();
    all_contracts.sort();
    let covered_contracts: Vec<_> = index.contract_to_test_files.keys().collect();
    
    let total_count = all_contracts.len();
    let covered_count = covered_contracts.len();
    
    if total_count == 0 {
        println!("{}✗{} No contracts found. Run ./spec_tool.sh registry first.", RED, NC);
        return Err(anyhow::anyhow!("No contracts found"));
    }

    let coverage_pct = if total_count > 0 {
        (covered_count * 100) / total_count
    } else {
        0
    };

    println!("Coverage Summary");
    println!("━━━━━━━━━━━━━━━━");
    println!("Contracts with test annotations: {}", covered_count);
    println!("Total contracts in registry:     {}", total_count);
    println!("Coverage:                        {}%\n", coverage_pct);

    // Uncovered
    let mut uncovered = Vec::new();
    for contract in &all_contracts {
        if !index.contract_to_test_files.contains_key(*contract) {
            uncovered.push(*contract);
        }
    }
    
    if !uncovered.is_empty() {
        println!("Uncovered Contracts ({}):", uncovered.len());
        println!("━━━━━━━━━━━━━━━━━━━━");
        for id in uncovered {
            println!("  - {}", id);
        }
    } else {
        println!("{}✓{} All contracts have test annotations!", GREEN, NC);
    }

    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    if coverage_pct >= 80 {
        println!("{}✓{} Coverage target met (≥80%)", GREEN, NC);
    } else {
        println!("{}⚠{} Coverage below target (<80%)", YELLOW, NC);
    }

    Ok(())
}
