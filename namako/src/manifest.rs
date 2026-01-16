//! `naia_namako manifest` command implementation.
//!
//! Outputs the semantic step registry as JSON to stdout.

use anyhow::Result;
use namako::npap::{SemanticStepRegistry, SemanticBinding, BindingSignature};
use namako::codegen::{StepConstructor, WorldInventory, inventory};

use naia_tests::SmokeWorld;

/// Collect bindings from inventory for the given World type.
fn collect_bindings_from_inventory<W: WorldInventory>() -> Vec<SemanticBinding> {
    let mut bindings = Vec::new();

    // Collect Given steps
    for step in inventory::iter::<W::Given> {
        let meta = StepConstructor::<W>::npap_metadata(step);
        bindings.push(SemanticBinding {
            binding_id: meta.binding_id.to_string(),
            kind: meta.kind.to_string(),
            expression: meta.expression.to_string(),
            signature: BindingSignature {
                captures_arity: meta.captures_arity,
                accepts_docstring: meta.accepts_docstring,
                accepts_datatable: meta.accepts_datatable,
            },
            impl_hash: meta.impl_hash.to_string(),
        });
    }

    // Collect When steps
    for step in inventory::iter::<W::When> {
        let meta = StepConstructor::<W>::npap_metadata(step);
        bindings.push(SemanticBinding {
            binding_id: meta.binding_id.to_string(),
            kind: meta.kind.to_string(),
            expression: meta.expression.to_string(),
            signature: BindingSignature {
                captures_arity: meta.captures_arity,
                accepts_docstring: meta.accepts_docstring,
                accepts_datatable: meta.accepts_datatable,
            },
            impl_hash: meta.impl_hash.to_string(),
        });
    }

    // Collect Then steps
    for step in inventory::iter::<W::Then> {
        let meta = StepConstructor::<W>::npap_metadata(step);
        bindings.push(SemanticBinding {
            binding_id: meta.binding_id.to_string(),
            kind: meta.kind.to_string(),
            expression: meta.expression.to_string(),
            signature: BindingSignature {
                captures_arity: meta.captures_arity,
                accepts_docstring: meta.accepts_docstring,
                accepts_datatable: meta.accepts_datatable,
            },
            impl_hash: meta.impl_hash.to_string(),
        });
    }

    bindings
}

/// Run the manifest command.
pub fn run() -> Result<()> {
    // Collect all bindings from inventory (macro-generated at compile time)
    let bindings = collect_bindings_from_inventory::<SmokeWorld>();

    // Create the registry (computes step_registry_hash internally)
    let registry = SemanticStepRegistry::new(bindings);

    // Output as JSON to stdout
    let json = serde_json::to_string_pretty(&registry)?;
    println!("{}", json);

    Ok(())
}
