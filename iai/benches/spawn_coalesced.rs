// iai-callgrind instruction-count benchmark for SpawnWithComponents coalescing (Win-4).
// Run with:  cargo bench -p naia-iai --bench spawn_coalesced
// Requires:  valgrind installed (sudo apt install valgrind)
//
// Win-4 invariant: spawning an entity with its components in one message
// is cheaper than spawning + later inserting components. In this build
// the coalesced path is the ONLY path — every `spawn_entity` + insert
// before the first send coalesces into one `SpawnWithComponents`. So the
// bench here measures the steady-state tick after N coalesced spawns,
// and the counter must scale linearly in N (one wire message per entity).
// A super-linear jump would indicate coalescing has broken and a per-entity
// second pass is emitting extra insert messages.

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
