use std::time::Duration;

use criterion::{criterion_group, BatchSize, BenchmarkId, Criterion, Throughput};

use naia_benches::{Archetype, BenchWorldBuilder};

/// Realistic-archetype bandwidth scenarios.
///
/// Unlike `wire/bandwidth/scenario` (which mutates a single `u32` per
/// entity), this group composes `Position` + `Velocity` (+ optional
/// `Rotation`) per entity to measure bytes/tick under a Halo-shaped state
/// load. Each scenario:
///
/// 1. Builds a `BenchWorld` with a single receiving client and zero entities.
/// 2. Spawns the requested mix of archetypes (Player / Projectile / Vehicle).
/// 3. Drives ticks until the client catches up — replication is *not* measured.
/// 4. Mutates every dynamic entity's position+velocity(+rotation) each tick.
/// 5. Calibrates `Throughput::Bytes` from `server.outgoing_bytes_last_tick()`
///    after a 60-tick warmup, so criterion reports bytes/sec directly.
///
/// Most scenarios target one receiving client; the `_4u` / `_16u` variants
/// fan out to 4 and 16 receiving clients respectively, to confirm that
/// per-tick server egress scales linearly with client count for archetype
/// shapes (the toy `BenchComponent` cells already showed this in
/// `wire/bandwidth/scenario/4u_*`).
struct Scenario {
    label: &'static str,
    users: usize,
    players: usize,
    projectiles: usize,
    vehicles: usize,
}

const SCENARIOS: &[Scenario] = &[
    // Pure-player scenarios — isolate the per-player cost.
    Scenario { label: "player_8",  users: 1, players: 8,  projectiles: 0, vehicles: 0 },
    Scenario { label: "player_16", users: 1, players: 16, projectiles: 0, vehicles: 0 },
    Scenario { label: "player_32", users: 1, players: 32, projectiles: 0, vehicles: 0 },
    // Pure-projectile — isolate the smaller P+V archetype.
    Scenario { label: "projectile_30", users: 1, players: 0, projectiles: 30, vehicles: 0 },
    Scenario { label: "projectile_50", users: 1, players: 0, projectiles: 50, vehicles: 0 },
    // Mixed match shapes (1 receiving client — per-client envelope).
    Scenario { label: "halo_4v4",       users: 1, players: 8,  projectiles: 15, vehicles: 0 },
    Scenario { label: "halo_8v8",       users: 1, players: 16, projectiles: 30, vehicles: 2 },
    Scenario { label: "halo_btb_12v12", users: 1, players: 24, projectiles: 40, vehicles: 6 },
    Scenario { label: "halo_btb_16v16", users: 1, players: 32, projectiles: 50, vehicles: 8 },
    Scenario { label: "halo_mega_64",   users: 1, players: 64, projectiles: 80, vehicles: 12 },
    // Multi-client fan-out — confirms server egress = per_client × users.
    Scenario { label: "halo_8v8_4u",       users: 4,  players: 16, projectiles: 30, vehicles: 2 },
    Scenario { label: "halo_8v8_16u",      users: 16, players: 16, projectiles: 30, vehicles: 2 },
    Scenario { label: "halo_btb_16v16_4u", users: 4,  players: 32, projectiles: 50, vehicles: 8 },
];

fn build_and_seed(s: &Scenario) -> (naia_benches::BenchWorld, std::ops::Range<usize>) {
    let mut world = BenchWorldBuilder::new().users(s.users).entities(0).build();
    let mut all_dynamic_start = world.server_entities_len();

    if s.players > 0 {
        let r = world.spawn_archetype(Archetype::Player, s.players);
        all_dynamic_start = all_dynamic_start.min(r.start);
    }
    if s.projectiles > 0 {
        world.spawn_archetype(Archetype::Projectile, s.projectiles);
    }
    if s.vehicles > 0 {
        world.spawn_archetype(Archetype::Vehicle, s.vehicles);
    }

    let total = s.players + s.projectiles + s.vehicles;
    world.replicate_until_caught_up(total);

    let dynamic_end = all_dynamic_start + total;
    (world, all_dynamic_start..dynamic_end)
}

pub fn wire_bandwidth_realistic(c: &mut Criterion) {
    let mut group = c.benchmark_group("wire/bandwidth_realistic");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(5));

    for s in SCENARIOS {
        // Probe to calibrate bytes/tick using Naia's per-tick byte counter.
        let bytes_per_tick = {
            let (mut probe, range) = build_and_seed(s);
            for _ in 0..60 {
                probe.mutate_archetype_range(range.clone());
                probe.tick();
            }
            probe.server_outgoing_bytes_per_tick()
        };

        group.throughput(Throughput::Bytes(bytes_per_tick));
        group.bench_with_input(BenchmarkId::new("scenario", s.label), s, |b, s| {
            b.iter_batched(
                || build_and_seed(s),
                |(mut world, range)| {
                    world.mutate_archetype_range(range);
                    world.tick();
                },
                BatchSize::LargeInput,
            )
        });
    }
    group.finish();
}

criterion_group!(
    name = wire_bandwidth_realistic_group;
    config = Criterion::default();
    targets = wire_bandwidth_realistic
);
