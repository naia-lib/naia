//! `naia_namako manifest` command implementation.
//!
//! Outputs the semantic step registry as JSON to stdout.

use anyhow::Result;
use namako::npap::SemanticStepRegistry;

use crate::bindings::smoke_bindings;

/// Run the manifest command.
pub fn run() -> Result<()> {
    // Collect all bindings
    let bindings = smoke_bindings();

    // Create the registry (computes step_registry_hash internally)
    let registry = SemanticStepRegistry::new(bindings);

    // Output as JSON to stdout
    let json = serde_json::to_string_pretty(&registry)?;
    println!("{}", json);

    Ok(())
}
