# Phase 10 — Cyberlith Capacity Benchmark

**Status:** 🔄 IN PROGRESS
**Theme:** Answer the question: *How many concurrent cyberlith games can one naia server handle?*
**Tick budget:** 25 Hz = 40 ms per tick
**Scenario:** `halo_btb_16v16_10k` — 16 players, 10 000 immutable tiles, 32 mutating units

---

## Why this phase

The Phase 9 correctness floor and Phase 8 wire-format wins give us confidence in naia's
algorithmic properties. What they cannot answer is the *operational* question:

> Given a cyberlith-scale game room at 25 Hz, how many concurrent rooms can one server run?
> Can a typical game client keep up with the incoming stream?

The existing synthetic benches (`tick/idle_matrix_immutable/u_x_n/16u_10000e`) prove O(1)
algorithmic behaviour, but:

- They use a 16 ms tick clock, not 25 Hz (40 ms).
- They do not model the mixed workload: immutable tiles **plus** mutable units in the same room.
- They measure server cost only; client receive cost is invisible.
- They produce a raw nanosecond figure, not a *capacity number*.

This phase fills all four gaps.

---

## Architecture: Hexagonal in the report tool

The bench harness (`benches/`) is infrastructure — it uses Criterion and naia APIs directly;
hexagonal layering doesn't apply there.

The **report tool** (`test/bench_report/`) is where business logic lives and where hexagonal
architecture matters. The current flat structure mixes parsing, computation, and rendering.
This phase restructures it:

```
test/bench_report/src/
  main.rs               ← thin CLI adapter: parses args, wires source → core → sinks
  core/
    mod.rs
    model.rs            ← BenchResult, BenchGroup (moved, unchanged)
    grouper.rs          ← group_results() pure fn (moved, unchanged)
    capacity.rs         ← NEW: ScenarioProfile, CapacityEstimate, estimate() — zero I/O
    assertions.rs       ← WinAssertion domain logic (extracted from assert_wins.rs)
  ports/
    mod.rs
    source.rs           ← trait BenchResultSource { fn load(&self) -> Vec<BenchResult>; }
    sink.rs             ← trait ReportSink { fn emit(&self, report: &Report); }
                           trait AssertionSink { fn emit(&self, r: &AssertionReport) -> bool; }
  adapters/
    mod.rs
    criterion.rs        ← CriterionSource: reads --message-format=json from stdin
    html.rs             ← HtmlSink: Chart.js HTML renderer (moved from renderer.rs + charts.rs)
    capacity_report.rs  ← NEW: CapacityReportSink — human-readable capacity table
    wins_sink.rs        ← WinsSink: --assert-wins stdout output (wraps assert_wins logic)
```

### Ports (traits)

```rust
// ports/source.rs
pub trait BenchResultSource {
    fn load(&self) -> Vec<BenchResult>;
}

// ports/sink.rs
pub trait ReportSink {
    fn emit(&self, results: &[BenchResult], groups: &[BenchGroup]);
}

pub trait AssertionSink {
    /// Returns true if all assertions pass.
    fn emit(&self, report: &AssertionReport) -> bool;
}
```

### Core purity rule

Every function in `core/` takes only data (no `Read`, no `Write`, no `stdin`, no `env::args`).
This makes unit tests trivial — no mocking, no process setup.

---

## Scenario design: `halo_btb_16v16_10k`

### Protocol additions (`benches/src/bench_protocol.rs`)

```rust
/// Immutable tile — stands in for cyberlith's NetworkedTile.
/// No properties: presence is the data (immutable, zero diff-tracking cost).
#[derive(Replicate)]
#[replicate(immutable)]
pub struct HaloTile;

/// Mutable unit — stands in for a moving character with quantized state.
/// Two properties: position (i16 tile coords) and facing (u8 angle).
#[derive(Replicate)]
pub struct HaloUnit {
    pub pos: Property<[i16; 2]>,
    pub facing: Property<u8>,
}
impl HaloUnit {
    pub fn new(x: i16, y: i16, facing: u8) -> Self {
        Self::new_complete([x, y], facing)
    }
}
```

Register both in `bench_protocol()`.

### Scenario constants

```rust
// benches/benches/scenarios/halo_btb_16v16.rs
const PLAYERS:    usize = 16;
const TILE_COUNT: usize = 10_000;
const UNIT_COUNT: usize = 32;   // 16v16 — one unit per player
const TICK_HZ:    u16   = 25;   // 40 ms budget
```

### Four measurement phases

