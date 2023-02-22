use naia_shared::{
    sequence_greater_than, wrapping_diff, BitReader, BitWriter, GameDuration, GameInstant, Instant,
    PacketType, PingIndex, PingStore, Serde, SerdeErr, StandardHeader, Tick,
    UnsignedVariableInteger,
};

use log::{info, warn};

use crate::connection::io::Io;

// skews occur over 3 seconds in milliseconds
// should be less than the Ping rate
const SKEW_DURATION_MS: f32 = 1000.0;

/// Is responsible for sending regular ping messages between client/servers
/// and to estimate rtt/jitter
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

    pub fn send_ping(&mut self, io: &mut Io) {
        let mut writer = BitWriter::new();

        // write header
        StandardHeader::new(PacketType::Ping, 0, 0, 0).ser(&mut writer);

        // Record ping
        let ping_index = self.sent_pings.push_new(self.game_time_now());

        // write index
        ping_index.ser(&mut writer);

        //info!("sent Ping: {ping_index} to Server");

        // send packet
        if io.send_writer(&mut writer).is_err() {
            // TODO: pass this on and handle above
            warn!("Client Error: Cannot send ping packet to Server");
        }
    }

    pub fn read_pong(&mut self, reader: &mut BitReader) -> Result<Option<(f32, i32, u32)>, SerdeErr> {
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
        // info!("READ: Tick Duration Average: {tick_duration_avg}");

        // read server sent time
        let server_sent_time = GameInstant::de(reader)?;

        // if this is the most recent Ping or the 1st ping, apply values
        if sequence_greater_than(ping_index, self.most_recent_ping) || self.never_been_pinged {
            self.never_been_pinged = false;
            self.most_recent_ping = ping_index;

            let send_offset_millis = server_received_time.offset_from(&client_sent_time);
            let recv_offset_millis = server_sent_time.offset_from(&client_received_time);

            // info!("Send Offset: {send_offset_millis}, Recv Offset: {recv_offset_millis}");

            let round_trip_time_millis = client_received_time
                .time_since(&client_sent_time)
                .as_millis();
            let server_process_time_millis = server_sent_time
                .time_since(&server_received_time)
                .as_millis();

            // info!("Total RTT: {round_trip_time_millis}, Server Processing Time: {server_process_time_millis}");

            // Final values
            let time_offset_millis = (send_offset_millis + recv_offset_millis) / 2;
            let round_trip_delay_millis = round_trip_time_millis - server_process_time_millis;

            return Ok(Some((tick_duration_avg,  time_offset_millis, round_trip_delay_millis)));
        }

        return Ok(None);
    }

    pub fn game_time_now(&self) -> GameInstant {
        GameInstant::new(&self.start_instant)
    }

    pub fn game_time_since(&self, previous_instant: &GameInstant) -> GameDuration {
        self.game_time_now().time_since(previous_instant)
    }

    pub fn sent_pings_clear(&mut self) {
        self.sent_pings.clear();
    }
}
