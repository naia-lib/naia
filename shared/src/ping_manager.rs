use std::time::Duration;

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use crate::{PacketReader, PacketWriter, Timer};

/// Handles pinging the remote host to measure RTT & jitter
#[derive(Debug)]
pub struct PingManager {
    timer: Timer,
    sample_size: u8,
    current_samples: u8,
}

impl PingManager {
    /// Create a new PingManager
    pub fn new(ping_interval: Duration, sample_size: u8) -> Self {
        PingManager {
            timer: Timer::new(ping_interval),
            sample_size,
            current_samples: 0,
        }
    }

    /// Process ping data from an incoming packet
    pub fn read_data(&self, reader: &mut PacketReader) {
        //        let cursor = reader.get_cursor();
        //        let val1 = cursor.read_u8().unwrap();
        //        let val2: u16 =
        // cursor.read_u16::<BigEndian>().unwrap().into();
    }

    /// Returns whether the PingManager has data to write into an outgoing
    /// packet
    pub fn should_write(&self) -> bool {
        self.current_samples < self.sample_size || self.timer.ringing()
    }

    /// Writes a ping message into an outgoing packet
    pub fn write_data(&self, writer: &mut PacketWriter) {
        //        writer.ping_working_bytes.write_u8(0).unwrap();
        //        writer.ping_working_bytes.write_u16::<BigEndian>(0).unwrap();
    }
}
