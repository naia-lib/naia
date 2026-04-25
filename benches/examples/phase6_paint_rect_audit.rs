//! Phase 6 — coalesce audit
//!
//! Question: when N entities are spawned with K components each in a single
//! server tick (the canonical "PaintRect of N tiles" pattern), does the wire
//! carry **N** `SpawnWithComponents` commands (one per entity, K kinds inline),
//! or **N + N×K** ops (`Spawn` + `InsertComponent` × K) per entity?
//!
//! This is hypothesis (a) vs (b) in `_AGENTS/BENCH_PERF_UPGRADE.md` Phase 6:
//!
//! - **(a) Coalescing is fine** — `init_entity_send_host_commands` builds one
//!   `SpawnWithComponents(entity, vec![all_kinds])` per scope-entry; the wire
//!   carries N spawn commands and 0 insert-component commands. The criterion
//!   `spawn/coalesced` bench measures *steady-state idle*, not the coalesce
//!   itself, which is why it looked only ~10% better than `spawn/burst`.
//!
//! - **(b) Coalescing is silently missing** — somewhere in the spawn-burst
//!   path the per-component messages slip through, and the wire actually
//!   carries N + N×K ops, with the bandwidth and latency cost that implies.
//!
//! The audit gate (asserted at the bottom of this example):
//!
//!     spawn_with_components_count == N
//!     spawn_count                 == 0
//!     insert_component_count      == 0
//!     payload_components          == N × K
//!
//! If **(a)** holds the gate passes and we record the firm validation in
//! `phase-06.md`. If **(b)** holds the gate fails and we have a concrete
//! before/after target for the coalesce fix.
//!
//! Run with:
//!     cargo run --release --example phase6_paint_rect_audit -p naia-benches

use std::sync::atomic::Ordering;

use naia_benches::BenchWorldBuilder;
use naia_shared::cmd_emission_counters;

/// Run the audit for one (N, K) cell. Returns the snapshot for printing.
fn run_cell(n: usize, k: usize) -> cmd_emission_counters::CmdEmissionSnapshot {
    // Setup: 1 user, 0 entities, scoped (so the room exists with the user in it).
    let mut world = BenchWorldBuilder::new().users(1).entities(0).build();

    // Reset counters AFTER setup (which itself does spawns/connect) so we
    // measure only the burst.
    cmd_emission_counters::reset();

    // PaintRect: spawn N entities with K components each, in one tick.
    world.paint_rect_spawn_burst(n, k);

    // Drive ticks until all N entities have replicated to the client. The
    // first send carries the N spawn commands; subsequent ticks just
    // ack/heartbeat. We need a few ticks because the replication budget per
    // tick is bounded by the bandwidth accumulator now (sidequest Phase A).
    let target = n;
    for _ in 0..2_000 {
        world.tick();
        if world.client_entity_count() >= target {
            break;
        }
    }

    cmd_emission_counters::snapshot()
}

fn main() {
    println!("Phase 6 — PaintRect coalescing audit");
    println!("─────────────────────────────────────");
    println!(
        "{:>5} {:>3} | {:>10} {:>10} {:>8} {:>10} {:>10} {:>8} | {:>10} | {:>10}",
        "N", "K", "spawn_wc", "spawn", "despawn", "insert_c", "remove_c", "noop", "payload", "verdict"
    );
    println!(
        "──────────┼─────────────────────────────────────────────────────────────────┼────────────┼─────────"
    );

    let cells: &[(usize, usize)] = &[
        (1, 1),
        (1, 2),
        (10, 1),
        (10, 2),
        (100, 1),
        (100, 2),
        (256, 2),  // canonical PaintRect of 16×16 tile-rect, 2 components
        (1_000, 2),
    ];

    let mut all_pass = true;
    for &(n, k) in cells {
        let s = run_cell(n, k);
        let pass = s.spawn_with_components == n as u64
            && s.spawn == 0
            && s.insert_component == 0
            && s.payload_components == (n * k) as u64;
        if !pass {
            all_pass = false;
        }
        println!(
            "{:>5} {:>3} | {:>10} {:>10} {:>8} {:>10} {:>10} {:>8} | {:>10} | {}",
            n,
            k,
            s.spawn_with_components,
            s.spawn,
            s.despawn,
            s.insert_component,
            s.remove_component,
            s.noop,
            s.payload_components,
            if pass { "OK" } else { "FAIL" }
        );
    }

    println!();
    if all_pass {
        println!("RESULT: hypothesis (a) holds — PaintRect coalescing is correct.");
        println!("        Each spawn-burst entity emits exactly one SpawnWithComponents");
        println!("        carrying all components inline. Zero stray Spawn / InsertComponent");
        println!("        ops. The criterion `spawn/coalesced` bench's apparent flatness");
        println!("        reflects that it measures steady-state idle cost, not the burst.");
        std::process::exit(0);
    } else {
        println!("RESULT: hypothesis (b) — at least one cell failed the coalesce gate.");
        println!("        See phase-06.md for the trace.");
        // Don't fail-exit — the readout itself is the deliverable; the run-log
        // is what gets pasted into phase-06.md.
        std::process::exit(0);
    }
}

// Sanity guard: keep cmd_emission_counters reachable.
#[allow(dead_code)]
fn _drain(_: std::sync::atomic::AtomicU64) -> u64 {
    cmd_emission_counters::SPAWN_WITH_COMPONENTS.load(Ordering::Relaxed)
}
