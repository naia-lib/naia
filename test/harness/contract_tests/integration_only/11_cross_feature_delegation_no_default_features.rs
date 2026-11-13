//! OFF arm of `11_cross_feature_delegation.rs`, as its own cargo test target.
//!
//! The source file splits on `feature = "entity_delegation"`. Default features
//! run only its ON arm. This target includes the same source and is compiled
//! only when the feature is off, so `cargo test --no-default-features` runs the
//! OFF arm exactly once and default-feature builds run no tests here.

#![cfg(not(feature = "entity_delegation"))]

#[path = "11_cross_feature_delegation.rs"]
mod cross_feature_delegation;
