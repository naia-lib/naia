//! Then-step bindings: observable state predicates.
//!
//! State assertions check the system's *current* observable state —
//! number of connected clients, which entities a client sees, what
//! authority status a client holds, etc. Distinct from
//! [`event_assertions`](super::event_assertions) which assert on
//! the *history* of emitted events.

use namako_engine::then;

use crate::TestWorldRef;

/// Then the server has {int} connected client(s).
#[then("the server has {int} connected client(s)")]
fn then_server_has_clients(ctx: &TestWorldRef, expected: usize) {
    let scenario = ctx.scenario();
    let count = scenario.server().expect("server").users_count();
    assert_eq!(
        count, expected,
        "server should have {} connected clients",
        expected
    );
}

/// Then the system intentionally fails.
///
/// Demo step from the P0-A runtime-failure scaffolding. Always
/// panics. Kept here for the namako-runtime smoke check.
#[then("the system intentionally fails")]
fn then_system_intentionally_fails(_ctx: &TestWorldRef) {
    panic!("INTENTIONAL FAILURE: This step is designed to fail for demo purposes");
}
