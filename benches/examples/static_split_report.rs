use naia_benches::BenchWorldBuilder;

const TILE_COUNT: usize = 10_000;
const UNIT_COUNT: usize = 32;
const WARMUP: usize = 120;
const MEASURE: usize = 60;

fn measure(label: &str, control: bool) -> u64 {
    let mut world = if control {
        BenchWorldBuilder::new().users(1).entities(TILE_COUNT + UNIT_COUNT).uncapped_bandwidth().build()
    } else {
        BenchWorldBuilder::new().users(1).static_entities(TILE_COUNT).entities(UNIT_COUNT).uncapped_bandwidth().build()
    };
    let unit_range = TILE_COUNT..(TILE_COUNT + UNIT_COUNT);
    for _ in 0..WARMUP { world.mutate_entity_range(unit_range.clone()); world.tick(); }
    let mut total = 0u64;
    for _ in 0..MEASURE { world.mutate_entity_range(unit_range.clone()); world.tick(); total += world.server_outgoing_bytes_per_tick(); }
    let avg = total / MEASURE as u64;
    println!("{label}: {avg} B/tick");
    avg
}

fn main() {
    let control = measure("control  (all-dynamic, tiles push unit IDs to 10k+)", true);
    let treatment = measure("treatment (static tiles, units start at 0)         ", false);
    let saved = control.saturating_sub(treatment);
    let pct = saved as f64 / control as f64 * 100.0;
    println!("saved:    {saved} B/tick  ({pct:.1}%)");
    println!("per unit: {:.1} B/tick per unit", saved as f64 / UNIT_COUNT as f64);
}
