//! Capacity estimation for the cyberlith halo_btb_16v16_10k scenario.
//!
//! This module is pure: zero I/O, zero side effects. Every function takes
//! data and returns data. Tests cover all branches without mocking.

use crate::core::model::BenchResult;

/// 25 Hz tick budget in nanoseconds.
pub const TICK_BUDGET_25HZ_NS: u64 = 40_000_000;

/// 1 Gbps network budget in bits per second.
pub const NETWORK_1GBPS_BPS: u64 = 1_000_000_000;

/// Bench IDs produced by `benches/benches/scenarios/halo_btb_16v16.rs`.
pub mod ids {
    pub const LEVEL_LOAD:            &str = "scenarios/halo_btb_16v16/level_load";
    pub const STEADY_STATE_IDLE:     &str = "scenarios/halo_btb_16v16/steady_state_idle";
    pub const STEADY_STATE_ACTIVE:   &str = "scenarios/halo_btb_16v16/steady_state_active";
    pub const CLIENT_RECEIVE_ACTIVE: &str = "scenarios/halo_btb_16v16/client_receive_active";
}

/// Measured and configured parameters for the capacity formula.
///
/// All `_ns` fields are nanoseconds; all `_bytes` fields are bytes.
/// Zero means "not measured" — treated as zero-cost / infinite capacity
/// rather than blocking the report.
#[derive(Debug, Clone)]
pub struct ScenarioProfile {
    #[allow(dead_code)]
    pub scenario_name:              &'static str,
    /// Server tick budget (40 ms at 25 Hz).
    pub tick_budget_ns:             u64,
    /// Available network bandwidth in bits per second.
    pub network_budget_bps:         u64,
    /// Connected players per game room.
    #[allow(dead_code)]
    pub players_per_game:           u32,
    /// Server tick cost (per game) with zero unit mutations.
    pub server_idle_ns:             u64,
    /// Server tick cost (per game) when all units mutate.
    pub server_active_ns:           u64,
    /// Server outgoing wire bytes per idle tick (aggregate across all clients in one game).
    pub server_wire_bytes_idle:     u64,
    /// Server outgoing wire bytes per active tick.
    pub server_wire_bytes_active:   u64,
    /// Cost for one representative client to receive and process an active tick.
    pub client_receive_active_ns:   u64,
    /// Wall time for 10K tiles + 32 units to fully replicate to all clients.
    pub level_load_ns:              u64,
}

/// The bottleneck resource that limits concurrent-game capacity.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Bottleneck {
    /// CPU is the binding constraint (server tick cost dominates).
    Server,
    /// Network is the binding constraint (wire bytes/tick dominates).
    Wire,
    /// A single client cannot keep up with the tick rate.
    Client,
}

/// Derived capacity estimate for the scenario.
#[derive(Debug, Clone)]
pub struct CapacityEstimate {
    /// Max concurrent games before server CPU is saturated (idle load).
    pub server_capacity_idle:   u32,
    /// Max concurrent games before server CPU is saturated (active load).
    pub server_capacity_active: u32,
    /// Max concurrent games before 1 Gbps outbound is saturated (idle load).
    pub wire_capacity_idle:     u32,
    /// Max concurrent games before 1 Gbps outbound is saturated (active load).
    pub wire_capacity_active:   u32,
    /// `true` if a single client's receive cost is < 10% of the tick budget.
    pub client_can_keep_up:     bool,
    /// Which resource limits first.
    pub bottleneck:             Bottleneck,
    /// Level load time in milliseconds (for display).
    pub level_load_ms:          f64,
}

// ─── Pure computation ─────────────────────────────────────────────────────────

