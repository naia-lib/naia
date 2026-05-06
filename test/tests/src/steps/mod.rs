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

pub mod prelude;
pub mod vocab;
pub mod world_helpers;

pub mod given;
pub mod when;
pub mod then;

// ──────────────────────────────────────────────────────────────────────
// Contract-aligned modules (phase A pre-refactor — being migrated to
// purpose-aligned modules in A.3, then deleted)
// ──────────────────────────────────────────────────────────────────────

// `client_events` module migrated 2026-05-06: 5 then → event_assertions.
// `common` module migrated 2026-05-06: 9 given→{setup,state},
// 8 when→{network_events,client_actions}, 11 then→state_assertions,
// 5 then→ordering. Last contract file migrated.
// `connection` module migrated 2026-05-06: 3 given→setup,
// 3 when→network_events, 9 then→event_assertions, 3 then→state_assertions,
// 5 then→ordering.
// `entity_authority` module migrated 2026-05-06: 1 given→state,
// 1 when→client_actions, 3 then→event_assertions, 2 then→state_assertions.
// `entity_delegation` module migrated 2026-05-06: 4 given→state,
// 2 when→client_actions, 3 when→server_actions, 1 when→network_events,
// 6 then→state_assertions.
// `entity_ownership` module migrated 2026-05-06: 1 given→state,
// 2 when→client_actions, 5 then→state_assertions.
// `entity_publication` module migrated 2026-05-06: 5 given→{setup,state},
// 2 when→client_actions, 5 then→state_assertions.
// `entity_replication` module migrated 2026-05-06: 3 given→state,
// 2 when→server_actions, 4 then→state_assertions.
// `entity_scopes` module migrated 2026-05-06: 8 given→state,
// 4 when→server_actions, 1 when→network_events, 7 then→state_assertions.
// `messaging` module migrated 2026-05-06: 2 when→client_actions,
// 3 when→server_actions, 4 then→state_assertions.
// `observability` module migrated 2026-05-06: 6 given→{setup,state},
// 8 when→{network_events,client_actions}, 7 then→state_assertions.
// Helper `disconnect_last_client` extracted.
// `priority_accumulator` module migrated 2026-05-06: 1 given→state,
// 3 when→server_actions, 5 then→state_assertions.
// `replicated_resources` module migrated 2026-05-06: 5 given→{setup,state},
// 1 when→server_actions, 2 when→network_events, 1 when→client_actions, 4 then→state_assertions.
// Helper `ensure_server_started` extracted.
// `scope_exit` module migrated 2026-05-06: 3 given→state,
// 4 when→server_actions, 1 when→network_events, 5 then→state_assertions.
// `scope_propagation` module migrated 2026-05-06: 1 when, 1 then.
// `server_events` module migrated 2026-05-06: 2 given→state,
// 1 when→server_actions, 5 then→event_assertions.
// `update_candidate_set` module migrated 2026-05-06: 1 given, 1 when, 1 then.
// `spawn_with_components` module migrated 2026-05-06: 2 given, 2 then.
// `immutable_components` module migrated 2026-05-06: 2 given, 2 then.
// `smoke` module migrated 2026-05-06: 2 given, 2 when, 2 then.
// `transport` module migrated 2026-05-06: 1 given→setup,
// 2 when→server_actions, 2 when→client_actions, 9 when→network_events,
// 7 then→state_assertions. Helper `panic_payload_to_string` extracted.
// `world_integration` module migrated 2026-05-06: 1 when→network_events,
// 1 when→server_actions, 3 then→state_assertions.

// ABI proof tests (compile_fail demonstrations - permanently disabled)
#[cfg(any())]
mod _abi_proofs;
