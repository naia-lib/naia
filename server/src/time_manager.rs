use std::time::Duration;

use naia_shared::{
    BitReader, BitWriter, GameDuration, GameInstant, Instant, PacketType, PingIndex, Serde,
    SerdeErr, StandardHeader, Tick, UnsignedVariableInteger,
};

/// Manages the current tick for the host
pub struct TimeManager {
    start_instant: Instant,
    current_tick: Tick,
    last_tick_game_instant: GameInstant,
    last_tick_instant: Instant,
    tick_interval_millis: f32,
    tick_duration_avg: f32,
    tick_duration_avg_min: f32,
    tick_duration_avg_max: f32,
    tick_speedup_potential: f32,
}

impl TimeManager {
    /// Max ticks of grid catch-up before [`Self::recv_server_tick`] resyncs to
    /// `now` instead of bursting the whole backlog. Bounds the worst-case
    /// catch-up after a long stall (debugger / severe overload) to a few sim
    /// steps; steady-state operation never approaches this.
    const MAX_CATCHUP_TICKS: u32 = 4;

    /// Create a new TickManager with a given tick interval duration
    pub fn new(tick_interval: Duration) -> Self {
        let start_instant = Instant::now();
        let last_tick_instant = start_instant.clone();
        let last_tick_game_instant = GameInstant::new(&start_instant);
        let tick_interval_millis = tick_interval.as_secs_f32() * 1000.0;
        let tick_duration_avg = tick_interval_millis;

        Self {
            start_instant,
            current_tick: 0,
            last_tick_game_instant,
            last_tick_instant,
            tick_interval_millis,
            tick_duration_avg,
            tick_duration_avg_min: tick_duration_avg,
            tick_duration_avg_max: tick_duration_avg,
            tick_speedup_potential: 0.0,
        }
    }

    // pub(crate) fn duration_until_next_tick(&self) -> Duration {
    //     let mut new_instant = self.last_tick_instant.clone();
    //     new_instant.add_millis(self.tick_interval_millis as u32);
    //     return new_instant.until();
    // }

    /// Whether or not we should emit a tick event
    /// Advance the tick clock by at most one tick, returning whether a tick
    /// fired. Callers loop until this returns `false` to drain all ticks due
    /// at `now` (catch-up). Tick N is pinned to the fixed grid
    /// `epoch + N·interval`: on fire we advance `last_tick_instant` by exactly
    /// one interval (`+= interval`, NOT `= now`) so a check that lands a few ms
    /// late never skips a tick and the loss is never made permanent — the
    /// reset-to-now predecessor drifted below the configured rate (e.g. ~15Hz)
    /// and emitted ~80ms gaps when checked only once per tick under jitter.
    ///
    /// Catch-up is bounded: if the clock has run more than [`Self::MAX_CATCHUP_TICKS`]
    /// behind (a long stall — debugger, severe overload), we resync the grid to
    /// `now` and emit a single tick rather than bursting the whole backlog.
    pub fn recv_server_tick(&mut self, now: &Instant) -> bool {
        let time_since_tick_ms = self.last_tick_instant.elapsed(now).as_secs_f32() * 1000.0;

        if time_since_tick_ms < self.tick_interval_millis {
            return false;
        }

        if time_since_tick_ms > self.tick_interval_millis * (Self::MAX_CATCHUP_TICKS as f32) {
            // Pathological lag: drop the backlog, resync the grid to now.
            self.record_tick_duration(time_since_tick_ms);
            self.last_tick_instant = now.clone();
        } else {
            // Steady state / brief catch-up: advance the grid by exactly one
            // interval so tick N stays at epoch + N·interval.
            self.record_tick_duration(self.tick_interval_millis);
            self.last_tick_instant
                .add_millis(self.tick_interval_millis as u32);
        }
        self.last_tick_game_instant = self.game_time_now();
        self.current_tick = self.current_tick.wrapping_add(1);
        true
    }

    /// Gets the current tick of the Server
    pub fn current_tick(&self) -> Tick {
        self.current_tick
    }

    pub fn current_tick_instant(&self) -> GameInstant {
        self.last_tick_game_instant
    }

    pub fn average_tick_duration(&self) -> Duration {
        Duration::from_millis(self.tick_duration_avg.round() as u64)
    }

    pub fn game_time_now(&self) -> GameInstant {
        GameInstant::new(&self.start_instant)
    }

    pub fn game_time_since(&self, previous_instant: &GameInstant) -> GameDuration {
        self.game_time_now().time_since(previous_instant)
    }

