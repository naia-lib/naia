use naia_shared::{sequence_greater_than, BitReader, BitWriter, GameDuration, GameInstant, Instant, PacketType, PingIndex, PingStore, Serde, SerdeErr, StandardHeader, Tick, UnsignedVariableInteger, wrapping_diff};

use log::{info, warn};

use crate::connection::io::Io;

const SKEW_DURATION_MS: f32 = 1000.0; // skews occur over 1 second in milliseconds

/// Is responsible for sending regular ping messages between client/servers
/// and to estimate rtt/jitter
pub struct BaseTimeManager {
    pub start_instant: Instant,
    sent_pings: PingStore,
    most_recent_ping: PingIndex,
    never_been_pinged: bool,

    server_tick: Tick,
    server_tick_instant: GameInstant,
    server_tick_duration_avg: GameDuration,

    last_server_tick_instant: GameInstant,
    last_server_tick_duration_avg: GameDuration,

    skew_accumulator: f32,
    skewed_server_tick_instant: GameInstant,
    skewed_server_tick_duration_avg: GameDuration,
}

impl BaseTimeManager {
    pub fn new() -> Self {
        let now = Instant::now();
        let server_tick_instant = GameInstant::new(&now);
        let last_server_tick_instant = server_tick_instant.clone();
        let skewed_server_tick_instant = server_tick_instant.clone();
        Self {
            start_instant: now,
            sent_pings: PingStore::new(),
            most_recent_ping: 0,
            never_been_pinged: true,

            server_tick: 0,
            server_tick_instant,
            server_tick_duration_avg: GameDuration::from_millis(0),

            last_server_tick_instant,
            last_server_tick_duration_avg: GameDuration::from_millis(0),

            skew_accumulator: 0.0,
            skewed_server_tick_instant,
            skewed_server_tick_duration_avg: GameDuration::from_millis(0),
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

        if self.never_been_pinged {
            // if this is the first Ping, set some initial values
            self.never_been_pinged = false;
            self.most_recent_ping = ping_index;

            self.server_tick = server_tick;
            self.server_tick_instant = server_tick_instant;
            self.server_tick_duration_avg = GameDuration::from_millis(tick_duration_avg);

            self.skew_accumulator = 0.0;
            self.last_server_tick_instant = self.server_tick_instant.clone();
            self.last_server_tick_duration_avg = self.server_tick_duration_avg.clone();
            self.skewed_server_tick_instant = self.server_tick_instant.clone();
            self.skewed_server_tick_duration_avg = self.server_tick_duration_avg.clone();

        } else {
            // if this is the most recent Ping, set some values
            if sequence_greater_than(ping_index, self.most_recent_ping) {
                self.most_recent_ping = ping_index;

                // find the last instant while the skew is still active
                self.last_server_tick_instant = self.tick_to_instant(server_tick);
                self.last_server_tick_duration_avg = self.skewed_server_tick_duration_avg.clone();
                // reset skew
                self.skew_accumulator = 0.0;

                self.server_tick = server_tick;
                self.server_tick_instant = server_tick_instant;
                self.server_tick_duration_avg = GameDuration::from_millis(tick_duration_avg);

                info!("Most Recent Ping: {ping_index}, Server Tick: {server_tick}, Avg Duration: {tick_duration_avg}");
            }
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

    pub(crate) fn skew_ticks(&mut self, delta_millis: f32) {
        if self.skew_accumulator >= SKEW_DURATION_MS {
            return;
        }

        self.skew_accumulator += delta_millis;
        if self.skew_accumulator > SKEW_DURATION_MS {
            self.skew_accumulator = SKEW_DURATION_MS;
        }

        let interpolation = self.skew_accumulator / SKEW_DURATION_MS;

        let server_tick_skew_distance = self.server_tick_instant.time_since(&self.last_server_tick_instant).as_millis() as f32;
        self.skewed_server_tick_instant = self.last_server_tick_instant.add_millis((server_tick_skew_distance * interpolation) as u32);

        let server_tick_duration_skew_distance = (self.server_tick_duration_avg.as_millis() as f32) - (self.last_server_tick_duration_avg.as_millis() as f32);
        self.skewed_server_tick_duration_avg = self.last_server_tick_duration_avg.add_millis((server_tick_duration_skew_distance * interpolation) as u32);
    }

    // Uses skewed values
    pub(crate) fn instant_to_tick(&self, instant: &GameInstant) -> Tick {
        let offset_ms = self.skewed_server_tick_instant.offset_from(instant);
        let offset_ticks_f32 = (offset_ms as f32) / (self.skewed_server_tick_duration_avg.as_millis() as f32);
        return self.server_tick.clone().wrapping_add_signed(offset_ticks_f32 as i16);
    }

    // Uses skewed values
    fn tick_to_instant(&self, tick: Tick) -> GameInstant {
        let tick_diff = wrapping_diff(self.server_tick, tick);
        let tick_diff_duration = (tick_diff as i32) * (self.skewed_server_tick_duration_avg.as_millis() as i32);
        if tick_diff_duration >= 0 {
            // positive
            return self.skewed_server_tick_instant.add_millis(tick_diff_duration as u32);
        } else {
            // negative
            return self.skewed_server_tick_instant.sub_millis((tick_diff_duration * -1) as u32);
        }
    }

    // Uses skewed values
    pub(crate) fn tick_duration_avg(&self) -> GameDuration {
        self.skewed_server_tick_duration_avg.clone()
    }
}
