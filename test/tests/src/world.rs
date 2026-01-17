//! TestWorld - A newtype wrapper around Scenario for BDD tests.
//!
//! # Architecture Rule (LOCKED)
//!
//! `TestWorld` MUST be a newtype wrapper around `Option<Scenario>` ONLY.
//! All test state lives in `Scenario`. Do NOT add fields to `TestWorld`.
//! Step bindings delegate to `Scenario` APIs for all operations.
//!
//! # Context-First ABI
//!
//! Steps use capability-separated context types:
//! - Given/When steps: `TestWorldMut` (mutation operations only)
//! - Then steps: `TestWorldRef` (read/assertion operations only)

use namako::World;
use namako::codegen::StepContext;
use naia_test_harness::Scenario;

/// The World type for Naia BDD tests.
///
/// This is a newtype wrapper around `Option<Scenario>`.
/// All test state lives in `Scenario` - do NOT add fields here.
#[derive(World, Default)]
#[world(mut_ctx = TestWorldMut<'a>, ref_ctx = TestWorldRef<'a>)]
pub struct TestWorld(Option<Scenario>);

impl TestWorld {
    /// Get the scenario as mutable, panicking if not initialized.
    /// Use this in Given/When steps.
    pub fn scenario_mut(&mut self) -> &mut Scenario {
        self.0.as_mut().expect("Scenario not initialized - call a Given step first")
    }

    /// Get the scenario as immutable, panicking if not initialized.
    /// Use this in Then steps.
    pub fn scenario(&self) -> &Scenario {
        self.0.as_ref().expect("Scenario not initialized - call a Given step first")
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

// =============================================================================
// Context Types for Capability Separation
// =============================================================================

/// Mutable context for Given/When steps.
///
/// This context provides ONLY mutation operations. Assertions and expect
/// operations are not available through this context.
pub struct TestWorldMut<'a>(&'a mut TestWorld);

impl<'a> TestWorldMut<'a> {
    /// Create a new mutable context from a TestWorld.
    pub fn new(world: &'a mut TestWorld) -> Self {
        Self(world)
    }

    /// Get mutable access to the scenario.
    /// Use this for mutation operations in Given/When steps.
    pub fn scenario_mut(&mut self) -> &mut Scenario {
        self.0.scenario_mut()
    }

    /// Initialize with a new scenario.
    pub fn init(&mut self) -> &mut Scenario {
        self.0.init()
    }

    /// Check if scenario is initialized.
    pub fn is_initialized(&self) -> bool {
        self.0.is_initialized()
    }
}

impl StepContext for TestWorldMut<'_> {
    type World = TestWorld;
}

/// Read-only context for Then steps.
///
/// This context provides ONLY read/assertion operations. Mutation operations
/// are not available through this context.
///
/// Note: Takes `&'a mut TestWorld` internally because expect operations may
/// need to tick simulation, but the public API only exposes read operations.
pub struct TestWorldRef<'a>(&'a mut TestWorld);

impl<'a> TestWorldRef<'a> {
    /// Create a new read-only context from a TestWorld.
    pub fn new(world: &'a mut TestWorld) -> Self {
        Self(world)
    }

    /// Get immutable access to the scenario.
    /// Use this for assertions in Then steps.
    pub fn scenario(&self) -> &Scenario {
        self.0.scenario()
    }

    /// Check if scenario is initialized.
    pub fn is_initialized(&self) -> bool {
        self.0.is_initialized()
    }
}

impl StepContext for TestWorldRef<'_> {
    type World = TestWorld;
}
