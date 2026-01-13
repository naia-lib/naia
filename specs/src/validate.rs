use std::path::PathBuf;
use crate::{lint, check_refs, check_orphans};
use crate::util::{print_header, print_success, print_error};

pub fn run_validate(root: &PathBuf) -> anyhow::Result<i32> {
    print_header("Full Specification Validation");

    let mut total_errors = 0;

    println!("Running lint...");
    // If lint fails inside, it returns count.
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
    println!("Running check-orphans...");
    check_orphans::run_check_orphans(root)?;

    println!();
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    if total_errors == 0 {
        print_success("All validation checks passed!");
    } else {
        print_error(&format!("Validation failed with {} errors", total_errors));
    }

    Ok(total_errors)
}
