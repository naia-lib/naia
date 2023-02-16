use std::time::Duration;

use naia_shared::{BitReader, BitWriter, Serde, SerdeErr, Tick};

use super::accumulator::Accumulator;

/// Manages the current tick for the host
pub struct TickManager {
    current_tick: Tick,
    accumulator: Accumulator,
}

impl TickManager {
    /// Create a new TickManager with a given tick interval duration
    pub fn new(tick_interval: Duration) -> Self {
        TickManager {
            current_tick: 0,
            accumulator: Accumulator::new(tick_interval),
        }
    }

    pub fn write_server_tick(&self, writer: &mut BitWriter) {
        self.current_tick.ser(writer);
    }

    pub fn read_client_tick(&self, reader: &mut BitReader) -> Result<Tick, SerdeErr> {
        Tick::de(reader)
    }

    /// Whether or not we should emit a tick event
    pub fn recv_server_ticks(&mut self) -> Tick {
        let ticks = self.accumulator.take_ticks();
        self.current_tick = self.current_tick.wrapping_add(ticks);
        ticks
    }

    /// Gets the current tick on the host
    pub fn server_tick(&self) -> Tick {
        self.current_tick
    }
}