| Phase | What is measured | Criterion method | Proves |
|---|---|---|---|
| **A: level_load** | Time to replicate 10K tiles + 32 units to 16 clients | `iter_custom` | Spawn coalescing holds at cyberlith scale |
| **B: steady_state_idle** | Server tick cost, 0 mutations | `iter_batched` | O(1) idle confirmed at cyberlith workload |
| **C: steady_state_active** | Server tick cost, 32 unit mutations/tick | `iter_batched` | O(mutations) holds; unit movement budget |
| **D: client_receive** | Per-client cost receiving active server tick | `iter_batched` | Client-side viability at 25 Hz |

### Client-side measurement strategy

The existing `TickBreakdown.clients` measures ALL clients' combined I/O.
For client capacity the meaningful number is the **representative single-client cost**.

`BenchWorld` will expose a new `tick_server_then_measure_one_client(client_idx)` that:
1. Advances the clock and runs the server's full tick (rx + process + tx).
2. Routes hub packets.
3. Times ONE client's `receive_all_packets + process_all_packets` in isolation.
4. Drains the remaining clients' buffers (without timing them).

This isolates the single-client receive path cleanly.

---

## Implementation phases

> **Rule:** For every new `core/` function, write the `#[cfg(test)]` tests **first**, then implement.

---

### Phase 10.1 — Scenario protocol (TDD for data types)

**Files touched:** `benches/src/bench_protocol.rs`

1. Add `HaloTile` (immutable marker) and `HaloUnit` (pos + facing).
2. Register in `bench_protocol()`.
3. Tests (inline `#[cfg(test)]`):

```rust
#[test]
fn halo_tile_is_immutable_and_has_no_properties() {
    // Verify the Replicate derive produces an immutable component.
    // Compile-time: the #[replicate(immutable)] attribute must compile.
    // Runtime: no mutable diff-tracking data (checked by zero-sized impl).
    let _ = HaloTile;  // type check: zero-size struct
}

#[test]
fn halo_unit_new_round_trips() {
    let u = HaloUnit::new(5, -3, 128);
    assert_eq!(*u.pos, [5i16, -3i16]);
    assert_eq!(*u.facing, 128u8);
}
```

4. `cargo check -p naia-benches --benches` must pass.

---

### Phase 10.2 — BenchWorld extensions

**Files touched:** `benches/src/lib.rs`

#### 2a. Tick-rate parameterisation

`TICK_MS` is currently a module-level `const`. Make it a per-world field:

```rust
pub struct BenchWorldBuilder {
    // ...existing fields...
    tick_ms: u64,  // default 16
}

impl BenchWorldBuilder {
    /// Set the simulated tick clock. Default is 16 ms (62.5 Hz).
    /// For cyberlith capacity bench use tick_rate_hz(25) → 40 ms.
    pub fn tick_rate_hz(mut self, hz: u16) -> Self {
        self.tick_ms = 1000 / hz as u64;
        self
    }
}
```

Thread `tick_ms` through `BenchWorld::new(...)` and store as `self.tick_ms`. Replace
every `TestClock::advance(TICK_MS)` with `TestClock::advance(self.tick_ms)`.

**Existing callers** omit `tick_rate_hz()` and get the existing default (16 ms). Zero
behaviour change for existing benches. `cargo check` verifies.

#### 2b. Halo-scenario spawn method

```rust
impl BenchWorld {
    /// Spawn the full halo scenario: `tile_count` immutable HaloTile entities
    /// and `unit_count` mutable HaloUnit entities, all added to the room.
    /// Drives ticks until every entity is replicated to every client.
    /// **Not measured** — call from iter_custom setup or iter_batched setup.
    pub fn spawn_halo_scene(&mut self, tile_count: usize, unit_count: usize) {
        // spawn tiles
        for _ in 0..tile_count {
            let e = self.server.spawn_entity(self.server_world.proxy_mut())
                .insert_component(HaloTile)
                .id();
            self.server.room_mut(&self.room_key).add_entity(&e);
            self.server_entities.push(e);
        }
        // spawn units
        for i in 0..unit_count {
            let e = self.server.spawn_entity(self.server_world.proxy_mut())
                .insert_component(HaloUnit::new(i as i16, 0, 0))
                .id();
            self.server.room_mut(&self.room_key).add_entity(&e);
            self.server_entities.push(e);
        }
        // wait for full replication
        let target = tile_count + unit_count;
        let mut ticks = 0;
        while self.client_entity_count() < target && ticks < REPLICATE_TIMEOUT {
            self.tick();
            ticks += 1;
        }
        assert_eq!(
            self.client_entity_count(), target,
            "halo scene did not replicate within {REPLICATE_TIMEOUT} ticks"
        );
    }

    /// Mutate the first `count` HaloUnit entities (increment facing).
    pub fn mutate_halo_units(&mut self, count: usize) {
        let count = count.min(self.server_entities.len());
        for i in 0..count {
            let e = self.server_entities[i];
            if let Some(mut u) = self.server
                .entity_mut(self.server_world.proxy_mut(), &e)
                .component::<HaloUnit>()
            {
                *u.facing = u.facing.wrapping_add(1);
            }
        }
    }
}
```

