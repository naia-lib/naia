//! Hierarchical test suite for the new sync engine.
//! Each sub-module corresponds to a step in the test-driven refactor plan
//! described in `REFACTOR_PLAN.md`.

mod bulletproof_migration;
mod command_validation_tests;
mod engine;
mod integration_migration;
mod migration;
mod perfect_migration_tests;
mod real_migration_tests;
