//! G-W4 Phase 2 — Compression audit
//!
//! Captures real server-to-client packet bytes from a halo_btb_16v16-like
//! scenario, buckets them by size, and measures zstd compression ratios at
//! levels -7/1/3 and with a game-traffic-trained dictionary.
//!
//! Gate: if dictionary mode shows ≥15% bandwidth reduction on the spawn-burst
//! bucket (150–430 bytes), recommend enabling dictionary compression.
//!
//! Run with:
//!     cargo run --release --example compression_audit -p naia-benches

use naia_benches::BenchWorldBuilder;

const SPAWN_TILES: usize = 256;
const SPAWN_UNITS: usize = 16;
const STEADY_STATE_TICKS: usize = 300;

fn main() {
    // ── Phase 1: capture spawn-burst packets ─────────────────────────────────

    let mut world = BenchWorldBuilder::new()
        .users(1)
        .entities(0)
        .uncapped_bandwidth()
        .build();

    // Enable recording AFTER setup; captures only spawn-burst + steady-state.
    world.hub().enable_packet_recording();

    // Spawn tiles + units and drive until all entities replicated.
    world.spawn_halo_scene(SPAWN_TILES, SPAWN_UNITS);

    let burst_raw = world.hub().take_recorded_packets();
    let spawn_burst: Vec<Vec<u8>> = burst_raw
        .into_iter()
        .filter(|(s2c, _)| *s2c)
        .map(|(_, b)| b)
        .collect();

    // ── Phase 2: capture steady-state mutation packets ────────────────────────

    for _ in 0..STEADY_STATE_TICKS {
        world.mutate_halo_units(SPAWN_UNITS);
        world.tick();
    }

    let steady_raw = world.hub().take_recorded_packets();
    let steady_state: Vec<Vec<u8>> = steady_raw
        .into_iter()
        .filter(|(s2c, _)| *s2c)
        .map(|(_, b)| b)
        .collect();

    // ── Analysis ──────────────────────────────────────────────────────────────

    println!("G-W4 Phase 2 — Compression audit");
    println!("Scenario: {} tiles + {} units, {} steady-state ticks",
             SPAWN_TILES, SPAWN_UNITS, STEADY_STATE_TICKS);
    println!();

    let all_packets: Vec<Vec<u8>> = spawn_burst.iter().chain(steady_state.iter()).cloned().collect();

    for (label, packets) in &[
        ("Spawn-burst", &spawn_burst),
        ("Steady-state", &steady_state),
        ("Combined", &all_packets),
    ] {
        println!("── {} ({} packets) ─────────────────────────────", label, packets.len());
        if packets.is_empty() {
            println!("  (no packets)");
            continue;
        }

        print_bucket_report(label, packets);
        println!();
    }

    // ── Gate evaluation ───────────────────────────────────────────────────────

    let large: Vec<Vec<u8>> = spawn_burst.iter()
        .filter(|p| p.len() > 150)
        .cloned()
        .collect();

    if large.is_empty() {
        println!("GATE: no large (150-430B) spawn-burst packets observed — cannot evaluate gate.");
        println!("      (Spawn burst may have been chunked into smaller packets.)");
        println!("      Evaluating gate on ALL spawn-burst packets instead.");
        let ratio = dict_compression_ratio(&spawn_burst);
        let reduction = 1.0 - ratio;
        println!("GATE: dictionary compression on spawn-burst = {:.1}% reduction", reduction * 100.0);
        if reduction >= 0.15 {
            println!("GATE PASS: ≥15% reduction — recommend enabling dictionary compression.");
            std::process::exit(0);
        } else {
            println!("GATE FAIL: <15% reduction — dictionary compression not worth the overhead.");
            std::process::exit(0);
        }
    }

    let ratio = dict_compression_ratio(&large);
    let reduction = 1.0 - ratio;
    println!("GATE: dictionary compression on large spawn-burst (>150B) = {:.1}% reduction", reduction * 100.0);
    if reduction >= 0.15 {
        println!("GATE PASS: ≥15% reduction — ship dictionary compression as recommended default.");
    } else {
        println!("GATE FAIL: <15% reduction — close the gap permanently with this data.");
    }
    std::process::exit(0);
}

