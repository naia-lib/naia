use naia_shared::{
    BaseConnection, BitReader, BitWriter, PacketType, PingIndex, Serde, SerdeErr, Tick,
};

pub struct TimeManager {
    current_tick: Tick,
}

impl TimeManager {
    pub fn new() -> Self {
        Self { current_tick: 0 }
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
