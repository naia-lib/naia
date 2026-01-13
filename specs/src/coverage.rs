use anyhow::Result;
use crate::index::Index;
use crate::util::{print_header, print_success, print_warning};

pub fn run_coverage(index: &Index) -> Result<(usize, usize)> {
    print_header("Contract Coverage Analysis");

    let mut all_contracts: Vec<_> = index.contracts.keys().collect();
    all_contracts.sort();
    let covered_contracts: Vec<_> = index.contract_to_test_files.keys().collect();
    
    let total_count = all_contracts.len();
    let covered_count = covered_contracts.len();
    
    if total_count == 0 {
        return Err(anyhow::anyhow!("No contracts found. Run 'cargo run -p naia_spec_tool -- registry' first."));
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
        print_success("All contracts have test annotations!");
    }

    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    if coverage_pct >= 80 {
        print_success("Coverage target met (≥80%)");
    } else {
        print_warning("Coverage below target (<80%)");
    }

    Ok((covered_count, total_count))
}