#### 2c. Single-client receive measurement

```rust
impl BenchWorld {
    /// Run a full server tick (advance clock, server rx+process+tx),
    /// then time ONE client's receive path in isolation.
    /// Drains all other clients' buffers without timing them.
    ///
    /// Use this as the measured operation for client-side capacity benches.
    pub fn tick_server_then_measure_one_client(&mut self, client_idx: usize) -> std::time::Duration {
        let now = TestClock::advance(self.tick_ms);
        // server full tick
        self.server.receive_all_packets();
        self.server.process_all_packets(self.server_world.proxy_mut(), &now);
        self.server.send_all_packets(self.server_world.proxy());
        // measure client[client_idx] receive only
        let (c, w) = &mut self.clients[client_idx];
        let t = std::time::Instant::now();
        c.receive_all_packets(w.proxy_mut(), &now);
        c.process_all_packets(w.proxy_mut(), &now);
        let elapsed = t.elapsed();
        // drain remaining clients (no timing)
        for (i, (client, world)) in self.clients.iter_mut().enumerate() {
            if i == client_idx { continue; }
            client.receive_all_packets(world.proxy_mut(), &now);
            client.process_all_packets(world.proxy_mut(), &now);
            client.send_all_packets(world.proxy_mut());
        }
        drain_all_events(&mut self.server, &mut self.clients);
        elapsed
    }
}
```

#### Verification after 10.2

```bash
cargo check -p naia-benches --benches
```

No regressions in existing benches; new API compiles.

---

### Phase 10.3 — Scenario criterion benches

**New file:** `benches/benches/scenarios/halo_btb_16v16.rs`  
**Update:** `benches/benches/main.rs` — add the scenario criterion group.

```rust
use criterion::{criterion_group, BenchmarkId, Criterion, Throughput};
use naia_benches::{
    bench_protocol::{HaloTile, HaloUnit},
    BenchWorldBuilder,
};

const PLAYERS:    usize = 16;
const TILE_COUNT: usize = 10_000;
const UNIT_COUNT: usize = 32;
const TICK_HZ:    u16   = 25;

pub fn bench_halo_btb(c: &mut Criterion) {
    let mut group = c.benchmark_group("scenarios/halo_btb_16v16");

    // ── Phase A: Level Load ─────────────────────────────────────────────────
    // Measures: time from spawn to full replication across all 16 clients.
    // iter_custom: each iteration is one full replication round (can't be
    // split into setup + measured cleanly; the replication IS the work).
    group.bench_function("level_load", |b| {
        b.iter_custom(|iters| {
            let mut total = std::time::Duration::ZERO;
            for _ in 0..iters {
                // Connect clients but spawn no entities yet.
                let mut world = BenchWorldBuilder::new()
                    .users(PLAYERS)
                    .tick_rate_hz(TICK_HZ)
                    .uncapped_bandwidth()
                    .entities(0)
                    .build();

                let t = std::time::Instant::now();
                world.spawn_halo_scene(TILE_COUNT, UNIT_COUNT);
                total += t.elapsed();
            }
            total
        });
    });

    // ── Phase B: Steady-State Idle ──────────────────────────────────────────
    // Measures: server tick cost with 0 mutations. The pure O(1) floor.
    group.bench_function("steady_state_idle", |b| {
        b.iter_batched(
            || {
                let mut w = BenchWorldBuilder::new()
                    .users(PLAYERS)
                    .tick_rate_hz(TICK_HZ)
                    .uncapped_bandwidth()
                    .entities(0)
                    .build();
                w.spawn_halo_scene(TILE_COUNT, UNIT_COUNT);
                w
            },
            |mut w| w.tick(),
            criterion::BatchSize::SmallInput,
        );
    });

    // ── Phase C: Steady-State Active ────────────────────────────────────────
    // Measures: server tick cost when all 32 units mutate every tick.
    group.bench_function("steady_state_active", |b| {
        b.iter_batched(
            || {
                let mut w = BenchWorldBuilder::new()
                    .users(PLAYERS)
                    .tick_rate_hz(TICK_HZ)
                    .uncapped_bandwidth()
                    .entities(0)
                    .build();
                w.spawn_halo_scene(TILE_COUNT, UNIT_COUNT);
                w
            },
            |mut w| {
                w.mutate_halo_units(UNIT_COUNT);
                w.tick();
            },
            criterion::BatchSize::SmallInput,
        );
    });

    // ── Phase D: Per-Client Receive ─────────────────────────────────────────
    // Measures: one client's receive path after an active server tick.
    // This is the client-side capacity signal: can a game client keep up?
    group.bench_function("client_receive_active", |b| {
        b.iter_batched(
            || {
                let mut w = BenchWorldBuilder::new()
                    .users(PLAYERS)
                    .tick_rate_hz(TICK_HZ)
                    .uncapped_bandwidth()
                    .entities(0)
                    .build();
                w.spawn_halo_scene(TILE_COUNT, UNIT_COUNT);
                w
            },
            |mut w| {
                w.mutate_halo_units(UNIT_COUNT);
                // measured: server tick + client[0] receive only
                w.tick_server_then_measure_one_client(0)
            },
            criterion::BatchSize::SmallInput,
        );
    });

    group.finish();
}

criterion_group!(halo_btb, bench_halo_btb);
```

