// iai-callgrind instruction-count benchmark for immutable-component dispatch (Win-5).
// Run with:  cargo bench -p naia-iai --bench immutable_dispatch
// Requires:  valgrind installed (sudo apt install valgrind)
//
// Win-5 invariant: immutable components skip diff-tracking allocation
// (no MutChannel, no UserDiffHandler, no MutReceiver) AND skip per-tick
// mutation dispatch entirely. A single-entity world lets iai isolate
// the per-component pipeline cost:
//   * mutable_update:   one mutable entity, one mutate_entities(1) per tick
//                       — full dispatch pipeline runs.
//   * immutable_update: one immutable entity, tick() only (immutable
//                       components cannot be mutated by design) — no
//                       dispatch pipeline runs.
// Gate: `immutable_update` must cost fewer instructions than
// `mutable_update`. A regression where they converge indicates an
// immutable component is being threaded through the mutable pipeline.

use iai_callgrind::{library_benchmark, library_benchmark_group, main, LibraryBenchmarkConfig};
use naia_benches::BenchWorldBuilder;

fn setup_mutable_1e() -> naia_benches::BenchWorld {
    BenchWorldBuilder::new().users(1).entities(1).build()
}

fn setup_immutable_1e() -> naia_benches::BenchWorld {
    BenchWorldBuilder::new()
        .users(1)
        .entities(1)
        .immutable()
        .build()
}

// Mutable component, single update per tick. Full dispatch pipeline cost.
#[library_benchmark]
#[bench::b(setup_mutable_1e())]
fn mutable_update(mut world: naia_benches::BenchWorld) {
    world.mutate_entities(1);
    world.tick();
}

// Immutable component, single tick (immutable can't be mutated by design).
// PRIMARY GATE: instruction count must be ≤ `mutable_update`.
#[library_benchmark]
#[bench::b(setup_immutable_1e())]
fn immutable_update(mut world: naia_benches::BenchWorld) {
    world.tick();
}

library_benchmark_group!(
    name = immutable_dispatch_group;
    benchmarks = mutable_update, immutable_update
);

main!(
    config = LibraryBenchmarkConfig::default();
    library_benchmark_groups = immutable_dispatch_group
);