    pub fn record_tick_duration(&mut self, duration_ms: f32) {
        self.tick_duration_avg = (0.9 * self.tick_duration_avg) + (0.1 * duration_ms);

        if self.tick_duration_avg < self.tick_duration_avg_min {
            self.tick_duration_avg_min = self.tick_duration_avg;
        } else {
            self.tick_duration_avg_min =
                (0.99999 * self.tick_duration_avg_min) + (0.00001 * self.tick_duration_avg);
        }

        if self.tick_duration_avg > self.tick_duration_avg_max {
            self.tick_duration_avg_max = self.tick_duration_avg;
        } else {
            self.tick_duration_avg_max =
                (0.999 * self.tick_duration_avg_max) + (0.001 * self.tick_duration_avg);
        }

        self.tick_speedup_potential = (((self.tick_duration_avg_max - self.tick_duration_avg_min)
            / self.tick_duration_avg_min)
            * 30.0)
            .clamp(0.0, 10.0);
    }

    pub(crate) fn process_ping(&self, reader: &mut BitReader) -> Result<BitWriter, SerdeErr> {
        let server_received_time = self.game_time_now();

        // read incoming ping index
        let ping_index = PingIndex::de(reader)?;

        // start packet writer
        let mut writer = BitWriter::new();

        // write pong payload
        StandardHeader::new(PacketType::Pong, 0, 0, 0).ser(&mut writer);

        // write server tick
        self.current_tick.ser(&mut writer);

        // write server tick instant
        self.last_tick_game_instant.ser(&mut writer);

        // write index
        ping_index.ser(&mut writer);

        // write received time
        server_received_time.ser(&mut writer);

        // write average tick duration as microseconds
        let tick_duration_avg =
            UnsignedVariableInteger::<9>::new((self.tick_duration_avg * 1000.0).round() as i128);
        tick_duration_avg.ser(&mut writer);

        let tick_speedup_potential = UnsignedVariableInteger::<9>::new(
            (self.tick_speedup_potential * 1000.0).round() as i128,
        );
        tick_speedup_potential.ser(&mut writer);

        // write send time
        self.game_time_now().ser(&mut writer);

        Ok(writer)
    }
}

#[cfg(test)]
mod tick_grid_measurement {
    //! Quantifies the tick-timing behavior of `recv_server_tick` (reset-to-now,
    //! +1, no catch-up) under different "when is it checked" patterns — the
    //! patterns produced by the production service loop. The function takes the
    //! check time as a param, so we drive it deterministically with synthetic
    //! `Instant`s (no real-clock harness needed).
    //!
    //! Two patterns over a 4s span (= 100 grid ticks at 40ms):
    //!   OLD: ~5ms poll (8 checks/tick) — masks jitter, fires within ~5ms of
    //!        each 40ms boundary.
    //!   NEW: one check per ~40ms loop, with work-offset jitter ±J ms (the
    //!        `recv_server_tick` call lands at a variable offset into each
    //!        iteration). When a check lands <40ms after the last FIRE the tick
    //!        is skipped, and reset-to-now makes the loss permanent → drift.

    use super::TimeManager;
    use naia_shared::Instant;
    use std::time::Duration;

    /// Deterministic pseudo-jitter in [0, max_ms].
    fn jitter(k: usize, max_ms: u32) -> u32 {
        if max_ms == 0 {
            return 0;
        }
        ((k.wrapping_mul(2_654_435_761) >> 11) as u32) % (max_ms + 1)
    }

    /// Drive the real `recv_server_tick` with check times (ms offsets from a
    /// base). Returns (fires, spacings_ms between consecutive fires).
    fn drive(check_offsets: &[u32]) -> (u32, Vec<u32>) {
        let mut tm = TimeManager::new(Duration::from_millis(40));
        let base = Instant::now();
        let mut fires = 0u32;
        let mut last_fire: Option<u32> = None;
        let mut spacings = Vec::new();
        for &off in check_offsets {
            let mut now = base.clone();
            now.add_millis(off);
            // Match the real callers (take_tick_events): drain all due ticks.
            while tm.recv_server_tick(&now) {
                fires += 1;
                if let Some(lf) = last_fire {
                    spacings.push(off - lf);
                }
                last_fire = Some(off);
            }
        }
        (fires, spacings)
    }

    fn min_max(v: &[u32]) -> (u32, u32) {
        (
            v.iter().copied().min().unwrap_or(0),
            v.iter().copied().max().unwrap_or(0),
        )
    }

