use log::{info, warn};

use naia_shared::{BaseConnection, BitReader, GameDuration, GameInstant, Instant, sequence_greater_than, SerdeErr, Tick, Timer};

use crate::connection::{base_time_manager::BaseTimeManager, io::Io, time_config::TimeConfig};

pub struct TimeManager {
    base: BaseTimeManager,
    ping_timer: Timer,

    // Stats
    pruned_offset_avg: f32,
    raw_offset_avg: f32,
    offset_stdv: f32,

    initial_rtt_avg: f32,
    pruned_rtt_avg: f32,
    raw_rtt_avg: f32,
    rtt_stdv: f32,

    // Ticks
    accumulator: f32,
    last_tick_check_instant: Instant,
    client_receiving_tick: Tick,
    client_sending_tick: Tick,
    server_receivable_tick: Tick,
    client_receiving_instant: GameInstant,
    client_sending_instant: GameInstant,
    server_receivable_instant: GameInstant,
}

impl TimeManager {
    pub fn from_parts(
        time_config: TimeConfig,
        base: BaseTimeManager,
        pruned_rtt_avg: f32,
        rtt_stdv: f32,
        offset_stdv: f32,
    ) -> Self {

        let now = base.game_time_now();
        let latency_ms = (pruned_rtt_avg / 2.0) as u32;
        let major_jitter_ms = (rtt_stdv / 2.0 * 3.0) as u32;
        let tick_duration_ms = base.tick_duration_avg().round() as u32;

        let client_receiving_instant = get_client_receiving_target(&now, latency_ms, major_jitter_ms, tick_duration_ms);
        let client_sending_instant = get_client_sending_target(&now, latency_ms, major_jitter_ms, tick_duration_ms);
        let server_receivable_instant = get_server_receivable_target(&now, latency_ms, major_jitter_ms);

        let client_receiving_tick = base.instant_to_tick(&client_receiving_instant);
        let client_sending_tick = base.instant_to_tick(&client_sending_instant);
        let server_receivable_tick = base.instant_to_tick(&server_receivable_instant);

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

            accumulator: 0.0,
            last_tick_check_instant: Instant::now(),

            client_receiving_tick,
            client_sending_tick,
            server_receivable_tick,

            client_receiving_instant,
            client_sending_instant,
            server_receivable_instant,
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

    pub(crate) fn check_ticks(&mut self) -> (bool, bool) {
        // updates client_receiving_tick
        // returns (true, _) if a client_receiving_tick has incremented
        // returns (_, true) if a client_sending_tick or server_receivable_tick has incremented
        let prev_client_receiving_tick = self.client_receiving_tick;
        let prev_client_sending_tick = self.client_sending_tick;

        {
            let time_elapsed = self.last_tick_check_instant.elapsed().as_secs_f32() * 1000.0;
            self.last_tick_check_instant = Instant::now();
            self.accumulator += time_elapsed;
            if self.accumulator < 1.0 {
                return (false, false);
            }
        }
        let millis_elapsed = self.accumulator;
        self.accumulator -= millis_elapsed;

        // TODO: take elapsed millis and skew the base tick if you need to

        // Target Instants
        let now: GameInstant = self.game_time_now();
        let latency_ms: u32 = self.latency() as u32;
        let major_jitter_ms: u32 = (self.jitter() * 3.0) as u32;

        // skew ticks so that tick_duration_avg and instant_to_tick() is properly adjusted
        self.base.skew_ticks(millis_elapsed);
        let tick_duration_ms: u32 = self.base.tick_duration_avg().round() as u32;

        // find targets
        let client_receiving_target = get_client_receiving_target(&now, latency_ms, major_jitter_ms, tick_duration_ms);
        let client_sending_target = get_client_sending_target(&now, latency_ms, major_jitter_ms, tick_duration_ms);
        let server_receivable_target = get_server_receivable_target(&now, latency_ms, major_jitter_ms);

        // find speeds
        let client_receiving_speed = offset_to_speed(self.client_receiving_instant.offset_from(&client_receiving_target));
        let client_sending_speed = offset_to_speed(self.client_sending_instant.offset_from(&client_sending_target));
        let server_receivable_speed = offset_to_speed(self.server_receivable_instant.offset_from(&server_receivable_target));

        // apply speeds
        self.client_receiving_instant = self.client_receiving_instant.add_millis((millis_elapsed * client_receiving_speed) as u32);
        self.client_sending_instant = self.client_sending_instant.add_millis((millis_elapsed * client_sending_speed) as u32);
        self.server_receivable_instant = self.server_receivable_instant.add_millis((millis_elapsed * server_receivable_speed) as u32);

        // convert current instants into ticks

        self.client_receiving_tick = self.base.instant_to_tick(&self.client_receiving_instant);
        self.client_sending_tick = self.base.instant_to_tick(&self.client_sending_instant);
        self.server_receivable_tick = self.base.instant_to_tick(&self.server_receivable_instant);

        // sanity checks
        // TODO: get rid of these?
        // Self::sanity_check(self.client_receiving_tick, prev_client_receiving_tick.wrapping_add(1));
        // Self::sanity_check(prev_client_receiving_tick, self.client_receiving_tick);
        // Self::sanity_check(self.client_sending_tick, prev_client_sending_tick.wrapping_add(1));
        // Self::sanity_check(prev_client_sending_tick, self.client_sending_tick);


        // TODO: figure out how to get these to work again
        // let receiving_incremented = self.client_receiving_tick == prev_client_receiving_tick.wrapping_add(1);
        // let sending_incremented = self.client_sending_tick == prev_client_sending_tick.wrapping_add(1);
        let receiving_incremented = self.client_receiving_tick != prev_client_receiving_tick;
        let sending_incremented = self.client_sending_tick != prev_client_sending_tick;

        // if receiving_incremented {
        //     let a = self.client_receiving_tick;
        //     warn!("Recv Tick: {prev_client_receiving_tick} -> {a}");
        // }
        // if sending_incremented {
        //     let a = self.client_sending_tick;
        //     warn!("Send Tick: {prev_client_sending_tick} -> {a}");
        // }

        return (receiving_incremented, sending_incremented);
    }

    pub(crate) fn client_receiving_tick(&self) -> Tick {
        self.client_receiving_tick
    }

    pub(crate) fn client_sending_tick(&self) -> Tick {
        self.client_sending_tick
    }

    pub(crate) fn server_receivable_tick(&self) -> Tick {
        self.server_receivable_tick
    }

    // Interpolation

    pub(crate) fn interpolation(&self) -> f32 {
        0.0
    }

    pub(crate) fn rtt(&self) -> f32 {
        self.pruned_rtt_avg
    }
    pub(crate) fn jitter(&self) -> f32 {
        self.rtt_stdv / 2.0
    }
    pub(crate) fn latency(&self) -> f32 {
        self.pruned_rtt_avg / 2.0
    }


    fn sanity_check(a: Tick, b: Tick) {
        if sequence_greater_than(a, b) {
            warn!("{a} shouldn't be greater than {b}");
        }
    }
}

fn get_client_receiving_target(now: &GameInstant, latency: u32, jitter: u32, tick_duration: u32) -> GameInstant {
    now.sub_millis(latency).sub_millis(jitter).sub_millis(tick_duration)
}

fn get_client_sending_target(now: &GameInstant, latency: u32, jitter: u32, tick_duration: u32) -> GameInstant {
    now.add_millis(latency).add_millis(jitter).add_millis(tick_duration)
}

fn get_server_receivable_target(now: &GameInstant, latency: u32, jitter: u32) -> GameInstant {
    now.add_millis(latency).sub_millis(jitter)
}

fn offset_to_speed(mut offset: i32) -> f32 {
    if offset <= OFFSET_MIN {
        offset *= -1;
        return 1.0 / (offset as f32);
    }

    if offset >= OFFSET_MAX {
        return (offset as f32) / (OFFSET_MAX as f32);
    }

    offset += OFFSET_FLOOR_INV;
    // offset is now >= 0 and <= OFFSET_RANGE

    let output_range = (offset as f32) / (OFFSET_RANGE as f32);
    // output_range is now >= 0.0 and <= 1.0

    return (output_range * SPEED_RANGE) + SPEED_MIN;
}

const OFFSET_MIN: i32 = -10;
const OFFSET_MAX: i32 = 10;
const SPEED_MIN: f32 = 0.1;
const SPEED_MAX: f32 = 1.0;

const OFFSET_RANGE: i32 = (OFFSET_MAX - 1) - (OFFSET_MIN + 1);
const OFFSET_FLOOR_INV: i32 = (OFFSET_MIN + 1) * -1;
const SPEED_RANGE: f32 = SPEED_MAX - SPEED_MIN;

// Tests
#[cfg(test)]
mod offset_to_speed_tests {
    use crate::connection::time_manager::{OFFSET_MAX, OFFSET_MIN, OFFSET_RANGE, offset_to_speed, SPEED_MAX, SPEED_MIN, SPEED_RANGE};

    #[test]
    fn min_speed() {
        assert_eq!(offset_to_speed(OFFSET_MIN), SPEED_MIN);
    }

    #[test]
    fn max_speed() {
        assert_eq!(offset_to_speed(OFFSET_MAX), SPEED_MAX);
    }

    #[test]
    fn middle_speed() {
        let middle_offset = ((OFFSET_MAX - OFFSET_MIN) / 2) + OFFSET_MIN;
        let middle_speed = ((SPEED_MAX - SPEED_MIN) / 2.0) + SPEED_MIN;
        assert_eq!(offset_to_speed(middle_offset), middle_speed);
    }

    #[test]
    fn over_max_speed() {
        let offset = OFFSET_MAX + OFFSET_RANGE;

        // TODO: derive these values?
        assert_eq!(offset_to_speed(offset), 2.8);
    }

    #[test]
    fn under_max_speed() {
        let offset = OFFSET_MIN - OFFSET_RANGE;

        // TODO: derive these values?
        assert_eq!(offset_to_speed(offset), 0.035714287);
    }
}
