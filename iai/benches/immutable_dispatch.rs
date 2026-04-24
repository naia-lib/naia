// iai-callgrind instruction-count benchmark for immutable-component dispatch (Win-5).
// Run with:  cargo bench -p naia-iai --bench immutable_dispatch
// Requires:  valgrind installed (sudo apt install valgrind)
//
// Win-5 invariant: immutable components skip diff-tracking allocation
// (no MutChannel, no UserDiffHandler, no MutReceiver) AND skip per-tick
// mutation dispatch entirely. So at the same N, `immutable_idle`
// instruction count should be strictly ≤ `mutable_idle`. A regression
// where they converge indicates an immutable component is incurring
// mutable-component-style diff state.

use iai_callgrind::{library_benchmark, library_benchmark_group, main, LibraryBenchmarkConfig};
use naia_benches::BenchWorldBuilder;

fn setup_mutable_1ke() -> naia_benches::BenchWorld {
    BenchWorldBuilder::new().users(1).entities(1_000).build()
}

fn setup_immutable_1ke() -> naia_benches::BenchWorld {
    BenchWorldBuilder::new()
        .users(1)
        .entities(1_000)
        .immutable()
        .build()
}

// Mutable-component idle tick, N=1000. Baseline for Win-5 comparison.
#[library_benchmark]
#[bench::b(setup_mutable_1ke())]
fn mutable_idle_1ke(mut world: naia_benches::BenchWorld) {
    world.tick();
}

// Immutable-component idle tick, N=1000.
// PRIMARY GATE: instruction count must be ≤ `mutable_idle_1ke`.
// Immutable components are excluded from diff tracking entirely, so
// their idle tick should be cheaper in absolute terms.
#[library_benchmark]
#[bench::b(setup_immutable_1ke())]
fn immutable_idle_1ke(mut world: naia_benches::BenchWorld) {
    world.tick();
}

library_benchmark_group!(
    name = immutable_dispatch_group;
    benchmarks = mutable_idle_1ke, immutable_idle_1ke
);

main!(
    config = LibraryBenchmarkConfig::default();
    library_benchmark_groups = immutable_dispatch_group
);
