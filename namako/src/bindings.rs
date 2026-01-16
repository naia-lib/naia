//! Step bindings for Naia smoke tests.
//!
//! These bindings implement the steps required by the smoke feature file.

use namako::npap::{SemanticBinding, BindingSignature, generate_binding_id, blake3_256_lowerhex};

/// All smoke test bindings for registry generation.
///
/// For v1, we manually define these. In production, these would be
/// collected via inventory from `#[given]`, `#[when]`, `#[then]` macros.
pub fn smoke_bindings() -> Vec<SemanticBinding> {
    vec![
        // Given a server is running
        SemanticBinding {
            binding_id: generate_binding_id("Given", "a server is running"),
            kind: "Given".to_string(),
            expression: "a server is running".to_string(),
            signature: BindingSignature {
                captures_arity: 0,
                accepts_docstring: false,
                accepts_datatable: false,
            },
            impl_hash: compute_impl_hash_static("given_a_server_is_running_v1"),
        },
        // When a client connects
        SemanticBinding {
            binding_id: generate_binding_id("When", "a client connects"),
            kind: "When".to_string(),
            expression: "a client connects".to_string(),
            signature: BindingSignature {
                captures_arity: 0,
                accepts_docstring: false,
                accepts_datatable: false,
            },
            impl_hash: compute_impl_hash_static("when_a_client_connects_v1"),
        },
        // Then the server has {int} connected client(s)
        SemanticBinding {
            binding_id: generate_binding_id("Then", "the server has {int} connected client(s)"),
            kind: "Then".to_string(),
            expression: "the server has {int} connected client(s)".to_string(),
            signature: BindingSignature {
                captures_arity: 1,
                accepts_docstring: false,
                accepts_datatable: false,
            },
            impl_hash: compute_impl_hash_static("then_server_has_n_clients_v1"),
        },
        // Given a client connects (reuses "When" binding via And)
        // Note: The engine handles And/But → effective kind mapping
        SemanticBinding {
            binding_id: generate_binding_id("Given", "a client connects"),
            kind: "Given".to_string(),
            expression: "a client connects".to_string(),
            signature: BindingSignature {
                captures_arity: 0,
                accepts_docstring: false,
                accepts_datatable: false,
            },
            impl_hash: compute_impl_hash_static("given_a_client_connects_v1"),
        },
        // When the server disconnects the client
        SemanticBinding {
            binding_id: generate_binding_id("When", "the server disconnects the client"),
            kind: "When".to_string(),
            expression: "the server disconnects the client".to_string(),
            signature: BindingSignature {
                captures_arity: 0,
                accepts_docstring: false,
                accepts_datatable: false,
            },
            impl_hash: compute_impl_hash_static("when_server_disconnects_client_v1"),
        },
    ]
}

/// Compute impl_hash for a static binding fingerprint.
/// In production, this comes from token fingerprinting at compile time.
fn compute_impl_hash_static(fingerprint: &str) -> String {
    let input = format!("token-fingerprint-v1|{}", fingerprint);
    blake3_256_lowerhex(input.as_bytes())
}
