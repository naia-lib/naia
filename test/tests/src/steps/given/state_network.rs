//! Given-step bindings: network conditions (RTT / jitter / latency) preconditions.
//!
//! Split out of `given/state.rs` (Q3, 2026-05-07). See `state_*` siblings
//! and `world_helpers` for cross-cutting helpers.

use crate::steps::prelude::*;


// ──────────────────────────────────────────────────────────────────────
// Observability — RTT preconditions
// ──────────────────────────────────────────────────────────────────────

/// Given RTT has converged near {n}ms round-trip.
///
/// Spins enough ticks (~50) for the per-client RTT estimate to
/// stabilize. Used as a precondition for stable-RTT predicates.
#[given("RTT has converged near {int}ms round-trip")]
fn given_rtt_has_converged(ctx: &mut TestWorldMut, _expected_rtt_ms: u32) {
    let scenario = ctx.scenario_mut();
    for _ in 0..50 {
        scenario.mutate(|_| {});
    }
    scenario.allow_flexible_next();
}

/// Given the link has stable fixed-latency conditions.
///
/// Configures the link conditioner with 50ms latency, 2ms jitter,
/// 0% loss — the canonical "stable" baseline for RTT tests.
#[given("the link has stable fixed-latency conditions")]
fn given_link_stable_fixed_latency(ctx: &mut TestWorldMut) {
    use naia_test_harness::LinkConditionerConfig;
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    let stable = LinkConditionerConfig::new(50, 2, 0.0);
    scenario.configure_link_conditioner(&client_key, Some(stable.clone()), Some(stable));
}

/// Given the link has high jitter and moderate packet loss.
///
/// Configures the link conditioner with 100ms latency, 50ms jitter,
/// 10% loss — the canonical "adverse" baseline.
#[given("the link has high jitter and moderate packet loss")]
fn given_link_high_jitter_loss(ctx: &mut TestWorldMut) {
    use naia_test_harness::LinkConditionerConfig;
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    let adverse = LinkConditionerConfig::new(100, 50, 0.1);
    scenario.configure_link_conditioner(&client_key, Some(adverse.clone()), Some(adverse));
}

