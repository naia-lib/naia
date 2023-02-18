use std::time::Duration;
use naia_shared::{BaseConnection, BitReader, BitWriter, PacketType, PingIndex, Serde, SerdeErr, Tick, Timer};

/// Manages the current tick for the host
pub struct TimeManager {
    current_tick: Tick,
    timer: Timer,
}

impl TimeManager {
    /// Create a new TickManager with a given tick interval duration
    pub fn new(tick_interval: Duration) -> Self {
        Self {
            current_tick: 0,
            timer: Timer::new(tick_interval),
        }
    }

    pub fn write_server_tick(&self, writer: &mut BitWriter) {
        self.current_tick.ser(writer);
    }

    /// Whether or not we should emit a tick event
    pub fn recv_server_tick(&mut self) -> bool {
        if self.timer.ringing() {
            self.timer.reset();
            self.current_tick = self.current_tick.wrapping_add(1);
            return true;
        }
        false
    }

    /// Gets the current tick on the host
    pub fn server_tick(&self) -> Tick {
        self.current_tick
    }

    pub(crate) fn process_ping(
        &self,
        connection: &mut BaseConnection,
        reader: &mut BitReader,
    ) -> Result<BitWriter, SerdeErr> {
        // read client tick
        let client_tick = Tick::de(reader)?;

        // read incoming ping index
        let ping_index = PingIndex::de(reader)?;

        // write pong payload
        let mut writer = BitWriter::new();

        // write header
        connection.write_outgoing_header(PacketType::Pong, &mut writer);

        // write server tick
        self.current_tick.ser(&mut writer);

        // write index
        ping_index.ser(&mut writer);

        Ok(writer)
    }
}