/// Compute the capacity estimate. Zero I/O — safe to call from tests directly.
pub fn estimate(p: &ScenarioProfile) -> CapacityEstimate {
    let server_cap_idle   = saturating_div(p.tick_budget_ns, p.server_idle_ns);
    let server_cap_active = saturating_div(p.tick_budget_ns, p.server_active_ns);

    let ticks_per_sec = 1_000_000_000.0 / p.tick_budget_ns as f64;
    let wire_cap_idle   = wire_capacity(p.server_wire_bytes_idle,   p.network_budget_bps, ticks_per_sec);
    let wire_cap_active = wire_capacity(p.server_wire_bytes_active, p.network_budget_bps, ticks_per_sec);

    // Client: flag if receive cost ≥ 10% of tick budget.
    let client_can_keep_up = p.client_receive_active_ns < p.tick_budget_ns / 10;

    let bottleneck = if !client_can_keep_up {
        Bottleneck::Client
    } else if server_cap_idle <= wire_cap_idle {
        Bottleneck::Server
    } else {
        Bottleneck::Wire
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

/// Build a `ScenarioProfile` from benchmark results. Missing bench IDs produce
/// zero fields (treated as unmeasured, not as a failure).
pub fn profile_from_results(results: &[BenchResult]) -> ScenarioProfile {
    let ns = |id: &str| -> u64 {
        results.iter()
            .find(|r| r.id == id)
            .map(|r| r.median_ns as u64)
            .unwrap_or(0)
    };
    ScenarioProfile {
        scenario_name:            "halo_btb_16v16_10k",
        tick_budget_ns:           TICK_BUDGET_25HZ_NS,
        network_budget_bps:       NETWORK_1GBPS_BPS,
        players_per_game:         16,
        server_idle_ns:           ns(ids::STEADY_STATE_IDLE),
        server_active_ns:         ns(ids::STEADY_STATE_ACTIVE),
        // Wire bytes not yet in the scenario bench suite; leave zero (= ∞ wire capacity).
        server_wire_bytes_idle:   0,
        server_wire_bytes_active: 0,
        client_receive_active_ns: ns(ids::CLIENT_RECEIVE_ACTIVE),
        level_load_ns:            ns(ids::LEVEL_LOAD),
    }
}

fn saturating_div(budget: u64, cost: u64) -> u32 {
    if cost == 0 {
        u32::MAX
    } else {
        ((budget / cost) as u64).min(u32::MAX as u64) as u32
    }
}

fn wire_capacity(bytes_per_tick: u64, budget_bps: u64, ticks_per_sec: f64) -> u32 {
    if bytes_per_tick == 0 {
        return u32::MAX;
    }
    let bits_per_game_per_sec = bytes_per_tick as f64 * 8.0 * ticks_per_sec;
    (budget_bps as f64 / bits_per_game_per_sec).floor() as u32
}

// ─── Tests — written before estimate() was implemented ────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(
        server_idle_ns:           u64,
        server_active_ns:         u64,
        wire_idle:                u64,
        wire_active:              u64,
        client_active_ns:         u64,
    ) -> ScenarioProfile {
        ScenarioProfile {
            scenario_name:            "test",
            tick_budget_ns:           TICK_BUDGET_25HZ_NS,
            network_budget_bps:       NETWORK_1GBPS_BPS,
            players_per_game:         16,
            server_idle_ns,
            server_active_ns,
            server_wire_bytes_idle:   wire_idle,
            server_wire_bytes_active: wire_active,
            client_receive_active_ns: client_active_ns,
            level_load_ns:            100_000_000, // 100 ms
        }
    }

    #[test]
    fn server_is_bottleneck() {
        // server: 100µs idle → 400 games; wire: tiny → fine
        let e = estimate(&profile(100_000, 200_000, 100, 200, 50_000));
        assert_eq!(e.server_capacity_idle,   400);  // 40ms / 100µs
        assert_eq!(e.server_capacity_active, 200);  // 40ms / 200µs
        assert!(e.wire_capacity_idle > 1_000);
        assert_eq!(e.bottleneck, Bottleneck::Server);
        assert!(e.client_can_keep_up);
    }

    #[test]
    fn wire_is_bottleneck() {
        // 500 KB/tick × 8 bits × 25 ticks/s = 100 Mbps/game → 10 games on 1 Gbps
        let e = estimate(&profile(1, 1, 500_000, 600_000, 1));
        assert_eq!(e.wire_capacity_idle, 10);
        assert_eq!(e.bottleneck, Bottleneck::Wire);
    }

    #[test]
    fn wire_formula_exact() {
        // 40 KB/tick × 8 bits × 25 ticks/s = 8 Mbps/game → 125 games on 1 Gbps
        let e = estimate(&profile(1, 1, 40_000, 40_000, 1));
        assert_eq!(e.wire_capacity_idle, 125);
    }

    #[test]
    fn client_cannot_keep_up() {
        // 36ms client cost > 10% of 40ms budget (threshold: 4ms)
        let e = estimate(&profile(100_000, 200_000, 100, 200, 36_000_000));
        assert!(!e.client_can_keep_up);
        assert_eq!(e.bottleneck, Bottleneck::Client);
    }

    #[test]
    fn client_exactly_at_threshold_can_keep_up() {
        // 3_999_999 ns < 4_000_000 ns (10% of 40ms) → can keep up
        let e = estimate(&profile(100_000, 200_000, 100, 200, 3_999_999));
        assert!(e.client_can_keep_up);
    }

    #[test]
    fn zero_server_cost_returns_saturated_capacity() {
        let e = estimate(&profile(0, 0, 0, 0, 0));
        assert_eq!(e.server_capacity_idle,   u32::MAX);
        assert_eq!(e.server_capacity_active, u32::MAX);
        assert_eq!(e.wire_capacity_idle,     u32::MAX);
        assert_eq!(e.wire_capacity_active,   u32::MAX);
    }

    #[test]
    fn level_load_converts_to_ms() {
        let e = estimate(&profile(100_000, 200_000, 100, 200, 50_000));
        assert!((e.level_load_ms - 100.0).abs() < 0.001);
    }

    #[test]
    fn effective_bottleneck_is_min_of_server_and_wire() {
        // Server: 400 idle games; Wire: only 10 → Wire wins
        let e = estimate(&profile(100_000, 200_000, 500_000, 600_000, 50_000));
        assert_eq!(e.server_capacity_idle, 400);
        assert_eq!(e.wire_capacity_idle,   10);
        assert_eq!(e.bottleneck,           Bottleneck::Wire);
    }

    #[test]
    fn profile_from_results_extracts_correct_ids() {
        fn make(id: &str, median_ns: f64) -> BenchResult {
            BenchResult {
                id: id.to_string(),
                category: "scenarios".to_string(),
                sub_id: id.to_string(),
                param: String::new(),
                median_ns,
                std_dev_ns: 0.0,
                throughput_unit: None,
                throughput_per_iter: None,
            }
        }
        let results = vec![
            make(ids::STEADY_STATE_IDLE,     42_000.0),
            make(ids::STEADY_STATE_ACTIVE,   84_000.0),
            make(ids::CLIENT_RECEIVE_ACTIVE, 10_000.0),
            make(ids::LEVEL_LOAD,            150_000_000.0),
        ];
        let p = profile_from_results(&results);
        assert_eq!(p.server_idle_ns,            42_000);
        assert_eq!(p.server_active_ns,          84_000);
        assert_eq!(p.client_receive_active_ns,  10_000);
        assert_eq!(p.level_load_ns,             150_000_000);
    }

    #[test]
    fn profile_from_results_missing_bench_is_zero() {
        let p = profile_from_results(&[]);
        assert_eq!(p.server_idle_ns,   0);
        assert_eq!(p.server_active_ns, 0);
    }

    #[test]
    fn capacity_from_full_profile() {
        // Simulate a measured run: 42µs idle, 84µs active, 10µs client
        let results = vec![
            make_result(ids::STEADY_STATE_IDLE,     42_000.0),
            make_result(ids::STEADY_STATE_ACTIVE,   84_000.0),
            make_result(ids::CLIENT_RECEIVE_ACTIVE, 10_000.0),
            make_result(ids::LEVEL_LOAD,            50_000_000.0),
        ];
        let p = profile_from_results(&results);
        let e = estimate(&p);
        // 40ms / 42µs = 952 games idle
        assert_eq!(e.server_capacity_idle,   952);
        // 40ms / 84µs = 476 games active
        assert_eq!(e.server_capacity_active, 476);
        assert!(e.client_can_keep_up); // 10µs << 4ms threshold
        assert_eq!(e.bottleneck, Bottleneck::Server); // wire = ∞ (unmeasured)
    }

    fn make_result(id: &str, median_ns: f64) -> BenchResult {
        BenchResult {
            id: id.to_string(),
            category: "scenarios".to_string(),
            sub_id: id.to_string(),
            param: String::new(),
            median_ns,
            std_dev_ns: 0.0,
            throughput_unit: None,
            throughput_per_iter: None,
        }
    }
}
