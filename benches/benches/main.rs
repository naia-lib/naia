// Criterion entry point for the Naia benchmark suite.
// Run with: cargo criterion -p naia-benches
// Or filter:  cargo criterion -p naia-benches -- tick/

mod tick {
    pub mod active;
    pub mod idle;
    pub mod scope;
    pub mod scope_with_rooms;
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
    pub mod mutate_path;
    pub mod mutation;
}

mod authority {
    pub mod contention;
    pub mod grant;
}

mod wire {
    pub mod bandwidth;
    pub mod bandwidth_realistic;
    pub mod bandwidth_realistic_quantized;
    pub mod framing;
}

use criterion::criterion_main;

criterion_main!(
    tick::idle::tick_idle,
    tick::active::tick_active,
    tick::scope::tick_scope,
    tick::scope_with_rooms::tick_scope_with_rooms,
    spawn::single::spawn_single_group,
    spawn::burst::spawn_burst,
    spawn::coalesced::spawn_coalesced,
    spawn::paint_rect::spawn_paint_rect,
    update::mutation::update_mutation,
    update::mutate_path::mutate_path_group,
    update::bulk::update_bulk,
    update::immutable::update_immutable,
    authority::grant::authority_grant_group,
    authority::contention::authority_contention,
    wire::framing::wire_framing_group,
    wire::bandwidth::wire_bandwidth,
    wire::bandwidth_realistic::wire_bandwidth_realistic_group,
    wire::bandwidth_realistic_quantized::wire_bandwidth_realistic_quantized_group,
);
