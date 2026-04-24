// Phase 1 diagnostic: prove dirty_receiver_candidates scans O(U·N) per tick.
//
// Run with:
//   cargo run --release --example phase1_scan_counters -p naia-benches
//
// What it does: builds a BenchWorld at several (U, N) sizes, runs ONE idle
// tick each, and reads the `dirty_scan_counters` snapshot. If the scan is
// truly O(U·N), `receivers_visited` should equal U × N for that tick, and
// `dirty_results` should equal 0 on idle ticks.
//
// This is the flamegraph-equivalent artifact for the perf-upgrade project —
// produces numerical evidence of the hot-path cost without needing valgrind
// or perf_event_paranoid access.

use naia_benches::BenchWorldBuilder;
use naia_shared::dirty_scan_counters;

fn measure(u: usize, n: usize) {
    let mut world = BenchWorldBuilder::new().users(u).entities(n).build();
    dirty_scan_counters::reset();
    world.tick();
    let (calls, visited, dirty) = dirty_scan_counters::snapshot();
    println!(
        "U={u:>2} N={n:>5} | scan_calls={calls:>4} receivers_visited={visited:>9} dirty_results={dirty:>4} | ratio visited/(U·N)={:.2}",
        visited as f64 / (u * n) as f64
    );
}

fn main() {
    println!("=== Phase 1: dirty_receiver_candidates scan counters (idle tick) ===");
    println!();
    for (u, n) in [(1, 100), (1, 1_000), (1, 10_000),
                   (4, 100), (4, 1_000), (4, 10_000),
                   (16, 100), (16, 1_000), (16, 10_000)] {
        measure(u, n);
    }
    println!();
    println!("Interpretation: ratio=1.0 proves receivers are scanned exhaustively");
    println!("once per user per tick. Post-Phase-3 target: ratio ≈ 0 for idle ticks.");
}
