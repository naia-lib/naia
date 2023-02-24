use std::time::Duration;

use naia_shared::{
    wrapping_diff, BitReader, BitWriter, GameDuration, GameInstant, Instant, PacketType, PingIndex,
    Serde, SerdeErr, StandardHeader, Tick, UnsignedVariableInteger,
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
    client_diff_avg: f32,
}

impl TimeManager {
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
            client_diff_avg: 0.0,
            tick_speedup_potential: 0.0,
        }
    }

    pub(crate) fn duration_until_next_tick(&self) -> Duration {
        let mut new_instant = self.last_tick_instant.clone();
        new_instant.add_millis(self.tick_interval_millis as u32);
        return new_instant.until();
    }

    /// Whether or not we should emit a tick event
    pub fn recv_server_tick(&mut self) -> bool {
        let time_since_tick_ms = self.last_tick_instant.elapsed().as_secs_f32() * 1000.0;

        if time_since_tick_ms >= self.tick_interval_millis {
            self.record_tick_duration(time_since_tick_ms);
            self.last_tick_instant = Instant::now();
            self.last_tick_game_instant = self.game_time_now();
            self.current_tick = self.current_tick.wrapping_add(1);
            return true;
        }
        return false;
    }

    /// Gets the current tick on the host
    pub fn current_tick(&self) -> Tick {
        self.current_tick
    }

    pub fn current_tick_instant(&self) -> GameInstant {
        self.last_tick_game_instant.clone()
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
            .max(0.0)
            .min(10.0);
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

    pub(crate) fn record_client_tick(&mut self, client_tick: Tick) {
        let ticks_client_ahead_by = wrapping_diff(self.current_tick, client_tick) as f32;
        self.client_diff_avg = (0.9 * self.client_diff_avg) + (0.1 * ticks_client_ahead_by);
    }
}
