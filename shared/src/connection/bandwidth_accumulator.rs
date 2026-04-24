use naia_socket_shared::Instant;

use crate::connection::bandwidth::BandwidthConfig;

/// Token-bucket bandwidth accumulator. Drives the unified priority-sort send
/// loop: budget accumulates as `target_bytes_per_sec × dt`; each successful
/// send spends from the budget; when `remaining() <= 0` the tick's send cycle
/// exits, leaving anything unsent for the next tick.
///
/// Surplus carries into the next tick (Fiedler token-bucket). Per-packet
/// overshoot is permitted once per tick so that a minimum of one packet can
/// always egress even under a tiny budget — see `can_spend()` semantics.
pub(crate) struct BandwidthAccumulator {
    budget_bytes: f64,
    target_bytes_per_sec: f64,
    last_accumulate: Option<Instant>,
    sent_this_tick: bool,
    // Telemetry (D13 always-on).
    bytes_sent_this_tick: u64,
    bytes_sent_last_tick: u64,
    #[cfg(feature = "bench_instrumentation")]
    packets_deferred_this_tick: u32,
    #[cfg(feature = "bench_instrumentation")]
    packets_deferred_last_tick: u32,
}

impl BandwidthAccumulator {
    pub(crate) fn new(config: &BandwidthConfig) -> Self {
        Self {
            budget_bytes: 0.0,
            target_bytes_per_sec: config.target_bytes_per_sec as f64,
            last_accumulate: None,
            sent_this_tick: false,
            bytes_sent_this_tick: 0,
            bytes_sent_last_tick: 0,
            #[cfg(feature = "bench_instrumentation")]
            packets_deferred_this_tick: 0,
            #[cfg(feature = "bench_instrumentation")]
            packets_deferred_last_tick: 0,
        }
    }

    /// Called once per outbound send cycle (tick). Adds
    /// `target_bytes_per_sec × (now - last_accumulate)` to the budget.
    /// Also resets `sent_this_tick` so the one-packet overshoot budget is
    /// available again for the new cycle.
    pub(crate) fn accumulate(&mut self, now: &Instant) {
        if let Some(prev) = &self.last_accumulate {
            let dt_secs = prev.elapsed(now).as_secs_f64();
            self.budget_bytes += self.target_bytes_per_sec * dt_secs;
        }
        self.last_accumulate = Some(now.clone());
        self.sent_this_tick = false;
        // Snapshot last-tick telemetry; reset this-tick counters.
        self.bytes_sent_last_tick = self.bytes_sent_this_tick;
        self.bytes_sent_this_tick = 0;
        #[cfg(feature = "bench_instrumentation")]
        {
            self.packets_deferred_last_tick = self.packets_deferred_this_tick;
            self.packets_deferred_this_tick = 0;
        }
    }

    /// Returns true iff a send of approximately `estimated_bytes` is allowed.
    /// When the accumulator is positive, at least one MTU-sized packet can
    /// always go (overshoot permitted so the bucket can go negative by up to
    /// one packet per tick).
    pub(crate) fn can_spend(&self, estimated_bytes: u32) -> bool {
        if self.budget_bytes >= estimated_bytes as f64 {
            return true;
        }
        // One-packet overshoot: iff budget is currently positive and we haven't
        // spent overshoot yet this tick, allow one oversized packet through.
        if !self.sent_this_tick && self.budget_bytes > 0.0 {
            return true;
        }
        false
    }

    /// Subtract the actual bytes serialized from the budget.
    pub(crate) fn spend(&mut self, actual_bytes: u32) {
        self.budget_bytes -= actual_bytes as f64;
        self.sent_this_tick = true;
        self.bytes_sent_this_tick = self.bytes_sent_this_tick.saturating_add(actual_bytes as u64);
    }

    /// Current remaining budget (may be negative when overshoot occurred).
    #[allow(dead_code)]
    pub(crate) fn remaining(&self) -> f64 {
        self.budget_bytes
    }

    /// Bytes spent during the most-recently-completed tick (D13 telemetry).
    #[allow(dead_code)]
    pub(crate) fn bytes_sent_last_tick(&self) -> u64 {
        self.bytes_sent_last_tick
    }

    /// Record that a packet was deferred due to the budget gate. Always a no-op
    /// unless `bench_instrumentation` is enabled.
    #[inline]
    pub(crate) fn record_deferred(&mut self) {
        #[cfg(feature = "bench_instrumentation")]
        {
            self.packets_deferred_this_tick = self.packets_deferred_this_tick.saturating_add(1);
        }
    }

