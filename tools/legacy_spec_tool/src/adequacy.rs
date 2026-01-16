use anyhow::Result;
use crate::index::Index;

const RED: &str = "\x1b[0;31m";
const GREEN: &str = "\x1b[0;32m";
const YELLOW: &str = "\x1b[1;33m";
const NC: &str = "\x1b[0m";

/// Check if a label covers an obligation.
/// Accepts these canonical formats:
/// - Exact: `cid.obl`
/// - Prefix: `cid.obl:` (and anything after)
/// - Exact: `cid: obl`
/// - Prefix: `cid: obl:` (and anything after)
fn label_covers_obligation(label: &str, cid: &str, obl: &str) -> bool {
    let dot_format = format!("{}.{}", cid, obl);
    let space_format = format!("{}: {}", cid, obl);

    // Exact match or prefix match with colon
    label == dot_format || label.starts_with(&format!("{}:", dot_format)) ||
    label == space_format || label.starts_with(&format!("{}:", space_format))
}

pub fn run_adequacy(index: &Index, strict: bool) -> Result<()> {
    // Header
    println!("\n{}═══════════════════════════════════════════════════════════════{}", "\x1b[0;34m", NC);
    println!("  Contract Adequacy Analysis");
    println!("{}═══════════════════════════════════════════════════════════════{}", "\x1b[0;34m", NC);

    let total_contracts = index.contracts.len();
    println!("  Found {} contracts in specs\n", total_contracts);

    println!("{}═══════════════════════════════════════════════════════════════{}", "\x1b[0;34m", NC);
    println!("  Adequacy To-Do Queue (Ranked)");
    println!("{}═══════════════════════════════════════════════════════════════{}", "\x1b[0;34m", NC);

    // Categorize
    let mut needs_tests = Vec::new();
    let mut needs_labels = Vec::new(); // Has tests but 0 labels
    let mut missing_obligations = Vec::new(); // Has tests and labels, but missing specific obligations

    let mut contracts_sorted: Vec<_> = index.contracts.keys().collect();
    contracts_sorted.sort();

    for cid in contracts_sorted {
        if !index.contract_to_test_files.contains_key(cid) {
            needs_tests.push(cid);
            continue;
        }

        let labels = index.contract_to_labels.get(cid);
        // If labels is None or empty
        if labels.map_or(true, |s| s.is_empty()) {
            needs_labels.push(cid);
            continue;
        }

        // Check missing obligations
        // Get expected obligations
        let mut missing = Vec::new();
        if let Some(obligations) = index.contract_obligations.get(cid) {
            let found = labels.unwrap();
            for obl in obligations {
                // Check if any label covers this obligation
                if !found.iter().any(|label| label_covers_obligation(label, cid, obl)) {
                     missing.push(obl);
                }
            }
        }
        
        if !missing.is_empty() {
             missing_obligations.push((cid, missing));
        }
    }

    let needs_tests_count = needs_tests.len();
    let missing_obligations_count = missing_obligations.len();
    let needs_labels_count = needs_labels.len();
    
    let mut has_issues = false;

    // Priority 1: Missing Tests
    if needs_tests_count > 0 {
        has_issues = true;
        println!("{}━━━ Priority 1: Missing Tests ({} contracts) ━━━{}", RED, needs_tests_count, NC);
        println!();
        for cid in needs_tests {
             let contract = index.contracts.get(cid).unwrap();
             let spec_file = contract.file_path.file_name().unwrap().to_string_lossy();
             
             println!("  ❌ {}", cid);
             println!("     Spec: {}", spec_file);
             println!("     Status: No test functions annotated");
             println!("     Next: Write test with /// Contract: [{}] annotation", cid);
             println!();
        }
    }

    // Priority 2: Missing Obligation Mappings
    if missing_obligations_count > 0 {
        has_issues = true;
        println!("{}━━━ Priority 2: Missing Obligation Mappings ({} contracts) ━━━{}", YELLOW, missing_obligations_count, NC);
        println!();
        for (cid, missing) in missing_obligations {
             let contract = index.contracts.get(cid).unwrap();
             let spec_file = contract.file_path.file_name().unwrap().to_string_lossy();
             
             let test_files = index.contract_to_test_files.get(cid).unwrap();
             let mut tfiles: Vec<_> = test_files.iter().map(|s| s.as_str()).collect();
             tfiles.sort();
             let tfiles_str = tfiles.join(", ");
             let missing_str = missing.into_iter().map(|s| s.to_string()).collect::<Vec<_>>().join(", ");

             println!("  ⚠️  {}", cid);
             println!("     Spec: {}", spec_file);
             println!("     Tests: {}", tfiles_str);
             println!("     Missing obligations: {}", missing_str);
             println!("     Next: cargo run -p naia_spec_tool -- packet {}", cid);
             println!();
        }
    }

    // Priority 3: Missing Labels
    if needs_labels_count > 0 {
        has_issues = true;
        println!("{}━━━ Priority 3: Missing Labels ({} contracts) ━━━{}", YELLOW, needs_labels_count, NC);
        println!();
         for cid in needs_labels {
             let contract = index.contracts.get(cid).unwrap();
             let spec_file = contract.file_path.file_name().unwrap().to_string_lossy();
             
             let test_files = index.contract_to_test_files.get(cid).unwrap();
             let mut tfiles: Vec<_> = test_files.iter().map(|s| s.as_str()).collect();
             tfiles.sort();
             let tfiles_str = tfiles.join(", ");

             println!("  ⚠️  {}", cid);
             println!("     Spec: {}", spec_file);
             println!("     Tests: {}", tfiles_str);
             println!("     Status: Tests exist but no labeled assertions");
             println!("     Next: Add spec_expect(\"{}: ...\") to tests", cid);
             println!();
        }
    }

    let ok_count = total_contracts - needs_tests_count - needs_labels_count - missing_obligations_count;

    if ok_count > 0 {
         println!("{}━━━ OK: Adequacy Met ({} contracts) ━━━{}", GREEN, ok_count, NC);
         println!();
         println!("  All obligations mapped to labeled assertions (or no obligations with contract-level label).");
         println!();
    }

    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!();
    println!("Summary:");
    println!("  Total contracts:             {}", total_contracts);
    println!("  Missing tests:               {}", needs_tests_count);
    println!("  Missing obligation mappings: {}", missing_obligations_count);
    println!("  Missing labels:              {}", needs_labels_count);
    println!("  Adequacy met:                {}", ok_count);
    println!();

    if has_issues {
        if strict {
            println!("{}✗{} Adequacy check failed (--strict mode)", RED, NC);
            return Err(anyhow::anyhow!("Adequacy check failed"));
        } else {
             println!("{}⚠{} Adequacy issues found (use --strict to fail on issues)", YELLOW, NC);
        }
    } else {
        println!("{}✓{} All contracts meet adequacy requirements! 🎉", GREEN, NC);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_label_covers_obligation_dot_format_exact() {
        assert!(label_covers_obligation("messaging-04.t1", "messaging-04", "t1"));
    }

    #[test]
    fn test_label_covers_obligation_dot_format_with_description() {
        assert!(label_covers_obligation(
            "messaging-04.t1: mismatched protocol_id rejects connection",
            "messaging-04",
            "t1"
        ));
    }

    #[test]
    fn test_label_covers_obligation_space_format_exact() {
        assert!(label_covers_obligation("messaging-04: t1", "messaging-04", "t1"));
    }

    #[test]
    fn test_label_covers_obligation_space_format_with_description() {
        assert!(label_covers_obligation(
            "messaging-04: t1: some description",
            "messaging-04",
            "t1"
        ));
    }

    #[test]
    fn test_label_covers_obligation_rejects_wrong_obligation() {
        assert!(!label_covers_obligation("messaging-04.t10", "messaging-04", "t1"));
        assert!(!label_covers_obligation("messaging-04.t10: desc", "messaging-04", "t1"));
    }

    #[test]
    fn test_label_covers_obligation_rejects_wrong_contract() {
        assert!(!label_covers_obligation("messaging-040.t1", "messaging-04", "t1"));
        assert!(!label_covers_obligation("messaging-040.t1: desc", "messaging-04", "t1"));
    }

    #[test]
    fn test_label_covers_obligation_rejects_partial_match() {
        assert!(!label_covers_obligation("messaging-04.t1x", "messaging-04", "t1"));
        assert!(!label_covers_obligation("messaging-04.t1x: desc", "messaging-04", "t1"));
    }

    #[test]
    fn test_label_covers_obligation_rejects_substring() {
        assert!(!label_covers_obligation("prefix-messaging-04.t1", "messaging-04", "t1"));
    }
}
