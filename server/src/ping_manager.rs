use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use naia_shared::PacketReader;

#[derive(Debug)]
pub struct PingManager {}

impl PingManager {
    pub fn new() -> Self {
        PingManager {}
    }

    /// Process an incoming ping payload
    pub fn process_ping(&self, ping_payload: &[u8]) -> Box<[u8]> {
        // read incoming ping index
        let mut reader = PacketReader::new(&ping_payload);
        let ping_index = reader.cursor().read_u16::<BigEndian>().unwrap();

        // write pong payload
        let mut out_bytes = Vec::<u8>::new();
        out_bytes.write_u16::<BigEndian>(ping_index).unwrap(); // write index
        out_bytes.into_boxed_slice()
    }
}