Register in `benches/benches/main.rs`:
```rust
// add import
mod scenarios { pub mod halo_btb_16v16; }
use scenarios::halo_btb_16v16::halo_btb;

criterion_main!(
    // ... existing groups ...
    halo_btb,
);
```

**Bench IDs produced** (used by assert-wins and capacity report):
- `scenarios/halo_btb_16v16/level_load`
- `scenarios/halo_btb_16v16/steady_state_idle`
- `scenarios/halo_btb_16v16/steady_state_active`
- `scenarios/halo_btb_16v16/client_receive_active`

#### Verification after 10.3

```bash
cargo check -p naia-benches --benches
# Then a quick single run to confirm the benches execute (fast sample count):
cargo criterion -p naia-benches --bench naia -- "scenarios/" 2>/dev/null | head -40
```

---

### Phase 10.4 — Report tool hexagonal refactor (tests-first)

**Goal:** Pure `core/capacity.rs` with unit tests written before implementation;
   rest of report tool reorganised around the hexagonal structure above.

#### Step 1 — Create directory skeleton (no logic yet)

```
test/bench_report/src/
  core/mod.rs
  ports/mod.rs
  adapters/mod.rs
```

#### Step 2 — Move unchanged modules under core/

Move (preserve file content verbatim):
- `model.rs` → `core/model.rs`
- `grouper.rs` → `core/grouper.rs`

Update `mod` declarations in `main.rs`.

#### Step 3 — Ports (thin traits, no logic)

```rust
// ports/source.rs
use crate::core::model::BenchResult;

pub trait BenchResultSource {
    fn load(&self) -> Vec<BenchResult>;
}

// ports/sink.rs
use crate::core::model::{BenchGroup, BenchResult};
use crate::core::assertions::AssertionReport;
use crate::core::capacity::CapacityEstimate;

pub trait ReportSink {
    fn emit(&self, results: &[BenchResult], groups: &[BenchGroup]);
}

pub trait AssertionSink {
    fn emit(&self, report: &AssertionReport) -> bool;
}

pub trait CapacitySink {
    fn emit(&self, estimate: &CapacityEstimate);
}
```

#### Step 4 — Move adapters

Move existing I/O code into adapter modules:
- `parser.rs` → `adapters/criterion.rs` — wrap in `struct CriterionSource; impl BenchResultSource`
- `renderer.rs` + `charts.rs` → `adapters/html.rs` — wrap in `struct HtmlSink; impl ReportSink`
- Extract `assert_wins::run()` logic → `core/assertions.rs` (pure); create
  `adapters/wins_sink.rs` with `struct WinsSink; impl AssertionSink` (prints to stdout)

#### Step 5 — Core: capacity module (tests first, then implement)

**Write tests first** in `core/capacity.rs` `#[cfg(test)]` block:

