// Canonical spike-detection harness for idle ticks.
//
// Criterion's `time: [lo mean hi]` is a confidence interval on the MEAN and
// hides tail behavior. A single 1.4 s tick in 2000 otherwise-50 µs ticks drags
// the mean to ~750 µs — invisible in the criterion line. Phase 4 hit exactly
// that trap: the probe median said 40 µs/tick, but criterion showed 40 ms/tick
// because it caught the tail. This harness always reports distribution.
//
// For each (U, N, kind) cell, we:
//   1. Build the world once (criterion-idiom: build outside timed region).
//   2. Warm up `WARMUP` ticks to pass the one-time initial-sync burst.
//   3. Measure `SAMPLES` ticks with `Instant::now()` around each `tick()`.
//   4. Report p50/p90/p99/max and the first 5 spikes above p99 × 10.
//
// Run with:
//   cargo run --release --example idle_distribution -p naia-benches

use std::time::Instant;

use naia_benches::BenchWorldBuilder;

const MATRIX_USERS: &[usize] = &[1, 4, 16];
const MATRIX_ENTITIES: &[usize] = &[100, 1_000, 10_000];
const WARMUP: usize = 100;
const SAMPLES: usize = 2_000;
const SPIKE_FACTOR: f64 = 10.0;

fn percentile(sorted: &[u64], p: f64) -> u64 {
    if sorted.is_empty() { return 0; }
    let idx = ((sorted.len() as f64 - 1.0) * p).round() as usize;
    sorted[idx]
}

struct CellReport {
    label: String,
    p50: u64,
    p90: u64,
    p99: u64,
    max: u64,
    mean: u64,
    spikes: Vec<(usize, u64)>,
}

fn measure_cell(u: usize, n: usize, immutable: bool) -> CellReport {
    let mut builder = BenchWorldBuilder::new().users(u).entities(n);
    if immutable {
        builder = builder.immutable();
    }
    let mut world = builder.build();

    for _ in 0..WARMUP {
        world.tick();
    }

    let mut times: Vec<u64> = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        let t = Instant::now();
        world.tick();
        times.push(t.elapsed().as_nanos() as u64);
    }

    let mut sorted = times.clone();
    sorted.sort_unstable();
    let p50 = percentile(&sorted, 0.50);
    let p90 = percentile(&sorted, 0.90);
    let p99 = percentile(&sorted, 0.99);
    let max = *sorted.last().unwrap();
    let mean: u64 = (times.iter().map(|&x| x as u128).sum::<u128>() / times.len() as u128) as u64;

    let spike_threshold = ((p99 as f64) * SPIKE_FACTOR) as u64;
    let spikes: Vec<(usize, u64)> = times.iter().enumerate()
        .filter(|(_, &ns)| ns > spike_threshold)
        .map(|(i, &ns)| (i, ns))
        .collect();

    let kind = if immutable { "imm" } else { "mut" };
    let label = format!("{u:>2}u_{n:>5}e_{kind}");
    CellReport { label, p50, p90, p99, max, mean, spikes }
}

fn print_report(r: &CellReport) {
    let ratio = r.max as f64 / r.p50 as f64;
    let flag = if r.spikes.is_empty() { " OK " } else { "SPIKE" };
    println!(
        "  [{flag}] {}  p50={:>8.1}µs  p90={:>8.1}µs  p99={:>8.1}µs  max={:>10.1}µs  mean={:>8.1}µs  max/p50={ratio:>6.1}×",
        r.label,
        r.p50 as f64 / 1_000.0,
        r.p90 as f64 / 1_000.0,
        r.p99 as f64 / 1_000.0,
        r.max as f64 / 1_000.0,
        r.mean as f64 / 1_000.0,
    );
    if !r.spikes.is_empty() {
        let preview: Vec<String> = r.spikes.iter().take(5)
            .map(|(i, ns)| format!("tick+{i} = {:.2}ms", *ns as f64 / 1e6))
            .collect();
        let rest = if r.spikes.len() > 5 {
            format!("  (+{} more)", r.spikes.len() - 5)
        } else {
            String::new()
        };
        println!("         spikes[{}]: {}{rest}", r.spikes.len(), preview.join(", "));
    }
}

fn main() {
    println!("=== Idle-tick distribution (warmup={WARMUP}, samples={SAMPLES}) ===");
    println!("Spike = tick > p99 × {SPIKE_FACTOR}. Flags 'SPIKE' if any.");
    println!();

    println!("--- MUTABLE ---");
    for &u in MATRIX_USERS {
        for &n in MATRIX_ENTITIES {
            print_report(&measure_cell(u, n, false));
        }
    }
    println!();
    println!("--- IMMUTABLE ---");
    for &u in MATRIX_USERS {
        for &n in MATRIX_ENTITIES {
            print_report(&measure_cell(u, n, true));
        }
    }
}
