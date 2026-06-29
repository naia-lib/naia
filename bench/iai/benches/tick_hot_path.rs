// iai-callgrind instruction-count benchmark for the Naia idle-tick hot path.
// Run with:  cargo bench -p naia-iai --bench tick_hot_path
// Requires:  valgrind installed (sudo apt install valgrind)
//
// Primary CI regression gate: an increase in instruction count here
// indicates a hot-path regression in the tick loop.

use iai_callgrind::{library_benchmark, library_benchmark_group, main, LibraryBenchmarkConfig};
use naia_benches::BenchWorldBuilder;

fn setup_1u_100e() -> naia_benches::BenchWorld {
    BenchWorldBuilder::new().users(1).entities(100).build()
}

fn setup_1u_1ke() -> naia_benches::BenchWorld {
    BenchWorldBuilder::new().users(1).entities(1_000).build()
}

fn setup_1u_10ke() -> naia_benches::BenchWorld {
    BenchWorldBuilder::new().users(1).entities(10_000).build()
}

fn setup_4u_10ke() -> naia_benches::BenchWorld {
    BenchWorldBuilder::new().users(4).entities(10_000).build()
}

// Idle tick — 100 entities, 1 user. Establishes per-connection baseline.
#[library_benchmark]
#[bench::b(setup_1u_100e())]
fn idle_tick_100e(mut world: naia_benches::BenchWorld) {
    world.tick();
}

// Idle tick — 1K entities, 1 user. Mid-scale O(1) confirmation.
#[library_benchmark]
#[bench::b(setup_1u_1ke())]
fn idle_tick_1ke(mut world: naia_benches::BenchWorld) {
    world.tick();
}

// Idle tick — 10K entities, 1 user.
// PRIMARY GATE: instruction count must be flat vs. 100-entity variant.
// Flat count proves Win-2 + Win-3 are genuinely O(1) in entity count.
#[library_benchmark]
#[bench::b(setup_1u_10ke())]
fn idle_tick_10ke(mut world: naia_benches::BenchWorld) {
    world.tick();
}

// Idle tick — 10K entities, 4 users.
// Count should scale with user count, not entity count.
#[library_benchmark]
#[bench::b(setup_4u_10ke())]
fn idle_tick_10ke_4u(mut world: naia_benches::BenchWorld) {
    world.tick();
}

library_benchmark_group!(
    name = tick_hot_path_group;
    benchmarks = idle_tick_100e, idle_tick_1ke, idle_tick_10ke, idle_tick_10ke_4u
);

main!(
    config = LibraryBenchmarkConfig::default();
    library_benchmark_groups = tick_hot_path_group
);
