// iai-callgrind instruction-count benchmark for the dirty-receiver push model (Win-3).
// Run with:  cargo bench -p naia-iai --bench dirty_receiver
// Requires:  valgrind installed (sudo apt install valgrind)
//
// Win-3 invariant: mutation dispatch cost is O(K mutations × U users), NOT
// O(N entities × U users). Holding K=1 constant, instruction count should
// stay roughly flat as N grows from 10 → 1000. A super-linear jump here
// indicates the dirty-receiver push model has regressed to an entity scan.

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

// K=1 mutation, N=10. Baseline push-model cost.
#[library_benchmark]
#[bench::b(setup_1u_10e())]
fn dirty_1_in_10e(mut world: naia_benches::BenchWorld) {
    world.mutate_entities(1);
    world.tick();
}

// K=1 mutation, N=100. Mid-scale check.
#[library_benchmark]
#[bench::b(setup_1u_100e())]
fn dirty_1_in_100e(mut world: naia_benches::BenchWorld) {
    world.mutate_entities(1);
    world.tick();
}

// K=1 mutation, N=1000.
// PRIMARY GATE: instruction count must be flat vs. N=10 variant.
// Flat count proves Win-3 push model is genuinely O(mutations), not O(N).
#[library_benchmark]
#[bench::b(setup_1u_1ke())]
fn dirty_1_in_1ke(mut world: naia_benches::BenchWorld) {
    world.mutate_entities(1);
    world.tick();
}

library_benchmark_group!(
    name = dirty_receiver_group;
    benchmarks = dirty_1_in_10e, dirty_1_in_100e, dirty_1_in_1ke
);

main!(
    config = LibraryBenchmarkConfig::default();
    library_benchmark_groups = dirty_receiver_group
);
