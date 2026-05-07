// Phase B transitional: prelude re-exports are not yet consumed by
// every binding file (B.3 refactor pass is in progress). Suppress the
// unused-import lint until the refactor lands. Once every binding
// module ends with `use crate::steps::prelude::*`, this attribute can
// be removed because the re-exports will all have in-crate consumers.
#![allow(unused_imports)]

//! Step-binding prelude — common imports for given/when/then bindings.
//!
//! **Purpose:** every binding file under `given/`, `when/`, and
//! `then/` opens with the same imports (cucumber macros, the
//! `AssertOutcome` enum, the test world contexts, common harness
//! types). Pulling them into a single prelude lets each file write
//! `use crate::steps::prelude::*;` and skip the boilerplate.
//!
//! ## Discipline
//!
//! - Keep this prelude lean. Only types/functions referenced by ≥3
//!   binding files belong here.
//! - Don't re-export `Scenario` or harness internals — those should
//!   stay explicit at call sites where they appear.
//! - Don't shadow standard names. The prelude is `pub use`'d
//!   wholesale, so any name conflict ripples across the catalog.

// Cucumber-rs binding macros.
pub use namako_engine::{given, then, when};

// Outcome enum used by polling Then bindings.
pub use namako_engine::codegen::AssertOutcome;

// Test world contexts.
pub use crate::{TestWorldMut, TestWorldRef};

// Most-used helpers + BDD-store keys from `world_helpers`.
pub use crate::steps::world_helpers::{
    client_key_storage, disconnect_last_client, entity_label_to_key_storage,
    panic_payload_to_string, CLIENT_LOCAL_VALUE_KEY, ENTITY_A_KEY, ENTITY_B_KEY,
    INITIAL_ENTITY_KEY, LAST_COMPONENT_VALUE_KEY, LAST_ENTITY_KEY, LAST_REQUEST_ERROR_KEY,
    RESPONSE_RECEIVE_KEY, SECOND_CLIENT_KEY, SPAWN_BURST_KEYS, SPAWN_POSITION_VALUE_KEY,
    SPAWN_VELOCITY_VALUE_KEY, WRITE_REJECTED_KEY,
};
// Connect-handshake and entity helpers (split into world_helpers_connect).
pub use crate::steps::world_helpers_connect::{
    connect_client, connect_named_client, connect_test_client, ensure_server_started,
};