    #[test]
    fn measure_recv_server_tick_drift_and_jitter() {
        let span_ms = 4000u32;
        let expected = span_ms / 40; // 100 grid ticks

        // OLD: ~5ms poll with ±1ms jitter.
        let old: Vec<u32> = (1..=(span_ms / 5))
            .map(|k| k * 5 + jitter(k as usize, 1))
            .collect();
        let (old_fires, old_sp) = drive(&old);

        eprintln!(
            "\n[TICK-GRID] span={}ms  expected grid ticks = {}",
            span_ms, expected
        );
        eprintln!(
            "[TICK-GRID] OLD 5ms-poll : fires={:>3}  drift={:>+3}  spacing(min..max)={:?}  >=80ms gaps={}",
            old_fires,
            old_fires as i32 - expected as i32,
            min_max(&old_sp),
            old_sp.iter().filter(|&&s| s >= 80).count()
        );

        // NEW: one check per 40ms loop, work-offset jitter ±J.
        for j in [1u32, 2, 4, 8] {
            let new: Vec<u32> = (1..=(span_ms / 40))
                .map(|k| k * 40 + jitter(k as usize, j))
                .collect();
            let (new_fires, new_sp) = drive(&new);
            let effective_hz = new_fires as f32 / (span_ms as f32 / 1000.0);
            eprintln!(
                "[TICK-GRID] NEW 40ms ±{}ms: fires={:>3}  drift={:>+3}  spacing(min..max)={:?}  >=80ms gaps={}  eff={:.2}Hz",
                j,
                new_fires,
                new_fires as i32 - expected as i32,
                min_max(&new_sp),
                new_sp.iter().filter(|&&s| s >= 80).count(),
                effective_hz
            );
        }

        // The GRID target (floor((now-epoch)/dt)) would emit exactly `expected`
        // ticks, each pinned to its 40ms slot — zero drift, spacing == 40ms.
        eprintln!(
            "[TICK-GRID] GRID target  : fires={:>3}  drift= +0  spacing=40ms (grid-aligned)\n",
            expected
        );
    }

    /// REGRESSION GUARD (deterministic): closes the `test_time` gap. The
    /// deterministic test suite advances the clock by exactly one tick per
    /// update, so it has ZERO jitter and cannot expose tick-grid bugs (the
    /// reset-to-now predecessor passed the whole gate while silently drifting to
    /// 15-23Hz with dropped-tick hitches in production). Here we feed the real
    /// `recv_server_tick` a jittery once-per-40ms check pattern — what the
    /// production loop actually produces — and require it to hold the grid: emit
    /// exactly `floor(span/dt)` ticks (no drift) with no dropped-tick gap
    /// (no spacing ≥ 2·dt). Fails if the grid pinning regresses to reset-to-now.
    #[test]
    fn grid_tick_holds_rate_under_jitter() {
        let span_ms = 4000u32;
        let expected = span_ms / 40;
        for j in [0u32, 1, 2, 4, 8] {
            let checks: Vec<u32> = (1..=(span_ms / 40))
                .map(|k| k * 40 + jitter(k as usize, j))
                .collect();
            let (fires, sp) = drive(&checks);
            assert_eq!(
                fires, expected,
                "jitter ±{j}ms: tick-rate drift — fired {fires}, expected {expected} (grid regression?)"
            );
            let max_spacing = sp.iter().copied().max().unwrap_or(0);
            assert!(
                max_spacing < 80,
                "jitter ±{j}ms: dropped-tick hitch — max spacing {max_spacing}ms ≥ 2·dt (grid regression?)"
            );
        }
    }

    /// REAL-CLOCK smoke: drives `recv_server_tick` over a real ~500ms span with
    /// real `thread::sleep` grid-pacing + jittery work — the integration the
    /// `test_time` gate cannot run. Catch-up makes the rate load-immune, so the
    /// `Hz` bound is robust; the gap bound is loose (only catches gross
    /// systematic hitches, tolerant of occasional CI scheduling hiccups).
    /// Skipped in `test_time` builds (the native clock is required).
    #[test]
    #[cfg(not(feature = "test_time"))]
    fn real_clock_loop_holds_tick_rate() {
        use std::time::Instant as StdInstant;
        let period = Duration::from_millis(40);
        let mut tm = TimeManager::new(period);
        let test_dur = Duration::from_millis(500);
        let start = StdInstant::now();
        let mut next_deadline = start + period;
        let mut fires = 0u32;

        while start.elapsed() < test_dur {
            // Simulate jittery cell.update() work (1..=9ms).
            std::thread::sleep(Duration::from_millis(1 + (fires as u64 % 9)));
            // Tick gen against the REAL clock (grid catch-up loop, as callers do).
            let now_naia = Instant::now();
            while tm.recv_server_tick(&now_naia) {
                fires += 1;
            }
            // grid-pace on an absolute deadline (mirrors service.rs::run).
            let now = StdInstant::now();
            if now < next_deadline {
                std::thread::sleep(next_deadline - now);
                next_deadline += period;
            } else {
                next_deadline = now + period;
            }
        }

        // Compare against the grid count for the ACTUAL elapsed time (avoids
        // the partial-last-tick boundary bias of fires/elapsed). Catch-up makes
        // the fire count load-immune, so this is robust; the -2 tolerance only
        // forgives edge/partial ticks. The reset-to-now regression drifted to
        // 15-23Hz (≈7-11 fires here) — well below the grid count — so it's
        // caught; occasional CI scheduling hiccups are not (catch-up recovers).
        let grid_count = (start.elapsed().as_millis() / 40) as u32;
        assert!(
            fires + 2 >= grid_count,
            "real-clock: fired {fires}, grid expected ~{grid_count} ticks — tick-rate drift (regression?)"
        );
    }
}
