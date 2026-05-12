// Criterion entry point for the Naia benchmark suite.
// Run with: cargo criterion -p naia-benches
// Or filter:  cargo criterion -p naia-benches -- tick/

mod tick {
    pub mod active;
    pub mod idle;
    pub mod scope;
}

mod update {
    pub mod immutable;
}

mod authority {
    pub mod contention;
    pub mod cycle;
}

mod wire {
    pub mod bandwidth_realistic_quantized;
}

mod scenarios {
    pub mod halo_btb_16v16;
}

use criterion::criterion_main;

criterion_main!(
    tick::idle::tick_idle,
    tick::active::tick_active,
    tick::scope::tick_scope,
    update::immutable::update_immutable,
    authority::cycle::authority_cycle,
    authority::contention::authority_contention,
    wire::bandwidth_realistic_quantized::wire_bandwidth_realistic_quantized_group,
    scenarios::halo_btb_16v16::halo_btb,
);
