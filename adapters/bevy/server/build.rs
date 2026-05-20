//! Centralizes the Recv/Send worker parked-vs-active choice behind a single
//! `workers_active` cfg so the ~half-dozen branch sites in `plugin_full.rs`
//! don't each repeat the predicate.
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
//! Existing builds that want parked workers reach it via `deterministic` (e.g.
//! `cyberlith_test_harness` enables it by default), so the determinism-critical
//! path is byte-for-byte unchanged.

fn main() {
    println!("cargo::rustc-check-cfg=cfg(workers_active)");

    let deterministic = std::env::var_os("CARGO_FEATURE_DETERMINISTIC").is_some();

    if !deterministic {
        println!("cargo::rustc-cfg=workers_active");
    }
}
