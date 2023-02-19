use std::time::Duration;

use naia_shared::{
    BitReader, BitWriter, GameDuration, GameInstant, Instant, PacketType, PingIndex, Serde,
    SerdeErr, StandardHeader, Tick, Timer, UnsignedVariableInteger,
};

/// Manages the current tick for the host
pub struct TimeManager {
    start_instant: Instant,
    current_tick: Tick,
    tick_interval: GameDuration,
    last_tick_instant: GameInstant,
    tick_duration_avg: f32,
}

impl TimeManager {
    /// Create a new TickManager with a given tick interval duration
    pub fn new(tick_interval: Duration) -> Self {
        let now = Instant::now();
        let last_tick_instant = GameInstant::new(&now);
        Self {
            start_instant: now,
            current_tick: 0,
            tick_interval: GameDuration::from_millis(tick_interval.as_millis() as u32),
            last_tick_instant,
            tick_duration_avg: tick_interval.as_millis() as f32,
        }
    }

    /// Whether or not we should emit a tick event
    pub fn recv_server_tick(&mut self) -> bool {
        let now = self.game_time_now();
        let time_since_tick = now.time_since(&self.last_tick_instant);

        if time_since_tick >= self.tick_interval {
            self.record_tick_duration(time_since_tick);
            self.last_tick_instant = now;
            self.current_tick = self.current_tick.wrapping_add(1);
            return true;
        }
        return false;
    }

    /// Gets the current tick on the host
    pub fn server_tick(&self) -> Tick {
        self.current_tick
    }

    pub fn game_time_now(&self) -> GameInstant {
        GameInstant::new(&self.start_instant)
    }

    pub fn game_time_since(&self, previous_instant: &GameInstant) -> GameDuration {
        self.game_time_now().time_since(previous_instant)
    }

    pub fn record_tick_duration(&mut self, duration: GameDuration) {
        let millis = duration.as_millis() as f32;
        self.tick_duration_avg = (0.96 * self.tick_duration_avg) + (0.04 * millis);
    }

    pub(crate) fn process_ping(&self, reader: &mut BitReader) -> Result<BitWriter, SerdeErr> {
        let server_received_time = self.game_time_now();

        // read incoming ping index
        let ping_index = PingIndex::de(reader)?;

        //info!("received Ping: {ping_index} from Client");

        // start packet writer
        let mut writer = BitWriter::new();

        // write pong payload
        StandardHeader::new(PacketType::Pong, 0, 0, 0).ser(&mut writer);

        // write index
        ping_index.ser(&mut writer);

        // write received time
        server_received_time.ser(&mut writer);

        // write current tick
        self.current_tick.ser(&mut writer);

        // write average tick duration
        let tick_duration_avg = UnsignedVariableInteger::<6>::new(self.tick_duration_avg as i128);
        tick_duration_avg.ser(&mut writer);

        // write instant of last tick
        self.last_tick_instant.ser(&mut writer);

        // write send time
        self.game_time_now().ser(&mut writer);

        //info!("sent Ping: {ping_index} to Client");

        Ok(writer)
    }
}
