use naia_shared::{BitReader, BitWriter, GameDuration, GameInstant, Instant, PacketType, PingIndex, PingStore, Serde, SerdeErr, StandardHeader, Tick, Timer};
use std::time::Duration;
use log::{info, warn};
use crate::connection::io::Io;

use crate::connection::time_config::TimeConfig;

const HANDSHAKE_PONGS_REQUIRED: u8 = 20;

/// Is responsible for sending regular ping messages between client/servers
/// and to estimate rtt/jitter
pub struct TimeManager {
    pub rtt: f32,
    pub jitter: f32,
    pub tick_duration: f32,
    ping_timer: Timer,
    sent_pings: PingStore,
    handshake_finished: bool,
    handshake_pongs_received: u8,
    start_instant: Instant,
}

impl TimeManager {
    pub fn new(time_config: &TimeConfig, tick_duration: &Duration) -> Self {
        let rtt_average = time_config.rtt_initial_estimate.as_secs_f32() * 1000.0;
        let jitter_average = time_config.jitter_initial_estimate.as_secs_f32() * 1000.0;
        let tick_duration_average = tick_duration.as_secs_f32() * 1000.0;

        TimeManager {
            rtt: rtt_average,
            jitter: jitter_average,
            tick_duration: tick_duration_average,
            ping_timer: Timer::new(time_config.ping_interval),
            sent_pings: PingStore::new(),
            handshake_pongs_received: 0,
            handshake_finished: false,
            start_instant: Instant::now(),
        }
    }

    pub(crate) fn handshake_finished(&self) -> bool {
        self.handshake_finished
    }

    pub(crate) fn handshake_send(&mut self, io: &mut Io) {
        if self.handshake_finished {
            panic!("Handshake should be finished by now");
        }

        self.send_ping(io);
    }

    fn handshake_finalize(&mut self) {

    }

    /// Returns whether a ping message should be sent
    pub fn connection_send(&mut self, io: &mut Io) -> bool {
        if self.ping_timer.ringing() {

            self.ping_timer.reset();

            self.send_ping(io);

            return true;
        }

        return false;
    }

    /// Get an outgoing ping payload
    fn send_ping(&mut self, io: &mut Io) {

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

    /// Process an incoming pong payload
    pub fn process_pong(&mut self, reader: &mut BitReader) -> Result<(), SerdeErr> {

        // important to record receipt time ASAP
        let client_received_time = self.game_time_now();

        let ping_index = PingIndex::de(reader)?;

        let Some(client_sent_time) = self.sent_pings.remove(ping_index) else {
            warn!("Unknown pong received");
            return Ok(());
        };

        let server_received_time = GameInstant::de(reader)?;
        let server_sent_time = GameInstant::de(reader)?;

        let send_offset_millis = server_received_time.offset_from(&client_sent_time);
        let recv_offset_millis = server_sent_time.offset_from(&client_received_time);

        let round_trip_time_millis = client_received_time.time_since(&client_sent_time).as_millis();
        let server_process_time_millis = server_sent_time.time_since(&server_received_time).as_millis();

        let time_offset_millis = (send_offset_millis + recv_offset_millis) / 2;
        let round_trip_delay_millis = round_trip_time_millis - server_process_time_millis;

        //TODO here
        put time_offset in here...
        self.process_new_rtt(round_trip_delay_millis);

        if !self.handshake_finished {
            self.handshake_pongs_received += 1;
            if self.handshake_pongs_received >= HANDSHAKE_PONGS_REQUIRED {
                self.handshake_finished = true;
                self.handshake_finalize();
            }
        }

        Ok(())
    }

    /// Recompute rtt/jitter estimations
    fn process_new_rtt(&mut self, rtt_millis: u32) {
        let rtt_millis_f32 = rtt_millis as f32;

        // TODO: return to proper standard deviation measure
        let new_jitter = ((rtt_millis_f32 - self.rtt) / 2.0).abs();

        self.jitter = (0.9 * self.jitter) + (0.1 * new_jitter);
        self.rtt = (0.9 * self.rtt) + (0.1 * rtt_millis_f32);
    }

    pub fn game_time_now(&self) -> GameInstant {
        GameInstant::new(&self.start_instant)
    }

    pub fn game_time_since(&self, previous_instant: &GameInstant) -> GameDuration {
        self.game_time_now().time_since(previous_instant)
    }

    pub(crate) fn recv_client_tick(&self) -> bool {
        todo!()
    }
    pub(crate) fn client_sending_tick(&self) -> Tick {
        todo!()
    }
    pub(crate) fn client_receiving_tick(&self) -> Tick {
        todo!()
    }
    pub(crate) fn server_receivable_tick(&self) -> Tick {
        todo!()
    }
    pub(crate) fn interpolation(&self) -> f32 {
        todo!()
    }
}
