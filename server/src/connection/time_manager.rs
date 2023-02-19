use std::time::Duration;
use log::info;

use naia_shared::{
    BitReader, BitWriter, GameDuration, GameInstant, Instant, PacketType, PingIndex, Serde,
    SerdeErr, StandardHeader, Tick, Timer, UnsignedVariableInteger,
};

/// Manages the current tick for the host
pub struct TimeManager {
    start_instant: Instant,
    current_tick: Tick,
    last_tick_game_instant: GameInstant,
    last_tick_instant: Instant,
    tick_interval_millis: f32,
    tick_duration_avg: f32,
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
        }
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
    pub fn server_tick(&self) -> Tick {
        self.current_tick
    }

    pub fn game_time_now(&self) -> GameInstant {
        GameInstant::new(&self.start_instant)
    }

    pub fn game_time_since(&self, previous_instant: &GameInstant) -> GameDuration {
        self.game_time_now().time_since(previous_instant)
    }

    pub fn record_tick_duration(&mut self, duration_ms: f32) {
        self.tick_duration_avg = (0.96 * self.tick_duration_avg) + (0.04 * duration_ms);
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

        // write average tick duration as microseconds
        // {
        //     let tick_duration_avg = self.tick_duration_avg;
        //     info!("SEND: Tick Duration Average: {tick_duration_avg}");
        // }
        let tick_duration_avg = UnsignedVariableInteger::<9>::new((self.tick_duration_avg * 1000.0).round() as i128);
        tick_duration_avg.ser(&mut writer);

        // write instant of last tick
        self.last_tick_game_instant.ser(&mut writer);

        // write send time
        self.game_time_now().ser(&mut writer);

        //info!("sent Ping: {ping_index} to Client");

        Ok(writer)
    }
}
