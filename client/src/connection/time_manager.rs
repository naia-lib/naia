use naia_shared::{
    BitReader, BitWriter, GameDuration, GameInstant, Instant, PacketType, PingIndex, PingStore,
    Serde, SerdeErr, StandardHeader, Tick, Timer, GAME_TIME_LIMIT,
};

use log::{info, warn};

use crate::connection::{io::Io, time_config::TimeConfig};

const HANDSHAKE_PONGS_REQUIRED: usize = 20;

/// Is responsible for sending regular ping messages between client/servers
/// and to estimate rtt/jitter
pub struct TimeManager {
    pruned_rtt_avg: f32,
    rtt_stdv: f32,
    pruned_offset_avg: f32,
    offset_stdv: f32,
    tick_duration: f32,
    ping_timer: Timer,
    sent_pings: PingStore,
    handshake_finished: bool,
    handshake_pongs: Vec<(f32, f32)>,
    start_instant: Instant,
}

impl TimeManager {
    pub fn new(time_config: &TimeConfig) -> Self {
        TimeManager {
            pruned_rtt_avg: 0.0,
            rtt_stdv: 0.0,
            pruned_offset_avg: 0.0,
            offset_stdv: 0.0,
            tick_duration: 0.0,
            ping_timer: Timer::new(time_config.ping_interval),
            sent_pings: PingStore::new(),
            handshake_pongs: Vec::new(),
            handshake_finished: false,
            start_instant: Instant::now(),
        }
    }

    // Ping & Pong

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

    pub fn read_pong(&mut self, reader: &mut BitReader) -> Result<(), SerdeErr> {
        // important to record receipt time ASAP
        let client_received_time = self.game_time_now();

        let ping_index = PingIndex::de(reader)?;

        let Some(client_sent_time) = self.sent_pings.remove(ping_index) else {
            warn!("Unknown pong received");
            return Ok(());
        };

        let server_received_time = GameInstant::de(reader)?;
        let server_sent_time = GameInstant::de(reader)?;

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

        if self.handshake_finished {
            self.connection_process_stats(time_offset_millis, round_trip_delay_millis);
        } else {
            self.handshake_buffer_stats(time_offset_millis, round_trip_delay_millis);
            if self.handshake_pongs.len() >= HANDSHAKE_PONGS_REQUIRED {
                self.handshake_finished = true;
                self.handshake_finalize();
            }
        }

        Ok(())
    }

    // Handshake

    pub(crate) fn handshake_finished(&self) -> bool {
        self.handshake_finished
    }

    pub(crate) fn handshake_send(&mut self, io: &mut Io) {
        if self.handshake_finished {
            panic!("Handshake should be finished by now");
        }

        self.send_ping(io);
    }

    fn handshake_buffer_stats(&mut self, time_offset_millis: i32, rtt_millis: u32) {
        let time_offset_millis_f32 = time_offset_millis as f32;
        let rtt_millis_f32 = rtt_millis as f32;

        self.handshake_pongs
            .push((time_offset_millis_f32, rtt_millis_f32));
    }

