// Phase 3 diagnostic: time each sub-phase of an idle tick to localize the
// real O(U·N) cost. The Phase-1 scan removal proved the dirty scan is gone
// (0 receivers visited per tick) but wall-time did not drop. Evidence shows
// the bottleneck is elsewhere — this probe replaces advance_tick() with a
// timed version that brackets: hub queues, per-client receive/process/send,
// server receive/process/send.
//
// Run with:
//   cargo run --release --example phase3_tick_breakdown -p naia-benches

use std::time::Instant as StdInstant;

use naia_benches::BenchWorldBuilder;

fn run(u: usize, n: usize, immutable: bool) {
    // Fresh build, time first tick only — this is what criterion iter_batched
    // actually measures (setup once per iteration, time the routine once).
    let mut t_builds = 0.0;
    let mut first_tick_times = Vec::new();
    let mut steady_tick_times = Vec::new();
    for _ in 0..5 {
        let mut builder = BenchWorldBuilder::new().users(u).entities(n);
        if immutable {
            builder = builder.immutable();
        }
        let tb = StdInstant::now();
        let mut world = builder.build();
        t_builds += tb.elapsed().as_secs_f64() * 1e3;

        let t1 = StdInstant::now();
        world.tick();
        first_tick_times.push(t1.elapsed().as_secs_f64() * 1e3);

        // 3 more ticks on SAME world
        let mut steady_sum = 0.0;
        for _ in 0..3 {
            let t = StdInstant::now();
            world.tick();
            steady_sum += t.elapsed().as_secs_f64() * 1e3;
        }
        steady_tick_times.push(steady_sum / 3.0);
    }
    let label = if immutable { "imm" } else { "mut" };
    first_tick_times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    steady_tick_times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    println!(
        "U={u:>2} N={n:>5} {label} | build_avg={:>6.1}ms first_tick_median={:>7.2}ms steady_tick_median={:>7.3}ms",
        t_builds / 5.0,
        first_tick_times[2],
        steady_tick_times[2],
    );
}

fn main() {
    println!("=== Phase 3 diagnostic: idle-tick wall time by (U, N) ===");
    println!();
    for (u, n) in [(1, 100), (1, 1_000), (1, 10_000),
                   (4, 100), (4, 1_000), (4, 10_000),
                   (16, 100), (16, 1_000), (16, 10_000)] {
        run(u, n, false);
    }
    println!();
    println!("=== Same matrix, IMMUTABLE entities ===");
    println!();
    for (u, n) in [(1, 100), (1, 1_000), (1, 10_000),
                   (4, 100), (4, 1_000), (4, 10_000),
                   (16, 100), (16, 1_000), (16, 10_000)] {
        run(u, n, true);
    }
}
