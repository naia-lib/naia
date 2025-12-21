use crate::harness::{ExpectCtx, Scenario};

/// Context for expectations with a custom tick timeout
/// Created by calling `scenario.until(ticks).expect(...)`
pub struct UntilCtx<'scenario> {
    scenario: &'scenario mut Scenario,
    max_ticks: usize,
}

impl<'scenario> UntilCtx<'scenario> {
    pub(crate) fn new(scenario: &'scenario mut Scenario, max_ticks: usize) -> Self {
        Self { scenario, max_ticks }
    }

    /// Register expectations and wait until they all pass or timeout.
    ///
    /// The closure is called each tick and should return `Some(T)` when expectations are met.
    /// Ticks the simulation until the closure returns `Some(value)` or the maximum tick count is reached.
    pub fn expect<T>(self, f: impl FnMut(&mut ExpectCtx<'_>) -> Option<T>) -> T {
        self.scenario.expect_with_ticks_internal(self.max_ticks, f)
    }
}
