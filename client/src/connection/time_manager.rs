use log::info;

use naia_shared::{BaseConnection, BitReader, GameDuration, GameInstant, SerdeErr, Tick, Timer};

use crate::connection::{base_time_manager::BaseTimeManager, io::Io, time_config::TimeConfig};

pub struct TimeManager {
    base: BaseTimeManager,
    ping_timer: Timer,
    pruned_offset_avg: f32,
    raw_offset_avg: f32,
    offset_stdv: f32,

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

    fn process_stats(&mut self, time_offset_millis: i32, rtt_millis: u32) {
        // let offset_avg = self.pruned_offset_avg;
        // let rtt_avg = self.pruned_rtt_avg;
        // info!(" ------- Average Offset: {offset_avg}, Average RTT: {rtt_avg}");
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
