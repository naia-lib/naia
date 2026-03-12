use crate::harness::{ExpectCtx, Scenario};

/// Context for expectations with a custom tick timeout
/// Created by calling `scenario.until(ticks).expect(...)`
pub struct UntilCtx<'scenario> {
    scenario: &'scenario mut Scenario,
    max_ticks: usize,
}

impl<'scenario> UntilCtx<'scenario> {
    pub(crate) fn new(scenario: &'scenario mut Scenario, max_ticks: usize) -> Self {
        Self {
            scenario,
            max_ticks,
        }
    }

    /// Register expectations and wait until they all pass or timeout.
    ///
    /// The closure is called each tick and should return `Some(T)` when expectations are met.
    /// Ticks the simulation until the closure returns `Some(value)` or the maximum tick count is reached.
    pub fn expect<T>(self, f: impl FnMut(&mut ExpectCtx<'_>) -> Option<T>) -> T {
        self.scenario.expect_with_ticks_internal(self.max_ticks, f)
    }

    /// Register expectations with a custom message and wait until they all pass or timeout.
    ///
    /// The closure is called each tick and should return `Some(T)` when expectations are met.
    /// Ticks the simulation until the closure returns `Some(value)` or the maximum tick count is reached.
    pub fn expect_msg<T>(self, msg: &str, f: impl FnMut(&mut ExpectCtx<'_>) -> Option<T>) -> T {
        self.scenario
            .expect_with_ticks_internal_msg(self.max_ticks, msg, f)
    }

    /// Register a labeled expectation for spec obligation tracing with custom tick timeout.
    ///
    /// This is the primary API for assertions that verify spec contract obligations when you need
    /// to override the default tick timeout. Labels should follow the format:
    /// `<contract-id>.tN: <description>` for obligations, or `<contract-id>: <description>` for
    /// contract-level assertions.
    ///
    /// # Example
    ///
    /// ```ignore
    /// scenario.until(200.ticks()).spec_expect("messaging-15-a.t2: boundary tick is accepted", |ctx| {
    ///     ctx.client(key, |c| c.has_message()).then_some(())
    /// });
    /// ```
    pub fn spec_expect<T>(
        self,
        label: impl AsRef<str>,
        f: impl FnMut(&mut ExpectCtx<'_>) -> Option<T>,
    ) -> T {
        self.scenario
            .expect_with_ticks_internal_msg(self.max_ticks, label.as_ref(), f)
    }
}
