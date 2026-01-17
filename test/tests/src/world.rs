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
//! - Then steps: `TestWorldRef` (read/assertion operations only, wraps ExpectCtx)

use namako::World;
use namako::codegen::StepContext;
use naia_test_harness::{ClientKey, ExpectCtx, Scenario, TrackedClientEvent, TrackedServerEvent};

/// The World type for Naia BDD tests.
///
/// This is a newtype wrapper around `Option<Scenario>`.
/// All test state lives in `Scenario` - do NOT add fields here.
///
/// Note: `ref_ctx` is not specified because Then steps use a custom path
/// that creates `TestWorldRef` from `ExpectCtx` inside the polling loop.
#[derive(World, Default)]
#[world(mut_ctx = TestWorldMut<'a>)]
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
/// This context wraps an `ExpectCtx` and provides ONLY read/assertion operations.
/// Mutation operations are not available through this context.
///
/// Uses a raw pointer for interior mutability to allow `&self` methods to
/// call ExpectCtx methods that need `&mut self`.
pub struct TestWorldRef<'a> {
    // Raw pointer to ExpectCtx - valid for the lifetime of the closure
    ctx: *mut ExpectCtx<'a>,
}

impl<'a> TestWorldRef<'a> {
    /// Create a new read-only context from an ExpectCtx.
    /// Called by the generated macro wrapper inside the polling loop.
    pub fn new(ctx: &mut ExpectCtx<'a>) -> Self {
        Self { ctx: ctx as *mut ExpectCtx<'a> }
    }

    /// Get access to the underlying ExpectCtx.
    /// Safety: We ensure single-threaded access and the pointer remains valid.
    fn ctx(&self) -> &mut ExpectCtx<'a> {
        // Safety: Tests are single-threaded, pointer is valid for 'a
        unsafe { &mut *self.ctx }
    }

    /// Get the scenario (for delegation).
    fn scenario_ref(&self) -> &Scenario {
        self.ctx().scenario()
    }

    /// Get the current tick count.
    pub fn global_tick(&self) -> usize {
        self.ctx().global_tick()
    }

    /// Get the last client key started.
    pub fn last_client(&self) -> ClientKey {
        self.scenario_ref().last_client()
    }

    /// Check server event ordering.
    pub fn server_event_before(&self, a: TrackedServerEvent, b: TrackedServerEvent) -> bool {
        self.scenario_ref().server_event_before(a, b)
    }

    /// Check if client observed an event.
    pub fn client_observed(&self, client_key: ClientKey, event: TrackedClientEvent) -> bool {
        self.scenario_ref().client_observed(client_key, event)
    }

    /// Check client event ordering.
    pub fn client_event_before(&self, client_key: ClientKey, a: TrackedClientEvent, b: TrackedClientEvent) -> bool {
        self.scenario_ref().client_event_before(client_key, a, b)
    }

    /// Check if client is connected.
    pub fn client_is_connected(&self, client_key: ClientKey) -> bool {
        self.scenario_ref().client_is_connected(client_key)
    }

    /// Get client event history (cloned).
    pub fn client_event_history(&self, client_key: ClientKey) -> Vec<TrackedClientEvent> {
        self.scenario_ref().client_event_history(client_key).to_vec()
    }

    /// Get server event history (cloned).
    pub fn server_event_history(&self) -> Vec<TrackedServerEvent> {
        self.scenario_ref().server_event_history().to_vec()
    }

    /// Get access to scenario for custom queries (if needed).
    pub fn scenario(&self) -> &Scenario {
        self.scenario_ref()
    }
}

impl StepContext for TestWorldRef<'_> {
    type World = TestWorld;
}