    // This happens when a necessary # of handshake pongs have been recorded
    fn handshake_finalize(&mut self) {
        let sample_count = self.handshake_pongs.len() as f32;

        let pongs = std::mem::take(&mut self.handshake_pongs);

        // Find the Mean
        let mut offset_mean = 0.0;
        let mut rtt_mean = 0.0;

        for (time_offset_millis, rtt_millis) in &pongs {
            offset_mean += *time_offset_millis;
            rtt_mean += *rtt_millis;
        }

        offset_mean /= sample_count;
        rtt_mean /= sample_count;

        // Find the Variance
        let mut offset_diff_mean = 0.0;
        let mut rtt_diff_mean = 0.0;

        for (time_offset_millis, rtt_millis) in &pongs {
            offset_diff_mean += (*time_offset_millis - offset_mean).powi(2);
            rtt_diff_mean += (*rtt_millis - rtt_mean).powi(2);
        }

        offset_diff_mean /= sample_count;
        rtt_diff_mean /= sample_count;

        // Find the Standard Deviation
        let offset_stdv = offset_diff_mean.sqrt();
        let rtt_stdv = rtt_diff_mean.sqrt();

        // Prune out any pong values outside the standard deviation (mitigation)
        let mut pruned_pongs = Vec::new();
        for (time_offset_millis, rtt_millis) in pongs {
            let offset_diff = (time_offset_millis - offset_mean).abs();
            let rtt_diff = (rtt_millis - rtt_mean).abs();
            if offset_diff < offset_stdv && rtt_diff < rtt_stdv {
                pruned_pongs.push((time_offset_millis, rtt_millis));
            }
        }

        // Find the mean of the pruned pongs
        let pruned_sample_count = pruned_pongs.len() as f32;
        let mut pruned_offset_mean = 0.0;
        let mut pruned_rtt_mean = 0.0;

        for (time_offset_millis, rtt_millis) in pruned_pongs {
            pruned_offset_mean += time_offset_millis;
            pruned_rtt_mean += rtt_millis;
        }

        pruned_offset_mean /= pruned_sample_count;
        pruned_rtt_mean /= pruned_sample_count;

        // Get values we were looking for
        self.pruned_rtt_avg = pruned_rtt_mean;
        self.pruned_offset_avg = pruned_offset_mean;
        self.rtt_stdv = rtt_stdv;
        self.offset_stdv = offset_stdv;

        info!(" ******** RTT AVG: {pruned_rtt_mean}, RTT STDV: {rtt_stdv}, OFFSET AVG: {pruned_offset_mean}, OFFSET STDV: {offset_stdv}");

        // Set internal time to match offset
        if self.pruned_offset_avg < 0.0 {
            let offset_ms = (self.pruned_offset_avg * -1.0) as u32;
            self.start_instant.subtract_millis(offset_ms);
        } else {
            let offset_ms = self.pruned_offset_avg as u32;
            // start_instant should only be able to go BACK in time, otherwise `.elapsed()` might not work
            self.start_instant
                .subtract_millis(GAME_TIME_LIMIT - offset_ms);
        }

        self.pruned_offset_avg = 0.0;

        // Clear out outstanding pings
        self.sent_pings.clear();
    }

    // Connection

    pub fn connection_send(&mut self, io: &mut Io) -> bool {
        if self.ping_timer.ringing() {
            self.ping_timer.reset();

            self.send_ping(io);

            return true;
        }

        return false;
    }

    fn connection_process_stats(&mut self, time_offset_millis: i32, rtt_millis: u32) {
        let offset_avg = self.pruned_offset_avg;
        let rtt_avg = self.pruned_rtt_avg;
        info!(" ------- Average Offset: {offset_avg}, Average RTT: {rtt_avg}");
        info!(" ------- Incoming Offset: {time_offset_millis}, Incoming RTT: {rtt_millis}");

        //let rtt_millis_f32 = rtt_millis as f32;

        // // TODO: return to proper standard deviation measure
        // let new_jitter = ((rtt_millis_f32 - self.rtt_avg) / 2.0).abs();
        //
        // self.rtt_stdv = (0.9 * self.rtt_stdv) + (0.1 * new_jitter);
        // self.rtt_avg = (0.9 * self.rtt_avg) + (0.1 * rtt_millis_f32);
    }

    // GameTime

    pub fn game_time_now(&self) -> GameInstant {
        GameInstant::new(&self.start_instant)
    }

    pub fn game_time_since(&self, previous_instant: &GameInstant) -> GameDuration {
        self.game_time_now().time_since(previous_instant)
    }

    // Tick

    pub(crate) fn recv_client_tick(&self) -> bool {
        false
    }

    pub(crate) fn client_sending_tick(&self) -> Tick {
        0
    }

    pub(crate) fn client_receiving_tick(&self) -> Tick {
        0
    }

    pub(crate) fn server_receivable_tick(&self) -> Tick {
        0
    }

    // Interpolation

    pub(crate) fn interpolation(&self) -> f32 {
        0.0
    }

    pub(crate) fn rtt(&self) -> f32 {
        self.pruned_rtt_avg
    }
    pub(crate) fn jitter(&self) -> f32 {
        self.rtt_stdv // TODO: is this correct? or needs to be rtt_stdv / 2 ?
    }
}
