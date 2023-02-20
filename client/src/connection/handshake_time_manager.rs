use log::info;

use naia_shared::{BitReader, SerdeErr, GAME_TIME_LIMIT};

use crate::connection::{
    base_time_manager::BaseTimeManager, io::Io, time_config::TimeConfig, time_manager::TimeManager,
};

const HANDSHAKE_PONGS_REQUIRED: usize = 7;

pub struct HandshakeTimeManager {
    base: BaseTimeManager,
    time_config: TimeConfig,
    pong_stats: Vec<(f32, f32)>,
}

impl HandshakeTimeManager {
    pub fn new(time_config: TimeConfig) -> Self {
        Self {
            base: BaseTimeManager::new(),
            time_config: time_config.clone(),
            pong_stats: Vec::new(),
        }
    }

    pub(crate) fn send_ping(&mut self, io: &mut Io) {
        self.base.send_ping(io);
    }

    pub(crate) fn read_pong(&mut self, reader: &mut BitReader) -> Result<bool, SerdeErr> {
        let (offset_millis, rtt_millis) = self.base.read_pong(reader)?;

        self.buffer_stats(offset_millis, rtt_millis);
        if self.pong_stats.len() >= HANDSHAKE_PONGS_REQUIRED {
            return Ok(true);
        }

        return Ok(false);
    }

    fn buffer_stats(&mut self, time_offset_millis: i32, rtt_millis: u32) {
        let time_offset_millis_f32 = time_offset_millis as f32;
        let rtt_millis_f32 = rtt_millis as f32;

        self.pong_stats
            .push((time_offset_millis_f32, rtt_millis_f32));
    }

    // This happens when a necessary # of handshake pongs have been recorded
    pub fn finalize(mut self) -> TimeManager {
        let sample_count = self.pong_stats.len() as f32;

        let pongs = std::mem::take(&mut self.pong_stats);

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

        info!(" ******** RTT AVG: {pruned_rtt_mean}, RTT STDV: {rtt_stdv}, OFFSET AVG: {pruned_offset_mean}, OFFSET STDV: {offset_stdv}");

        // Set internal time to match offset
        if pruned_offset_mean < 0.0 {
            let offset_ms = (pruned_offset_mean * -1.0) as u32;
            self.base.start_instant.subtract_millis(offset_ms);
        } else {
            let offset_ms = pruned_offset_mean as u32;
            // start_instant should only be able to go BACK in time, otherwise `.elapsed()` might not work
            self.base
                .start_instant
                .subtract_millis(GAME_TIME_LIMIT - offset_ms);
        }

        // Clear out outstanding pings
        self.base.sent_pings_clear();

        TimeManager::from_parts(
            self.time_config,
            self.base,
            pruned_rtt_mean,
            rtt_stdv,
            offset_stdv,
        )
    }
}
