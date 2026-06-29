//! MISSION_PIPELINE_API_BOUNDARY G7-3.
//!
//! Centralizes the Recv/Send worker parked-vs-active choice behind a single
//! `workers_active` cfg (consumed by `pipeline_actors/runtime.rs`) so the branch
//! sites don't each repeat the predicate.
//!
//! `workers_active = not(deterministic)`. The `test_time` feature (advanceable
//! clock) is orthogonal and does NOT influence this. Truth table:
//!
//! | test_time | deterministic | workers_active | meaning                          |
//! |-----------|---------------|----------------|----------------------------------|
//! | off       | off           | ON             | production: real clock, active   |
//! | on        | on            | off            | test suite: sim clock, parked    |
//! | on        | off           | ON             | bench: sim clock, ACTIVE workers |
//!
//! Builds that need byte-exact deterministic output reach parked workers via
//! `deterministic` (e.g. the bevy adapter forwards its own `deterministic`
//! feature to `naia-server/deterministic`), so the determinism-critical path is
//! byte-for-byte unchanged from the pre-move (adapter-owned) runtime.

fn main() {
    println!("cargo::rustc-check-cfg=cfg(workers_active)");

    let deterministic = std::env::var_os("CARGO_FEATURE_DETERMINISTIC").is_some();

    if !deterministic {
        println!("cargo::rustc-cfg=workers_active");
    }
}
