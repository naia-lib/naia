
use log::warn;

use naia_shared::{
    sequence_greater_than, BitReader, BitWriter, GameInstant, Instant, PacketType, PingIndex,
    PingStore, Serde, SerdeErr, StandardHeader, UnsignedVariableInteger,
};

use crate::connection::{connection::Connection, io::Io};

/// Responsible for keeping track of internal time, as well as sending and receiving Ping/Pong messages
pub struct BaseTimeManager {
    pub start_instant: Instant,
    sent_pings: PingStore,
    most_recent_ping: PingIndex,
    never_been_pinged: bool,
}

impl BaseTimeManager {
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            start_instant: now,
            sent_pings: PingStore::new(),
            most_recent_ping: 0,
            never_been_pinged: true,
        }
    }

    // Ping & Pong

    pub fn write_ping(&mut self) -> BitWriter {
        let mut writer = BitWriter::new();

        // write header
        StandardHeader::new(PacketType::Ping, 0, 0, 0).ser(&mut writer);

        // Record ping
        let ping_index = self.sent_pings.push_new(self.game_time_now());

        // write index
        ping_index.ser(&mut writer);

        writer
    }

    pub fn send_ping(&mut self, io: &mut Io) {
        let writer = self.write_ping();

        // send packet
        if io.send_packet(writer.to_packet()).is_err() {
            // TODO: pass this on and handle above
            warn!("Client Error: Cannot send ping packet to Server");
        }
    }

    pub(crate) fn read_ping(reader: &mut BitReader) -> Result<PingIndex, SerdeErr> {
        // read incoming ping index
        let ping_index = PingIndex::de(reader)?;
        Ok(ping_index)
    }

    pub(crate) fn send_pong(
        connection: &mut Connection,
        io: &mut Io,
        ping_index: PingIndex,
    ) {
        // write pong payload
        let mut writer = BitWriter::new();

        // write header
        connection.base.write_header(PacketType::Pong, &mut writer);

        // write index
        ping_index.ser(&mut writer);

        // send packet
        if io.send_packet(writer.to_packet()).is_err() {
            // TODO: pass this on and handle above
            warn!("Client Error: Cannot send pong packet to Server");
        }
        connection.base.mark_sent();
    }

    pub fn read_pong(
        &mut self,
        reader: &mut BitReader,
    ) -> Result<Option<(f32, f32, i32, u32)>, SerdeErr> {
        // important to record receipt time ASAP
        let client_received_time = self.game_time_now();

        // read ping index
        let ping_index = PingIndex::de(reader)?;

        // get client sent time from ping index
        let Some(client_sent_time) = self.sent_pings.remove(ping_index) else {
            warn!("Unknown pong received");

            // TODO: should bubble up another error
            return Err(SerdeErr);
        };

        // read server received time
        let server_received_time = GameInstant::de(reader)?;

        // read average tick duration
        // convert from microseconds to milliseconds
        let tick_duration_avg = (UnsignedVariableInteger::<9>::de(reader)?.get() as f32) / 1000.0;

        let tick_speeedup_potential =
            (UnsignedVariableInteger::<9>::de(reader)?.get() as f32) / 1000.0;

        // read server sent time
        let server_sent_time = GameInstant::de(reader)?;

        // if this is the most recent Ping or the 1st ping, apply values
        if sequence_greater_than(ping_index, self.most_recent_ping) || self.never_been_pinged {
            self.never_been_pinged = false;
            self.most_recent_ping = ping_index;

            let send_offset_millis = server_received_time.offset_from(&client_sent_time);
            let recv_offset_millis = server_sent_time.offset_from(&client_received_time);

            let round_trip_time_millis = client_received_time
                .time_since(&client_sent_time)
                .as_millis();
            let server_process_time_millis = server_sent_time
                .time_since(&server_received_time)
                .as_millis();

            // Final values
            let time_offset_millis = (send_offset_millis + recv_offset_millis) / 2;
            let round_trip_delay_millis = round_trip_time_millis - server_process_time_millis;

            return Ok(Some((
                tick_duration_avg,
                tick_speeedup_potential,
                time_offset_millis,
                round_trip_delay_millis,
            )));
        }

        return Ok(None);
    }

    pub fn game_time_now(&self) -> GameInstant {
        GameInstant::new(&self.start_instant)
    }

    // pub fn game_time_since(&self, previous_instant: &GameInstant) -> GameDuration {
    //     self.game_time_now().time_since(previous_instant)
    // }

    pub fn sent_pings_clear(&mut self) {
        self.sent_pings.clear();
    }
}