    /// Packets deferred due to the budget gate during the most-recently-completed
    /// tick. Always returns 0 unless `bench_instrumentation` is enabled.
    #[allow(dead_code)]
    pub(crate) fn packets_deferred_last_tick(&self) -> u32 {
        #[cfg(feature = "bench_instrumentation")]
        {
            self.packets_deferred_last_tick
        }
        #[cfg(not(feature = "bench_instrumentation"))]
        {
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use naia_socket_shared::Instant;

    use super::*;

    fn init_clock() {
        #[cfg(feature = "test_time")]
        naia_socket_shared::TestClock::init(0);
    }

    fn advance(t: &Instant, ms: u32) -> Instant {
        let mut out = t.clone();
        out.add_millis(ms);
        out
    }

    #[test]
    fn initial_budget_is_zero() {
        init_clock();
        let cfg = BandwidthConfig {
            target_bytes_per_sec: 64_000,
        };
        let acc = BandwidthAccumulator::new(&cfg);
        assert_eq!(acc.remaining(), 0.0);
    }

    #[test]
    fn first_accumulate_just_sets_baseline() {
        init_clock();
        let cfg = BandwidthConfig {
            target_bytes_per_sec: 64_000,
        };
        let mut acc = BandwidthAccumulator::new(&cfg);
        let t0 = Instant::now();
        acc.accumulate(&t0);
        // No prior timestamp, so no dt accrued.
        assert_eq!(acc.remaining(), 0.0);
    }

    #[test]
    fn subsequent_accumulate_adds_rate_times_dt() {
        init_clock();
        let cfg = BandwidthConfig {
            target_bytes_per_sec: 64_000,
        };
        let mut acc = BandwidthAccumulator::new(&cfg);
        let t0 = Instant::now();
        acc.accumulate(&t0);
        let t1 = advance(&t0, 1000);
        acc.accumulate(&t1);
        assert!((acc.remaining() - 64_000.0).abs() < 1.0);
    }

    #[test]
    fn spend_subtracts_from_budget() {
        init_clock();
        let cfg = BandwidthConfig {
            target_bytes_per_sec: 64_000,
        };
        let mut acc = BandwidthAccumulator::new(&cfg);
        let t0 = Instant::now();
        acc.accumulate(&t0);
        let t1 = advance(&t0, 1000);
        acc.accumulate(&t1);
        acc.spend(1000);
        assert!((acc.remaining() - 63_000.0).abs() < 1.0);
    }

    #[test]
    fn can_spend_true_when_budget_covers_estimate() {
        init_clock();
        let cfg = BandwidthConfig {
            target_bytes_per_sec: 64_000,
        };
        let mut acc = BandwidthAccumulator::new(&cfg);
        let t0 = Instant::now();
        acc.accumulate(&t0);
        let t1 = advance(&t0, 1000);
        acc.accumulate(&t1);
        assert!(acc.can_spend(1000));
        assert!(acc.can_spend(64_000));
    }

    #[test]
    fn one_packet_overshoot_when_budget_positive_but_short() {
        init_clock();
        let cfg = BandwidthConfig {
            target_bytes_per_sec: 64_000,
        };
        let mut acc = BandwidthAccumulator::new(&cfg);
        let t0 = Instant::now();
        acc.accumulate(&t0);
        let t1 = advance(&t0, 10); // ~640 bytes of budget
        acc.accumulate(&t1);
        // Not enough budget for a 1200-byte packet, but overshoot allowed once.
        assert!(acc.can_spend(1200));
        acc.spend(1200);
        // After spending overshoot, further oversized sends are denied.
        assert!(!acc.can_spend(1200));
    }

    #[test]
    fn overshoot_resets_on_next_accumulate() {
        init_clock();
        let cfg = BandwidthConfig {
            target_bytes_per_sec: 64_000,
        };
        let mut acc = BandwidthAccumulator::new(&cfg);
        let t0 = Instant::now();
        acc.accumulate(&t0);
        let t1 = advance(&t0, 10);
        acc.accumulate(&t1);
        acc.spend(1200);
        // New tick: overshoot allowance refills, even if budget stays negative
        // briefly until the next accumulate delta lands.
        let t2 = advance(&t1, 20); // add ~1280 bytes — back into positive
        acc.accumulate(&t2);
        assert!(acc.can_spend(1200));
    }

    #[test]
    fn telemetry_bytes_sent_snapshots_per_tick() {
        init_clock();
        let cfg = BandwidthConfig { target_bytes_per_sec: 64_000 };
        let mut acc = BandwidthAccumulator::new(&cfg);
        let t0 = Instant::now();
        acc.accumulate(&t0);
        // After first accumulate, last-tick byte count defaults to 0.
        assert_eq!(acc.bytes_sent_last_tick(), 0);
        let t1 = advance(&t0, 1000);
        acc.accumulate(&t1);
        acc.spend(500);
        acc.spend(300);
        // This-tick total isn't visible yet; snapshot happens on next accumulate.
        assert_eq!(acc.bytes_sent_last_tick(), 0);
        let t2 = advance(&t1, 1000);
        acc.accumulate(&t2);
        assert_eq!(acc.bytes_sent_last_tick(), 800);
        // Next tick's counter starts fresh.
        acc.spend(100);
        let t3 = advance(&t2, 1000);
        acc.accumulate(&t3);
        assert_eq!(acc.bytes_sent_last_tick(), 100);
    }

    #[test]
    fn can_spend_false_when_budget_nonpositive_and_no_slack() {
        init_clock();
        let cfg = BandwidthConfig {
            target_bytes_per_sec: 64_000,
        };
        let mut acc = BandwidthAccumulator::new(&cfg);
        let t0 = Instant::now();
        acc.accumulate(&t0);
        let t1 = advance(&t0, 10);
        acc.accumulate(&t1);
        acc.spend(1200);
        // Budget now negative; overshoot already spent this tick.
        assert!(!acc.can_spend(1));
    }
}
