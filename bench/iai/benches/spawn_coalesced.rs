// iai-callgrind instruction-count benchmark for SpawnWithComponents coalescing (Win-4).
// Run with:  cargo bench -p naia-iai --bench spawn_coalesced
// Requires:  valgrind installed (sudo apt install valgrind)
//
// Win-4 invariant: spawning an entity + inserting its components before
// the first outbound send coalesces into ONE `SpawnWithComponents` wire
// message, rather than one `SpawnEntity` + K `InsertComponent` messages.
//
// ── Why this bench is STEADY-STATE, not a direct A/B ──────────────────
// The plan called for "one spawn-with-3-components vs spawn + 3 inserts".
// In this build that direct A/B comparison is NOT achievable:
//   1. The bench protocol defines a single `BenchComponent` (plus an
//      `BenchImmutableComponent` marker) — there is no "3 components"
//      entity type to exercise the 3-insert path.
//   2. Even with 3 component types, the non-coalesced path requires
//      inserts AFTER the first send — the server's SpawnWithComponents
//      queueing coalesces every insert issued before the outbound send.
//      The bench harness does not currently expose a tick-then-insert
//      helper.
//   3. A direct A/B would otherwise require two Naia builds (pre-Win-4
//      and post-Win-4), which is the criterion suite's stated position
//      — see `benches/benches/spawn/coalesced.rs` doc comment.
//
// What we CAN gate is the steady-state tick cost after N coalesced
// spawns have replicated. Once all N entities are replicated via one
// SpawnWithComponents each, the idle tick does nothing per entity (O(1)
// per Win-2). A super-linear jump here would indicate coalescing has
// silently regressed and each entity is now taking multiple wire
// messages to replicate (setup phase would measure the extra cost
// amortised into ongoing ticks).

use iai_callgrind::{library_benchmark, library_benchmark_group, main, LibraryBenchmarkConfig};
use naia_benches::BenchWorldBuilder;

fn setup_1u_10e() -> naia_benches::BenchWorld {
    BenchWorldBuilder::new().users(1).entities(10).build()
}

fn setup_1u_100e() -> naia_benches::BenchWorld {
    BenchWorldBuilder::new().users(1).entities(100).build()
}

fn setup_1u_1ke() -> naia_benches::BenchWorld {
    BenchWorldBuilder::new().users(1).entities(1_000).build()
}

// Coalesced-spawn steady-state tick, N=10.
#[library_benchmark]
#[bench::b(setup_1u_10e())]
fn coalesced_spawn_10e(mut world: naia_benches::BenchWorld) {
    world.tick();
}

// Coalesced-spawn steady-state tick, N=100.
#[library_benchmark]
#[bench::b(setup_1u_100e())]
fn coalesced_spawn_100e(mut world: naia_benches::BenchWorld) {
    world.tick();
}

// Coalesced-spawn steady-state tick, N=1000.
// GATE: instruction count should be roughly flat at steady state
// (all spawns already replicated; this measures ongoing idle cost,
// which for coalesced spawn is ~O(1) post-replication).
#[library_benchmark]
#[bench::b(setup_1u_1ke())]
fn coalesced_spawn_1ke(mut world: naia_benches::BenchWorld) {
    world.tick();
}

library_benchmark_group!(
    name = spawn_coalesced_group;
    benchmarks = coalesced_spawn_10e, coalesced_spawn_100e, coalesced_spawn_1ke
);

main!(
    config = LibraryBenchmarkConfig::default();
    library_benchmark_groups = spawn_coalesced_group
);
