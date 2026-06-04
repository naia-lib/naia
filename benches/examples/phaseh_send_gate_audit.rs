//! Phase-H deferred-followup: measure the send-side "correctness machinery".
//!
//! Two questions the mission doc deferred (MISSION_THREE_TIMELINE_DESYNC §"Deferred
//! follow-up"), now answered with measured numbers instead of a guess:
//!
//!   (b) What does CLOSING the fast-path bypass cost? The Phase-3A send gate uses
//!       a single-lookup `is_dirty_and_delivered` fast path; "hardening" the gate
//!       into a strict barrier means every visited component pays the 6+-HashMap
//!       `is_component_updatable_for_entity` chain. We A/B `srv_tx` (the server
//!       send phase — the ONLY place the gate runs) on the 16p halo active tick,
//!       fast-path vs forced-slow.
//!
//!   (a) How much bandwidth do REDUNDANT (pre-delivery) updates cost? A redundant
//!       update = one emitted for a component whose insert the receiver has not
//!       yet confirmed (the receiver-side waitlist must then buffer it). We turn
//!       on leak measurement and (1) confirm the steady-state leak is zero, then
//!       (2) open a fresh spawn->ACK window (a wave of units entering scope while
//!       moving) and watch what the gate emits vs suppresses.
//!
//! Run:
//!   cargo run --release --example phaseh_send_gate_audit -p naia-benches
//!
//! (`naia-benches` enables `bench_instrumentation` on naia-server, which compiles
//! the gate counters + the FORCE_SLOW_GATE / MEASURE_LEAK toggles.)

use naia_benches::BenchWorldBuilder;
use naia_server::bench_iris_counters as gate;

const PLAYERS: usize = 16;
const TILE_COUNT: usize = 10_000;
const UNIT_COUNT: usize = 32;
const TICK_HZ: u16 = 25;

fn median(v: &mut [u128]) -> u128 {
    v.sort_unstable();
    v[v.len() / 2]
}

fn mean(v: &[u128]) -> f64 {
    v.iter().sum::<u128>() as f64 / v.len() as f64
}

fn new_world() -> naia_benches::BenchWorld {
    let mut w = BenchWorldBuilder::new()
        .users(PLAYERS)
        .tick_rate_hz(TICK_HZ)
        .uncapped_bandwidth()
        .build();
    w.spawn_halo_scene(TILE_COUNT, 0); // tiles only — units spawned per-phase
    w
}

