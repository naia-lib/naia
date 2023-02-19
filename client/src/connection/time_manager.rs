use log::info;

use naia_shared::{BaseConnection, BitReader, GameDuration, GameInstant, SerdeErr, Tick, Timer};

use crate::connection::{base_time_manager::BaseTimeManager, io::Io, time_config::TimeConfig};

pub struct TimeManager {
    base: BaseTimeManager,
    ping_timer: Timer,

    pruned_offset_avg: f32,
    raw_offset_avg: f32,
    offset_stdv: f32,

    initial_rtt_avg: f32,
    pruned_rtt_avg: f32,
    raw_rtt_avg: f32,
    rtt_stdv: f32,
}

impl TimeManager {
    pub fn from_parts(
        time_config: TimeConfig,
        base: BaseTimeManager,
        pruned_rtt_avg: f32,
        rtt_stdv: f32,
        offset_stdv: f32,
    ) -> Self {
        Self {
            base,
            ping_timer: Timer::new(time_config.ping_interval),

            pruned_offset_avg: 0.0,
            raw_offset_avg: 0.0,
            offset_stdv,

            initial_rtt_avg: pruned_rtt_avg,
            pruned_rtt_avg,
            raw_rtt_avg: pruned_rtt_avg,
            rtt_stdv,
        }
    }

    // Base

    pub fn send_ping(&mut self, io: &mut Io) -> bool {
        if self.ping_timer.ringing() {
            self.ping_timer.reset();

            self.base.send_ping(io);

            return true;
        }

        return false;
    }

    pub fn read_pong(&mut self, reader: &mut BitReader) -> Result<(), SerdeErr> {
        let (offset_millis, rtt_millis) = self.base.read_pong(reader)?;
        self.process_stats(offset_millis, rtt_millis);
        Ok(())
    }

    fn process_stats(&mut self, offset_millis: i32, rtt_millis: u32) {
        let offset_sample = offset_millis as f32;
        let rtt_sample = rtt_millis as f32;

        self.raw_offset_avg = (0.9 * self.raw_offset_avg) + (0.1 * offset_sample);
        self.raw_rtt_avg = (0.9 * self.raw_rtt_avg) + (0.1 * rtt_sample);

        let offset_diff = offset_sample - self.raw_offset_avg;
        let rtt_diff = rtt_sample - self.raw_rtt_avg;

        self.offset_stdv = ((0.9 * self.offset_stdv.powi(2)) + (0.1 * offset_diff.powi(2))).sqrt();
        self.rtt_stdv = ((0.9 * self.rtt_stdv.powi(2)) + (0.1 * rtt_diff.powi(2))).sqrt();

        if offset_diff.abs() < self.offset_stdv && rtt_diff.abs() < self.rtt_stdv {
            self.pruned_offset_avg = (0.9 * self.pruned_offset_avg) + (0.1 * offset_sample);
            self.pruned_rtt_avg = (0.9 * self.pruned_rtt_avg) + (0.1 * rtt_sample);
            info!("New Pruned Averages");

            info!(" ------- Incoming Offset: {offset_millis}, Incoming RTT: {rtt_millis}");
            let offset_avg = self.pruned_offset_avg;
            let rtt_avg = self.pruned_rtt_avg - self.initial_rtt_avg;
            info!(" ------- Average Offset: {offset_avg}, Average RTT Offset: {rtt_avg}");

        } else {
            info!("Pruned out Sample");
        }


    }

    // GameTime

    pub fn game_time_now(&self) -> GameInstant {
        self.base.game_time_now()
    }

    pub fn game_time_since(&self, previous_instant: &GameInstant) -> GameDuration {
        self.base.game_time_since(previous_instant)
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
