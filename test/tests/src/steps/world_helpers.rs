//! Shared imperative helpers used by step bindings.
//!
//! **Purpose:** absorb the repeated mutate/expect/until boilerplate so
//! a typical binding becomes ≤ 6 LOC instead of the current 18 LOC
//! median.
//!
//! ## Helper catalog (target — landing in Phase B)
//!
//! - `tick_once(world)` — one server-client tick exchange
//! - `tick_until(world, n_max, predicate)` — poll until or fail
//! - `with_server(world, |s| ...)` — server-mut closure
//! - `with_client(world, name, |c| ...)` — client-mut closure by name
//! - `expect_server_event(world, |e| ...)` — wait for server event
//! - `expect_client_event(world, name, |e| ...)` — wait for client event
//! - `store_entity(world, key, entity)` — BDD-storage typed wrapper
//! - `lookup_entity(world, key) -> Entity` — typed lookup (panics if absent)
//! - `lookup_client_key(world, name) -> ClientKey` — client name → key
//!
//! Phase A.1: file created as a stub. Helpers land in Phase B.

// Stub. See `_AGENTS/SDD_MIGRATION_PLAN.md` Phase B for content plan.