fn main() {
    println!("=== Phase-H send-gate audit: 16 players, 10K tiles, {UNIT_COUNT} units, 25 Hz ===\n");

    // ─────────────────────────────────────────────────────────────────────────
    // (b) CPU cost of closing the fast-path bypass.
    //     Measure srv_tx only (the send phase). Leak measurement OFF so the extra
    //     is_component_updatable cross-check never skews timings.
    // ─────────────────────────────────────────────────────────────────────────
    {
        let mut w = new_world();
        // Units live at the TAIL of server_entities (tiles occupy 0..TILE_COUNT);
        // mutate that range, not the first N (which are tiles → no-ops).
        let start = w.spawn_halo_units_no_catchup(UNIT_COUNT);
        // Deliver the 32 units, then reach steady state (all on the fast path).
        for _ in 0..40 {
            w.mutate_halo_units_range(start, UNIT_COUNT);
            w.tick();
        }
        gate::set_measure_leak(false);

        const BLOCK: usize = 50;
        const ROUNDS: usize = 40;
        let mut fast = Vec::with_capacity(BLOCK * ROUNDS);
        let mut slow = Vec::with_capacity(BLOCK * ROUNDS);
        // Warmup both arms.
        for _ in 0..5 {
            w.mutate_halo_units_range(start, UNIT_COUNT);
            let _ = w.tick_timed();
        }
        for _ in 0..ROUNDS {
            gate::set_force_slow_gate(false);
            for _ in 0..BLOCK {
                w.mutate_halo_units_range(start, UNIT_COUNT);
                fast.push(w.tick_timed().srv_tx.as_nanos());
            }
            gate::set_force_slow_gate(true);
            for _ in 0..BLOCK {
                w.mutate_halo_units_range(start, UNIT_COUNT);
                slow.push(w.tick_timed().srv_tx.as_nanos());
            }
        }
        gate::set_force_slow_gate(false);

        let fast_med = median(&mut fast);
        let slow_med = median(&mut slow);
        let visits_per_tick = (UNIT_COUNT * PLAYERS) as f64; // dirty component-visits / tick
        println!("(b) CLOSING THE FAST-PATH BYPASS — server send phase (srv_tx), {} samples/arm:", fast.len());
        println!("    fast-path (current)  : median {:>7.2} µs   mean {:>7.2} µs", fast_med as f64 / 1e3, mean(&fast) / 1e3);
        println!("    forced-slow (hardened): median {:>7.2} µs   mean {:>7.2} µs", slow_med as f64 / 1e3, mean(&slow) / 1e3);
        let delta = slow_med as f64 - fast_med as f64;
        println!(
            "    Δ per tick           : {:>+7.2} µs  ({:+.1}%)   ≈ {:.1} ns / dirty-component-visit (×{} visits)",
            delta / 1e3,
            100.0 * delta / fast_med as f64,
            delta / visits_per_tick,
            visits_per_tick as u64,
        );
        println!();
    }

    // ─────────────────────────────────────────────────────────────────────────
    // (a1) Steady-state leak: with everything delivered, does the fast path ever
    //      emit a pre-delivery update? (Structural trace says no — confirm it.)
    // ─────────────────────────────────────────────────────────────────────────
    {
        let mut w = new_world();
        let start = w.spawn_halo_units_no_catchup(UNIT_COUNT);
        for _ in 0..40 {
            w.mutate_halo_units_range(start, UNIT_COUNT);
            w.tick();
        }
        gate::set_force_slow_gate(false);
        gate::set_measure_leak(true);
        gate::reset();
        let mut bytes = Vec::new();
        for _ in 0..100 {
            w.mutate_halo_units_range(start, UNIT_COUNT);
            w.tick();
            bytes.push(w.server_outgoing_bytes_per_tick() as u128);
        }
        let (fast_emit, slow_emit, leak, suppressed) = gate::snapshot_gate();
        println!("(a1) STEADY STATE — 100 active ticks, all units delivered:");
        println!("     fast-path emits : {fast_emit}");
        println!("     slow-path emits : {slow_emit}");
        println!("     PRE-DELIVERY LEAKS (redundant updates the receiver must buffer): {leak}");
        println!("     gate-suppressed : {suppressed}");
        println!("     server outgoing : median {} B/tick", median(&mut bytes));
        println!();
    }

    // ─────────────────────────────────────────────────────────────────────────
    // (a2) Open spawn->ACK window: a wave of 32 units enters scope while moving.
    //      Mutate them every tick from the moment they spawn and watch the gate.
    //      Expectation per the trace: pre-delivery mutations are SUPPRESSED (not
    //      leaked) until the SpawnWithComponents ACK arrives, then they flip to
    //      the fast path. Leak must stay 0 — the receiver waitlist exists only
    //      for transport reordering, which no send gate can prevent.
    // ─────────────────────────────────────────────────────────────────────────
    {
        let mut w = new_world();
        let start = w.spawn_halo_units_no_catchup(UNIT_COUNT);
        gate::set_force_slow_gate(false);
        gate::set_measure_leak(true);
        println!("(a2) SCOPE-ENTRY WINDOW — 32 units spawned, mutated every tick while replicating:");
        println!("     tick |  fast | slow | LEAK | suppressed | out B");
        let mut total_suppressed = 0u64;
        let mut total_leak = 0u64;
        for t in 0..12 {
            gate::reset();
            w.mutate_halo_units_range(start, UNIT_COUNT);
            w.tick();
            let (fast_emit, slow_emit, leak, suppressed) = gate::snapshot_gate();
            let out = w.server_outgoing_bytes_per_tick();
            total_suppressed += suppressed;
            total_leak += leak;
            println!(
                "     {:>4} | {:>5} | {:>4} | {:>4} | {:>10} | {:>5}",
                t, fast_emit, slow_emit, leak, suppressed, out
            );
        }
        println!("     ── window totals: suppressed (bandwidth the gate SAVED) = {total_suppressed}, LEAK = {total_leak} ──");
        println!();
    }

    println!("Done.");
}