```rust
// core/capacity.rs

pub const TICK_BUDGET_25HZ_NS: u64 = 40_000_000;
pub const NETWORK_1GBPS_BPS:   u64 = 1_000_000_000;

pub struct ScenarioProfile {
    pub scenario_name:                    &'static str,
    pub tick_budget_ns:                   u64,
    pub network_budget_bps:               u64,
    pub players_per_game:                 u32,
    // Server costs (per game, per tick)
    pub server_idle_ns:                   u64,
    pub server_active_ns:                 u64,
    // Wire (server outbound, per game, per tick)
    pub server_wire_bytes_idle:           u64,
    pub server_wire_bytes_active:         u64,
    // Client cost (one representative client, per tick)
    pub client_receive_active_ns:         u64,
    // Level load (one-shot; converted to ms for display)
    pub level_load_ns:                    u64,
}

#[derive(Debug, PartialEq)]
pub enum Bottleneck { Server, Wire, Client }

pub struct CapacityEstimate {
    pub server_capacity_idle:   u32,
    pub server_capacity_active: u32,
    pub wire_capacity_idle:     u32,
    pub wire_capacity_active:   u32,
    pub client_can_keep_up:     bool,
    pub bottleneck:             Bottleneck,
    pub level_load_ms:          f64,
}

/// Pure function — zero I/O. Testable without any infrastructure.
pub fn estimate(p: &ScenarioProfile) -> CapacityEstimate { ... }

#[cfg(test)]
mod tests {
    use super::*;

    fn make_profile(server_idle_ns: u64, server_active_ns: u64,
                    wire_idle: u64, wire_active: u64,
                    client_active_ns: u64) -> ScenarioProfile {
        ScenarioProfile {
            scenario_name:              "test",
            tick_budget_ns:             TICK_BUDGET_25HZ_NS,
            network_budget_bps:         NETWORK_1GBPS_BPS,
            players_per_game:           16,
            server_idle_ns,
            server_active_ns,
            server_wire_bytes_idle:     wire_idle,
            server_wire_bytes_active:   wire_active,
            client_receive_active_ns:   client_active_ns,
            level_load_ns:              100_000_000, // 100 ms
        }
    }

    #[test]
    fn server_is_bottleneck() {
        let p = make_profile(100_000, 200_000, 100, 200, 50_000);
        let e = estimate(&p);
        assert_eq!(e.server_capacity_idle,   400);   // 40ms / 100µs
        assert_eq!(e.server_capacity_active, 200);   // 40ms / 200µs
        assert!(e.wire_capacity_idle > 1000);        // wire is fine
        assert_eq!(e.bottleneck, Bottleneck::Server);
        assert!(e.client_can_keep_up);
    }

    #[test]
    fn wire_is_bottleneck() {
        // 500 KB/tick × 8 bit/byte × 25 ticks/s = 100 Mbps per game → 10 games on 1 Gbps
        let p = make_profile(1, 1, 500_000, 600_000, 1);
        let e = estimate(&p);
        assert_eq!(e.wire_capacity_idle, 10);
        assert_eq!(e.bottleneck, Bottleneck::Wire);
    }

    #[test]
    fn client_cannot_keep_up() {
        // client takes 90% of the tick budget — not viable
        let p = make_profile(100_000, 200_000, 100, 200, 36_000_000);
        let e = estimate(&p);
        assert!(!e.client_can_keep_up);
    }

    #[test]
    fn zero_server_cost_returns_saturated_capacity() {
        let p = make_profile(0, 0, 0, 0, 0);
        let e = estimate(&p);
        assert_eq!(e.server_capacity_idle,   u32::MAX);
        assert_eq!(e.server_capacity_active, u32::MAX);
        assert_eq!(e.wire_capacity_idle,     u32::MAX);
        assert_eq!(e.wire_capacity_active,   u32::MAX);
    }

    #[test]
    fn level_load_converts_to_ms() {
        let p = make_profile(100_000, 200_000, 100, 200, 50_000);
        let e = estimate(&p);
        assert!((e.level_load_ms - 100.0).abs() < 0.001);
    }

    #[test]
    fn effective_capacity_is_min_of_server_and_wire() {
        // Server supports 400 idle, wire supports only 10 → effective is 10
        let p = make_profile(100_000, 200_000, 500_000, 600_000, 50_000);
        let e = estimate(&p);
        assert_eq!(e.server_capacity_idle,   400);
        assert_eq!(e.wire_capacity_idle,     10);
        assert_eq!(e.bottleneck,             Bottleneck::Wire);
    }
}
```

