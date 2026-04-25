// Criterion entry point for the Naia benchmark suite.
// Run with: cargo criterion -p naia-benches
// Or filter:  cargo criterion -p naia-benches -- tick/

mod tick {
    pub mod active;
    pub mod idle;
    pub mod scope;
}

mod spawn {
    pub mod burst;
    pub mod coalesced;
    pub mod paint_rect;
    pub mod single;
}

mod update {
    pub mod bulk;
    pub mod immutable;
    pub mod mutation;
}

mod authority {
    pub mod contention;
    pub mod grant;
}

mod wire {
    pub mod bandwidth;
    pub mod bandwidth_realistic;
    pub mod framing;
}

use criterion::criterion_main;

criterion_main!(
    tick::idle::tick_idle,
    tick::active::tick_active,
    tick::scope::tick_scope,
    spawn::single::spawn_single_group,
    spawn::burst::spawn_burst,
    spawn::coalesced::spawn_coalesced,
    spawn::paint_rect::spawn_paint_rect,
    update::mutation::update_mutation,
    update::bulk::update_bulk,
    update::immutable::update_immutable,
    authority::grant::authority_grant_group,
    authority::contention::authority_contention,
    wire::framing::wire_framing_group,
    wire::bandwidth::wire_bandwidth,
    wire::bandwidth_realistic::wire_bandwidth_realistic_group,
);
