// iai-callgrind instruction-count benchmark for the dirty-receiver push model (Win-3).
// Run with:  cargo bench -p naia-iai --bench dirty_receiver
// Requires:  valgrind installed (sudo apt install valgrind)
//
// Win-3 invariant: mutation dispatch cost is O(K mutations × U users), NOT
// O(N entities × U users). Holding K=1 constant, instruction count across
// scoped N=10/100/1000 should stay flat. The `unscoped_baseline` variant
// spawns entities but never adds them to a room — no dirty receivers are
// allocated, so it isolates framework overhead from dispatch overhead.
// A super-linear jump in the scoped series indicates the push model has
// regressed to a candidate-set scan.

use iai_callgrind::{library_benchmark, library_benchmark_group, main, LibraryBenchmarkConfig};
use naia_benches::BenchWorldBuilder;

fn setup_scoped_10e() -> naia_benches::BenchWorld {
    BenchWorldBuilder::new().users(1).entities(10).build()
}

fn setup_scoped_100e() -> naia_benches::BenchWorld {
    BenchWorldBuilder::new().users(1).entities(100).build()
}

fn setup_scoped_1ke() -> naia_benches::BenchWorld {
    BenchWorldBuilder::new().users(1).entities(1_000).build()
}

// Unscoped: entities spawned but NEVER added to a room, so no dirty
// receiver is ever allocated for them. Pure framework-overhead floor.
fn setup_unscoped_1ke() -> naia_benches::BenchWorld {
    BenchWorldBuilder::new()
        .users(1)
        .entities(1_000)
        .unscoped()
        .build()
}

// K=1 mutation, N=10 scoped. Baseline push-model cost.
#[library_benchmark]
#[bench::b(setup_scoped_10e())]
fn dirty_1_in_scoped_10e(mut world: naia_benches::BenchWorld) {
    world.mutate_entities(1);
    world.tick();
}

// K=1 mutation, N=100 scoped. Mid-scale check.
#[library_benchmark]
#[bench::b(setup_scoped_100e())]
fn dirty_1_in_scoped_100e(mut world: naia_benches::BenchWorld) {
    world.mutate_entities(1);
    world.tick();
}

// K=1 mutation, N=1000 scoped.
// PRIMARY GATE: instruction count must be roughly flat vs. the N=10
// variant. Flat count proves Win-3 push model is genuinely O(mutations),
// not O(N).
#[library_benchmark]
#[bench::b(setup_scoped_1ke())]
fn dirty_1_in_scoped_1ke(mut world: naia_benches::BenchWorld) {
    world.mutate_entities(1);
    world.tick();
}

// Unscoped baseline: N=1000 entities exist but none are in any room.
// No dirty receivers are allocated; mutate_entities still writes to the
// component, but the push model has nothing to dispatch. This floor is
// the cost of tick() + mutate with zero replication work — scoped runs
// must not be wildly higher than this for low K.
#[library_benchmark]
#[bench::b(setup_unscoped_1ke())]
fn dirty_1_in_unscoped_1ke(mut world: naia_benches::BenchWorld) {
    world.mutate_entities(1);
    world.tick();
}

library_benchmark_group!(
    name = dirty_receiver_group;
    benchmarks =
        dirty_1_in_scoped_10e,
        dirty_1_in_scoped_100e,
        dirty_1_in_scoped_1ke,
        dirty_1_in_unscoped_1ke
);

main!(
    config = LibraryBenchmarkConfig::default();
    library_benchmark_groups = dirty_receiver_group
);