**Then** implement `estimate()`:
```rust
pub fn estimate(p: &ScenarioProfile) -> CapacityEstimate {
    let server_cap_idle   = saturating_div(p.tick_budget_ns, p.server_idle_ns);
    let server_cap_active = saturating_div(p.tick_budget_ns, p.server_active_ns);

    let ticks_per_sec = 1_000_000_000.0 / p.tick_budget_ns as f64;

    let wire_cap_idle   = wire_capacity(p.server_wire_bytes_idle,   p.network_budget_bps, ticks_per_sec);
    let wire_cap_active = wire_capacity(p.server_wire_bytes_active, p.network_budget_bps, ticks_per_sec);

    // client: warn if receive cost > 10% of tick budget
    let client_can_keep_up = p.client_receive_active_ns < p.tick_budget_ns / 10;

    let bottleneck = {
        let cpu_wins_idle = server_cap_idle.saturating_mul(1) <= wire_cap_idle.saturating_mul(1);
        if !client_can_keep_up {
            Bottleneck::Client
        } else if cpu_wins_idle {
            Bottleneck::Server
        } else {
            Bottleneck::Wire
        }
    };

    CapacityEstimate {
        server_capacity_idle:   server_cap_idle,
        server_capacity_active: server_cap_active,
        wire_capacity_idle:     wire_cap_idle,
        wire_capacity_active:   wire_cap_active,
        client_can_keep_up,
        bottleneck,
        level_load_ms: p.level_load_ns as f64 / 1_000_000.0,
    }
}

fn saturating_div(budget: u64, cost: u64) -> u32 {
    if cost == 0 { u32::MAX } else { (budget / cost).min(u32::MAX as u64) as u32 }
}

fn wire_capacity(bytes_per_tick: u64, budget_bps: u64, ticks_per_sec: f64) -> u32 {
    if bytes_per_tick == 0 { return u32::MAX; }
    let bits_per_game_per_sec = bytes_per_tick as f64 * 8.0 * ticks_per_sec;
    (budget_bps as f64 / bits_per_game_per_sec).floor() as u32
}
```

#### Step 6 — Capacity adapter and --capacity-report CLI mode

```rust
// adapters/capacity_report.rs
use crate::core::capacity::{CapacityEstimate, Bottleneck};
use crate::ports::sink::CapacitySink;

pub struct CapacityReportSink;

impl CapacitySink for CapacityReportSink {
    fn emit(&self, e: &CapacityEstimate) {
        println!("╔══════════════════════════════════════════════════════════╗");
        println!("║  Cyberlith halo_btb_16v16 — Capacity Estimate (25 Hz)   ║");
        println!("╠══════════════════════════════════════════════════════════╣");
        println!("║  Level load (10K tiles → 16 clients): {:>8.1} ms      ║", e.level_load_ms);
        println!("║                                                          ║");
        println!("║  Server capacity (CPU):                                  ║");
        println!("║    idle  (0 mutations/tick):  {:>6} concurrent games   ║", cap_display(e.server_capacity_idle));
        println!("║    active (32 mutations/tick): {:>6} concurrent games   ║", cap_display(e.server_capacity_active));
        println!("║                                                          ║");
        println!("║  Wire capacity (1 Gbps):                                 ║");
        println!("║    idle:                      {:>6} concurrent games   ║", cap_display(e.wire_capacity_idle));
        println!("║    active:                    {:>6} concurrent games   ║", cap_display(e.wire_capacity_active));
        println!("║                                                          ║");
        println!("║  Client (one player at active load):  {}                 ║",
                 if e.client_can_keep_up { "✓ keeps up" } else { "✗ OVERLOADED" });
        println!("║                                                          ║");
        let bottleneck_str = match e.bottleneck {
            Bottleneck::Server => "CPU",
            Bottleneck::Wire   => "Wire",
            Bottleneck::Client => "Client CPU",
        };
        println!("║  Bottleneck: {:<46} ║", bottleneck_str);
        println!("╚══════════════════════════════════════════════════════════╝");
    }
}

fn cap_display(n: u32) -> String {
    if n == u32::MAX { "∞".to_string() } else { n.to_string() }
}
```

#### Step 7 — Wire into main.rs

