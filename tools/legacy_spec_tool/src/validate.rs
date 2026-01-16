use std::path::PathBuf;
use crate::{lint, check_refs, check_pairing, check_isolation, check_annotations, check_label_literals, check_label_scoping, check_obligations};
use crate::util::{print_header, print_success, print_error};

pub fn run_validate(root: &PathBuf) -> anyhow::Result<usize> {
    print_header("Full Specification Validation");

    let mut total_errors: usize = 0;

    println!("Running lint...");
    match lint::run_lint(root) {
        Ok(count) => total_errors += count,
        Err(e) => return Err(e),
    }

    println!();
    println!("Running check-refs...");
    match check_refs::run_check_refs(root) {
        Ok(count) => total_errors += count,
        Err(e) => return Err(e),
    }

    println!();
    println!("Running check-pairing...");
    match check_pairing::run_check_pairing(root) {
        Ok(count) => total_errors += count,
        Err(e) => return Err(e),
    }

    println!();
    println!("Running check-annotations...");
    match check_annotations::run_check_annotations(root) {
        Ok(count) => total_errors += count,
        Err(e) => return Err(e),
    }

    println!();
    println!("Running check-label-literals...");
    match check_label_literals::run_check_label_literals(root) {
        Ok(count) => total_errors += count,
        Err(e) => return Err(e),
    }

    println!();
    println!("Running check-label-scoping...");
    match check_label_scoping::run_check_label_scoping(root) {
        Ok(count) => total_errors += count,
        Err(e) => return Err(e),
    }

    println!();
    println!("Running check-obligations...");
    match check_obligations::run_check_obligations(root) {
        Ok(count) => total_errors += count,
        Err(e) => return Err(e),
    }

    println!();
    println!("Running check-isolation...");
    match check_isolation::run_check_isolation(root) {
        Ok(count) => total_errors += count,
        Err(e) => return Err(e),
    }

    println!();
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    if total_errors == 0 {
        print_success("All validation checks passed!");
    } else {
        print_error(&format!("Validation failed with {} errors", total_errors));
    }

    Ok(total_errors)
}