// ── per-bucket report ─────────────────────────────────────────────────────────

fn print_bucket_report(phase: &str, packets: &[Vec<u8>]) {
    let small: Vec<&Vec<u8>>  = packets.iter().filter(|p| p.len() <= 50).collect();
    let medium: Vec<&Vec<u8>> = packets.iter().filter(|p| p.len() > 50 && p.len() <= 150).collect();
    let large: Vec<&Vec<u8>>  = packets.iter().filter(|p| p.len() > 150).collect();

    let _ = phase;

    println!("  {:20} {:>6} {:>8} {:>8} {:>8} {:>8} {:>8}",
             "bucket", "count", "raw(B)", "zstd-7", "zstd-1", "zstd-3", "dict-3");
    println!("  {}", "─".repeat(72));

    for (name, bucket) in &[
        ("small (0-50B)",   &small),
        ("medium (50-150B)", &medium),
        ("large (150-430B)", &large),
    ] {
        let owned: Vec<Vec<u8>> = bucket.iter().map(|p| (*p).clone()).collect();
        if owned.is_empty() {
            println!("  {:20} {:>6}  (no data)", name, 0);
            continue;
        }
        let total_raw: usize = owned.iter().map(|p| p.len()).sum();
        let r_neg7 = compression_ratio(&owned, -7);
        let r1     = compression_ratio(&owned, 1);
        let r3     = compression_ratio(&owned, 3);
        let rd     = dict_compression_ratio(&owned);
        println!(
            "  {:20} {:>6} {:>8} {:>7.1}% {:>7.1}% {:>7.1}% {:>7.1}%",
            name,
            owned.len(),
            total_raw,
            (1.0 - r_neg7) * 100.0,
            (1.0 - r1)     * 100.0,
            (1.0 - r3)     * 100.0,
            (1.0 - rd)     * 100.0,
        );
    }

    let total_raw: usize = packets.iter().map(|p| p.len()).sum();
    let all_owned: Vec<Vec<u8>> = packets.to_vec();
    let r_neg7 = compression_ratio(&all_owned, -7);
    let r1     = compression_ratio(&all_owned, 1);
    let r3     = compression_ratio(&all_owned, 3);
    let rd     = dict_compression_ratio(&all_owned);
    println!("  {}", "─".repeat(72));
    println!(
        "  {:20} {:>6} {:>8} {:>7.1}% {:>7.1}% {:>7.1}% {:>7.1}%",
        "TOTAL",
        packets.len(),
        total_raw,
        (1.0 - r_neg7) * 100.0,
        (1.0 - r1)     * 100.0,
        (1.0 - r3)     * 100.0,
        (1.0 - rd)     * 100.0,
    );
}

// ── compression helpers ───────────────────────────────────────────────────────

fn compression_ratio(packets: &[Vec<u8>], level: i32) -> f64 {
    if packets.is_empty() {
        return 1.0;
    }
    let orig: usize = packets.iter().map(|p| p.len()).sum();
    let comp: usize = packets
        .iter()
        .map(|p| {
            zstd::encode_all(p.as_slice(), level)
                .map(|c| c.len())
                .unwrap_or(p.len())
        })
        .sum();
    comp as f64 / orig as f64
}

fn dict_compression_ratio(packets: &[Vec<u8>]) -> f64 {
    if packets.len() < 5 {
        // Too few samples to train a meaningful dictionary — fall back to no-dict.
        return compression_ratio(packets, 3);
    }
    let orig: usize = packets.iter().map(|p| p.len()).sum();
    // Train a 16 KiB dictionary on the full sample set.
    let dict = match zstd::dict::from_samples(packets, 16_384) {
        Ok(d) => d,
        Err(_) => return compression_ratio(packets, 3),
    };
    let mut compressor = match zstd::bulk::Compressor::with_dictionary(3, &dict) {
        Ok(c) => c,
        Err(_) => return compression_ratio(packets, 3),
    };
    let mut comp_total = 0usize;
    for p in packets {
        let c = compressor.compress(p)
            .map(|c| c.len())
            .unwrap_or(p.len());
        comp_total += c;
    }
    comp_total as f64 / orig as f64
}