```rust
// main.rs (after refactor)
mod core { mod model; mod grouper; mod capacity; mod assertions; }
mod ports { mod source; mod sink; }
mod adapters { mod criterion; mod html; mod wins_sink; mod capacity_report; }

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let source = adapters::criterion::CriterionSource;
    let results = source.load();

    if results.is_empty() {
        eprintln!("naia-bench-report: no results on stdin.");
        eprintln!("Usage: cargo criterion --message-format=json 2>/dev/null | cargo run -p naia-bench-report [--assert-wins] [--capacity-report]");
        std::process::exit(1);
    }

    if args.iter().any(|a| a == "--assert-wins") {
        let report = core::assertions::check_all(&results);
        let sink = adapters::wins_sink::WinsSink;
        if !sink.emit(&report) {
            std::process::exit(1);
        }
        return;
    }

    if args.iter().any(|a| a == "--capacity-report") {
        let profile = core::capacity::profile_from_results(&results);
        let estimate = core::capacity::estimate(&profile);
        let sink = adapters::capacity_report::CapacityReportSink;
        sink.emit(&estimate);
        return;
    }

    // Default: HTML report
    let title = extract_title_arg(&args);
    let groups = core::grouper::group_results(results.clone());
    let sink = adapters::html::HtmlSink { title };
    sink.emit(&results, &groups);
}
```

The key new function `core::capacity::profile_from_results(&results) -> ScenarioProfile`
reads the four scenario bench IDs from the result set and fills a `ScenarioProfile`:

```rust
pub fn profile_from_results(results: &[BenchResult]) -> ScenarioProfile {
    let get_ns = |id: &str| -> u64 {
        results.iter()
            .find(|r| r.id == id)
            .map(|r| r.median_ns as u64)
            .unwrap_or(0)
    };
    let get_bytes = |id: &str| -> u64 {
        results.iter()
            .find(|r| r.id == id)
            .and_then(|r| r.throughput_per_iter)
            .unwrap_or(0)
    };
    ScenarioProfile {
        scenario_name:                "halo_btb_16v16_10k",
        tick_budget_ns:               TICK_BUDGET_25HZ_NS,
        network_budget_bps:           NETWORK_1GBPS_BPS,
        players_per_game:             16,
        server_idle_ns:               get_ns("scenarios/halo_btb_16v16/steady_state_idle"),
        server_active_ns:             get_ns("scenarios/halo_btb_16v16/steady_state_active"),
        server_wire_bytes_idle:       get_bytes("wire/bandwidth_realistic_quantized/..."),
        server_wire_bytes_active:     get_bytes("wire/bandwidth_realistic_quantized/..."),
        client_receive_active_ns:     get_ns("scenarios/halo_btb_16v16/client_receive_active"),
        level_load_ns:                get_ns("scenarios/halo_btb_16v16/level_load"),
    }
}
```

Wire bytes come from the existing `bandwidth_realistic_quantized` bench at the 16-user
parameter. If the bench hasn't been run, `server_wire_bytes_*` will be 0 (treated as ∞).

**Tests for profile_from_results:**

```rust
#[test]
fn profile_from_results_extracts_correct_ids() {
    let results = vec![
        make_result("scenarios/halo_btb_16v16/steady_state_idle",   42_000.0),
        make_result("scenarios/halo_btb_16v16/steady_state_active", 84_000.0),
        make_result("scenarios/halo_btb_16v16/client_receive_active", 10_000.0),
        make_result("scenarios/halo_btb_16v16/level_load",          150_000_000.0),
    ];
    let p = profile_from_results(&results);
    assert_eq!(p.server_idle_ns,            42_000);
    assert_eq!(p.server_active_ns,          84_000);
    assert_eq!(p.client_receive_active_ns,  10_000);
    assert!((p.level_load_ns - 150_000_000).abs() < 1000);
}

#[test]
fn profile_from_results_missing_bench_is_zero() {
    let results = vec![];
    let p = profile_from_results(&results);
    assert_eq!(p.server_idle_ns, 0); // missing → 0
}
```

#### Verification after 10.4

```bash
cargo test -p naia-bench-report   # all unit tests pass
cargo check -p naia-bench-report  # compiles cleanly
```

---

### Phase 10.5 — New assert-wins check

Add to `core/assertions.rs` (or `adapters/wins_sink.rs` if assertions stay there):

```rust
// check_halo_capacity: assert the scenario bench results meet
// minimum quality bar before computing the full capacity estimate.
fn check_halo_idle_budget(idx: &BTreeMap<&str, &BenchResult>, out: &mut AssertOutcome) {
    // steady_state_idle must be < 5 ms (125µs headroom per game at 8 concurrent)
    check_threshold(
        idx,
        "scenarios/halo_btb_16v16/steady_state_idle",
        5_000_000.0, // 5 ms
        "halo.idle_budget",
        out,
    );
}

fn check_halo_client_keepup(idx: &BTreeMap<&str, &BenchResult>, out: &mut AssertOutcome) {
    // client_receive_active must be < 4 ms (10% of 40ms tick budget)
    check_threshold(
        idx,
        "scenarios/halo_btb_16v16/client_receive_active",
        4_000_000.0, // 4 ms
        "halo.client_keepup",
        out,
    );
}
```

