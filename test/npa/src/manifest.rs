//! `naia_namako manifest` command implementation.
//!
//! Outputs the semantic step registry as JSON to stdout.

use anyhow::Result;
use namako_engine::codegen::{inventory, StepConstructor, WorldInventory};
use namako_engine::npap::{BindingSignature, CustomParameterDef, SemanticBinding, SemanticStepRegistry};

use naia_tests::TestWorld;

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
            source_symbol: Some(meta.source_symbol.to_string()),
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
            source_symbol: Some(meta.source_symbol.to_string()),
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
            source_symbol: Some(meta.source_symbol.to_string()),
        });
    }

    bindings
}

/// Custom parameter types from `naia_tests/steps/vocab.rs`.
///
/// Regexes must mirror the `#[param(regex = ...)]` attributes in vocab.rs exactly.
fn naia_custom_parameters() -> Vec<CustomParameterDef> {
    vec![
        CustomParameterDef { name: "client".to_string(),    regex: r"[A-Za-z][A-Za-z0-9_]*".to_string() },
        CustomParameterDef { name: "entity".to_string(),    regex: r"[a-z][a-z0-9_]*".to_string() },
        CustomParameterDef { name: "component".to_string(), regex: r"[A-Z][A-Za-z0-9]*".to_string() },
        CustomParameterDef { name: "channel".to_string(),   regex: r"[A-Z][A-Za-z0-9]*".to_string() },
        CustomParameterDef { name: "role".to_string(),      regex: r"granted|denied|available|requested|releasing".to_string() },
        CustomParameterDef { name: "room".to_string(),      regex: r"[a-z][a-z0-9_]*".to_string() },
        CustomParameterDef { name: "message".to_string(),   regex: r"[A-Z][A-Za-z0-9]*".to_string() },
    ]
}

/// Run the manifest command.
pub fn run() -> Result<()> {
    // Collect all bindings from inventory (macro-generated at compile time)
    let bindings = collect_bindings_from_inventory::<TestWorld>();

    // Create the registry (computes step_registry_hash internally)
    let registry = SemanticStepRegistry::new_with_params(bindings, naia_custom_parameters());

    // Output as JSON to stdout
    let json = serde_json::to_string_pretty(&registry)?;
    println!("{}", json);

    Ok(())
}
