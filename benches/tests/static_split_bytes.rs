//! Proves the bandwidth saving from routing tiles through the static entity pool.
//!
//! | Scenario  | Tile IDs           | Unit IDs  | Expected bits/unit-ref |
//! |-----------|--------------------|-----------|------------------------|
//! | control   | dyn 0..9999        | dyn 10000+| 18 (16-bit varint)     |
//! | treatment | static 0..9999     | dyn 0..31 | 10  (8-bit varint)     |
//!
//! Savings: 8 bits × 32 units × ~2 refs/component = 512 bits = 64 bytes/tick.
//! We assert treatment bytes_per_tick < control bytes_per_tick.

use naia_benches::BenchWorldBuilder;

const TILE_COUNT: usize = 10_000;
const UNIT_COUNT: usize = 32;
const WARMUP_TICKS: usize = 120;
const MEASURE_TICKS: usize = 60;

fn steady_state_bytes(control: bool) -> u64 {
    let mut world = if control {
        BenchWorldBuilder::new()
            .users(1)
            .entities(TILE_COUNT + UNIT_COUNT)
            .uncapped_bandwidth()
            .build()
    } else {
        BenchWorldBuilder::new()
            .users(1)
            .static_entities(TILE_COUNT)
            .entities(UNIT_COUNT)
            .uncapped_bandwidth()
            .build()
    };

    let unit_range = TILE_COUNT..(TILE_COUNT + UNIT_COUNT);

    // Warmup — drive to steady state (all entities fully replicated).
    for _ in 0..WARMUP_TICKS {
        world.mutate_entity_range(unit_range.clone());
        world.tick();
    }

    // Measurement — accumulate bytes over several steady-state ticks.
    let mut total: u64 = 0;
    for _ in 0..MEASURE_TICKS {
        world.mutate_entity_range(unit_range.clone());
        world.tick();
        total += world.server_outgoing_bytes_per_tick();
    }

    total / MEASURE_TICKS as u64
}

#[test]
fn static_split_saves_bytes_per_tick() {
    let control_bytes = steady_state_bytes(true);
    let treatment_bytes = steady_state_bytes(false);

    assert!(
        treatment_bytes < control_bytes,
        "treatment ({treatment_bytes} B/tick) should be smaller than control ({control_bytes} B/tick)"
    );
}

#[test]
fn static_split_saves_at_least_24_bytes_per_tick() {
    // Math floor: 32 units × 1 entity-ref/mutation × 8 bits saved / 8 = 32 bytes.
    // Actual measurement lands a few bytes lower due to packet-boundary alignment
    // (each Remote ref carries an is_static bit, so per-tick bit counts don't align
    // identically between control and treatment). 24 is a stable lower bound.
    let control_bytes = steady_state_bytes(true);
    let treatment_bytes = steady_state_bytes(false);
    let saved = control_bytes.saturating_sub(treatment_bytes);

    assert!(
        saved >= 24,
        "expected ≥24 bytes/tick saved, got control={control_bytes} treatment={treatment_bytes} saved={saved}"
    );
}