Add both calls inside `assert_wins::run()`.

These checks are **conditional** — if the scenario benchmarks haven't been run (no matching IDs
in results), they emit `SKIP` not `FAIL`, consistent with existing baseline-regression behaviour.

---

## Verification gate (full)

```bash
# 1. Run the scenario benches only (fast path during development)
cargo criterion -p naia-benches --bench naia -- "scenarios/" --message-format=json 2>/dev/null \
  | cargo run -p naia-bench-report -- --capacity-report

# 2. Full assert-wins gate (includes new halo checks)
cargo criterion -p naia-benches --bench naia --message-format=json 2>/dev/null \
  | cargo run -p naia-bench-report -- --assert-wins

# 3. Unit tests (must always be green before pushing)
cargo test -p naia-bench-report
cargo test -p naia-benches

# 4. wasm32 check (enforced by pre-push hook)
cargo check -p naia-{shared,client,bevy-client} --target wasm32-unknown-unknown --quiet
```

Expected terminal output from `--capacity-report`:
```
╔══════════════════════════════════════════════════════════╗
║  Cyberlith halo_btb_16v16 — Capacity Estimate (25 Hz)   ║
╠══════════════════════════════════════════════════════════╣
║  Level load (10K tiles → 16 clients):    nnn.n ms      ║
║                                                          ║
║  Server capacity (CPU):                                  ║
║    idle  (0 mutations/tick):    nnnn concurrent games   ║
║    active (32 mutations/tick):  nnnn concurrent games   ║
║                                                          ║
║  Wire capacity (1 Gbps):                                 ║
║    idle:                        nnnn concurrent games   ║
║    active:                      nnnn concurrent games   ║
║                                                          ║
║  Client (one player at active load):  ✓ keeps up        ║
║                                                          ║
║  Bottleneck: CPU                                         ║
╚══════════════════════════════════════════════════════════╝
```

---

## Files created / modified

| File | Change |
|---|---|
| `benches/src/bench_protocol.rs` | Add `HaloTile`, `HaloUnit`, register in `bench_protocol()` |
| `benches/src/lib.rs` | Add `tick_rate_hz()` to builder; `spawn_halo_scene()`; `mutate_halo_units()`; `tick_server_then_measure_one_client()` |
| `benches/benches/scenarios/halo_btb_16v16.rs` | **NEW** — four scenario benches |
| `benches/benches/main.rs` | Register `halo_btb` criterion group |
| `test/bench_report/src/main.rs` | Thin CLI adapter; add `--capacity-report` mode |
| `test/bench_report/src/core/model.rs` | Moved from `model.rs` |
| `test/bench_report/src/core/grouper.rs` | Moved from `grouper.rs` |
| `test/bench_report/src/core/capacity.rs` | **NEW** — pure capacity domain |
| `test/bench_report/src/core/assertions.rs` | Extracted from `assert_wins.rs`; add halo checks |
| `test/bench_report/src/ports/source.rs` | **NEW** — `BenchResultSource` trait |
| `test/bench_report/src/ports/sink.rs` | **NEW** — `ReportSink`, `AssertionSink`, `CapacitySink` |
| `test/bench_report/src/adapters/criterion.rs` | Moved from `parser.rs` |
| `test/bench_report/src/adapters/html.rs` | Moved from `renderer.rs` + `charts.rs` |
| `test/bench_report/src/adapters/wins_sink.rs` | Moved I/O half of `assert_wins.rs` |
| `test/bench_report/src/adapters/capacity_report.rs` | **NEW** — `CapacityReportSink` |

---

## Implementation order

```
10.1 → 10.2 → 10.3 → (run scenarios fast once, note IDs) →
10.4 step 1-4 (skeleton + moves) →
10.4 step 5 (tests first, then capacity.rs impl) →
10.4 step 6-7 (capacity adapter + CLI wiring) →
10.5 (new assert-wins checks) →
verification gate
```

After the gate: update `BENCH_PERF_UPGRADE.md` with the capacity headline numbers
(actual measured values), commit, and push.
