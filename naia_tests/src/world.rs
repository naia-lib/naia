//! TestWorld - A newtype wrapper around Scenario for BDD tests.
//!
//! # Architecture Rule (LOCKED)
//! 
//! `TestWorld` MUST be a newtype wrapper around `Option<Scenario>` ONLY.
//! All test state lives in `Scenario`. Do NOT add fields to `TestWorld`.
//! Step bindings delegate to `Scenario` APIs for all operations.

use namako::World;
use naia_test_harness::Scenario;

/// The World type for Naia BDD tests.
/// 
/// This is a newtype wrapper around `Option<Scenario>`.
/// All test state lives in `Scenario` - do NOT add fields here.
#[derive(World, Default)]
pub struct TestWorld(Option<Scenario>);

impl TestWorld {
    /// Get the scenario, panicking if not initialized.
    pub fn scenario(&mut self) -> &mut Scenario {
        self.0.as_mut().expect("Scenario not initialized - call a Given step first")
    }

    /// Initialize with a new scenario.
    pub fn init(&mut self) -> &mut Scenario {
        self.0.insert(Scenario::new())
    }

    /// Check if scenario is initialized.
    pub fn is_initialized(&self) -> bool {
        self.0.is_some()
    }
}

impl std::fmt::Debug for TestWorld {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("TestWorld")
            .field(&self.0.as_ref().map(|_| "Scenario { ... }"))
            .finish()
    }
}
