//! Step definition modules.
//!
//! ## Phase A architecture (in progress — see `_AGENTS/SDD_MIGRATION_PLAN.md`)
//!
//! The catalog is being reorganized from contract-aligned modules
//! (one .rs file per `.feature` file) to **purpose-aligned modules**:
//!
//! - `vocab` — parameter vocabulary (typed wrappers + parser convention)
//! - `world_helpers` — reusable mutate/expect/tick helpers
//! - `given/` — preconditions (split: setup, state)
//! - `when/`  — actions (split: server_actions, client_actions, network_events)
//! - `then/`  — assertions (split: state_assertions, event_assertions, ordering)
//!
//! Phase A.1 lands the new module skeleton (this commit). Phase A.3
//! moves the contract-aligned bindings into the purpose-aligned modules.
//! Until A.3 lands, BOTH structures coexist; only the contract-aligned
//! modules contain real bindings. The `vocab`/`world_helpers`/given/when/then
//! modules are stubs.

pub mod vocab;
pub mod world_helpers;

pub mod given;
pub mod when;
pub mod then;

// ──────────────────────────────────────────────────────────────────────
// Contract-aligned modules (phase A pre-refactor — being migrated to
// purpose-aligned modules in A.3, then deleted)
// ──────────────────────────────────────────────────────────────────────

pub mod client_events;
pub mod common;
pub mod connection;
pub mod entity_authority;
pub mod entity_delegation;
pub mod entity_ownership;
pub mod entity_publication;
pub mod entity_replication;
pub mod entity_scopes;
pub mod messaging;
pub mod observability;
pub mod priority_accumulator;
pub mod replicated_resources;
pub mod scope_exit;
// `scope_propagation` module migrated 2026-05-06: 1 when, 1 then.
pub mod server_events;
// `update_candidate_set` module migrated 2026-05-06: 1 given, 1 when, 1 then.
// `spawn_with_components` module migrated 2026-05-06: 2 given, 2 then.
// `immutable_components` module migrated 2026-05-06: 2 given, 2 then.
// `smoke` module migrated 2026-05-06: 2 given, 2 when, 2 then.
pub mod transport;
pub mod world_integration;

// ABI proof tests (compile_fail demonstrations - permanently disabled)
#[cfg(any())]
mod _abi_proofs;
