use std::time::Duration;

use naia_shared::{BitReader, BitWriter, GameInstant, Serde, SerdeErr, Tick, GAME_TIME_LIMIT};

use crate::connection::{base_time_manager::BaseTimeManager, time_manager::TimeManager};

pub struct HandshakeTimeManager {
    base: BaseTimeManager,
    handshake_pings: u8,
    ping_interval: Duration,
    pong_stats: Vec<(f32, f32)>,
    server_tick: Tick,
    server_tick_instant: GameInstant,
    server_tick_duration_avg: f32,
    server_speedup_potential: f32,
}

impl HandshakeTimeManager {
    pub fn new(ping_interval: Duration, handshake_pings: u8) -> Self {
        let base = BaseTimeManager::new();
        let server_tick_instant = base.game_time_now();
        Self {
            base,
            ping_interval,
            handshake_pings,
            pong_stats: Vec::new(),
            server_tick: 0,
            server_tick_instant,
            server_tick_duration_avg: 0.0,
            server_speedup_potential: 0.0,
        }
    }

    pub(crate) fn write_ping(&mut self) -> BitWriter {
        self.base.write_ping()
    }

    pub(crate) fn read_pong(&mut self, reader: &mut BitReader) -> Result<bool, SerdeErr> {
        // read server tick
        let server_tick = Tick::de(reader)?;

        // read time since last tick
        let server_tick_instant = GameInstant::de(reader)?;

        if let Some((duration_avg, speedup_potential, offset_millis, rtt_millis)) =
            self.base.read_pong(reader)?
        {
            self.server_tick = server_tick;
            self.server_tick_instant = server_tick_instant;
            self.server_tick_duration_avg = duration_avg;
            self.server_speedup_potential = speedup_potential;

            self.buffer_stats(offset_millis, rtt_millis);
            if self.pong_stats.len() >= self.handshake_pings as usize {
                return Ok(true);
            }
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

        // Prune out any pong values outside the Standard Deviation (mitigation)
        let mut pruned_pongs = Vec::new();
        for (time_offset_millis, rtt_millis) in pongs {
            let offset_diff = (time_offset_millis - offset_mean).abs();
            let rtt_diff = (rtt_millis - rtt_mean).abs();
            if offset_diff < offset_stdv && rtt_diff < rtt_stdv {
                pruned_pongs.push((time_offset_millis, rtt_millis));
            }
        }

        // If no pongs were pruned, add the mean values.
        // This may happen if the standard deviation is close to 0.
        // This is a safety measure to ensure the time manager has at least one sample.
        if pruned_pongs.is_empty() {
            pruned_pongs.push((offset_mean, rtt_mean));
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
            self.ping_interval,
            self.base,
            self.server_tick,
            self.server_tick_instant,
            self.server_tick_duration_avg,
            self.server_speedup_potential,
            pruned_rtt_mean,
            rtt_stdv,
            offset_stdv,
        )
    }
}
