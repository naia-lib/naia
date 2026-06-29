// iai-callgrind instruction-count benchmark for the update-dispatch pipeline.
// Run with:  cargo bench -p naia-iai --bench update_dispatch
// Requires:  valgrind installed (sudo apt install valgrind)

use iai_callgrind::{library_benchmark, library_benchmark_group, main, LibraryBenchmarkConfig};
use naia_benches::BenchWorldBuilder;

fn setup_1_mutation() -> naia_benches::BenchWorld {
    BenchWorldBuilder::new().users(1).entities(1).build()
}

fn setup_100_mutations() -> naia_benches::BenchWorld {
    BenchWorldBuilder::new().users(1).entities(100).build()
}

// Single mutation: per-mutation baseline instruction cost.
#[library_benchmark]
#[bench::b(setup_1_mutation())]
fn dispatch_1_mutation(mut world: naia_benches::BenchWorld) {
    world.mutate_entities(1);
    world.tick();
}

// 100 mutations: confirms linear O(K) growth in instruction count.
#[library_benchmark]
#[bench::b(setup_100_mutations())]
fn dispatch_100_mutations(mut world: naia_benches::BenchWorld) {
    world.mutate_entities(100);
    world.tick();
}

library_benchmark_group!(
    name = update_dispatch_group;
    benchmarks = dispatch_1_mutation, dispatch_100_mutations
);

main!(
    config = LibraryBenchmarkConfig::default();
    library_benchmark_groups = update_dispatch_group
);
