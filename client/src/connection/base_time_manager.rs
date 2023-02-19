use naia_shared::{
    sequence_greater_than, BitReader, BitWriter, GameDuration, GameInstant, Instant, PacketType,
    PingIndex, PingStore, Serde, SerdeErr, StandardHeader, Tick, UnsignedVariableInteger,
};

use log::{info, warn};

use crate::connection::io::Io;

/// Is responsible for sending regular ping messages between client/servers
/// and to estimate rtt/jitter
pub struct BaseTimeManager {
    pub start_instant: Instant,
    sent_pings: PingStore,
    most_recent_ping: PingIndex,
    last_server_tick: Tick,
    last_server_tick_instant: GameInstant,
    server_tick_duration_avg: GameDuration,
}

impl BaseTimeManager {
    pub fn new() -> Self {
        let now = Instant::now();
        let last_server_tick_instant = GameInstant::new(&now);
        Self {
            sent_pings: PingStore::new(),
            start_instant: now,
            most_recent_ping: 0,
            last_server_tick: 0,
            last_server_tick_instant,
            server_tick_duration_avg: GameDuration::from_millis(0),
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

    pub fn read_pong(&mut self, reader: &mut BitReader) -> Result<(i32, u32), SerdeErr> {
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

        // read server tick
        let server_tick = Tick::de(reader)?;

        // read average tick duration (ms)
        let tick_duration_avg = UnsignedVariableInteger::<6>::de(reader)?.get() as u32;

        // read time since last tick
        let server_tick_instant = GameInstant::de(reader)?;

        // read server sent time
        let server_sent_time = GameInstant::de(reader)?;

        // if this is the most recent Ping, set some values
        if sequence_greater_than(ping_index, self.most_recent_ping) {
            self.most_recent_ping = ping_index;
            self.last_server_tick = server_tick;
            self.last_server_tick_instant = server_tick_instant;
            self.server_tick_duration_avg = GameDuration::from_millis(tick_duration_avg);

            info!("Most Recent Ping: {ping_index}, Server Tick: {server_tick}, Avg Duration: {tick_duration_avg}");
        }

        // {
        //     let client_sent_time_ms = client_sent_time.as_millis();
        //     info!("Client Sent Time: {client_sent_time_ms}");
        //     let server_received_time_ms = server_received_time.as_millis();
        //     info!("Server Received Time: {server_received_time_ms}");
        //     let server_sent_time_ms = server_sent_time.as_millis();
        //     info!("Server Sent Time: {server_sent_time_ms}");
        //     let client_received_time_ms = client_received_time.as_millis();
        //     info!("Client Received Time: {client_received_time_ms}");
        // }

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

        Ok((time_offset_millis, round_trip_delay_millis))
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
