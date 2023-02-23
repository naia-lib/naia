use log::{info, warn};

use naia_shared::{sequence_greater_than, sequence_less_than, BaseConnection, BitReader, GameDuration, GameInstant, Instant, SerdeErr, Tick, Timer, wrapping_diff};

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

    server_tick: Tick,
    server_tick_instant: GameInstant,
    server_tick_duration_avg: f32,
    server_tick_duration_min: f32,
    server_tick_duration_max: f32,
    danger_adjust: f32,

    last_tick_check_instant: Instant,
    pub client_receiving_tick: Tick,
    pub client_sending_tick: Tick,
    pub server_receivable_tick: Tick,
    client_receiving_instant: GameInstant,
    client_sending_instant: GameInstant,
    server_receivable_instant: GameInstant,
}

impl TimeManager {
    pub fn from_parts(
        time_config: TimeConfig,
        base: BaseTimeManager,
        server_tick: Tick,
        server_tick_instant: GameInstant,
        server_tick_duration_avg: f32,
        server_tick_duration_min: f32,
        server_tick_duration_max: f32,
        pruned_rtt_avg: f32,
        rtt_stdv: f32,
        offset_stdv: f32,
    ) -> Self {
        let now = base.game_time_now();
        let latency_ms = (pruned_rtt_avg / 2.0) as u32;
        let major_jitter_ms = (rtt_stdv / 2.0 * 3.0) as u32;
        let tick_duration_ms = server_tick_duration_avg.round() as u32;

        let client_receiving_instant =
            get_client_receiving_target(&now, latency_ms, major_jitter_ms, tick_duration_ms);
        let client_sending_instant =
            get_client_sending_target(&now, latency_ms, major_jitter_ms, tick_duration_ms, 1.0);
        let server_receivable_instant =
            get_server_receivable_target(&now, latency_ms, major_jitter_ms, tick_duration_ms);

        let client_receiving_tick = instant_to_tick(&server_tick, &server_tick_instant, server_tick_duration_avg, &client_receiving_instant);
        let client_sending_tick = instant_to_tick(&server_tick, &server_tick_instant, server_tick_duration_avg, &client_sending_instant);
        let server_receivable_tick = instant_to_tick(&server_tick, &server_tick_instant, server_tick_duration_avg, &server_receivable_instant);

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

            server_tick,
            server_tick_instant,
            server_tick_duration_avg,
            server_tick_duration_min,
            server_tick_duration_max,
            danger_adjust: 1.0,

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
        if let Some((tick_duration_avg, tick_duration_min, tick_duration_max, offset_millis, rtt_millis)) = self.base.read_pong(reader)? {
            self.process_stats(offset_millis, rtt_millis);
            self.recv_tick_duration_avg(tick_duration_avg, tick_duration_min, tick_duration_max);
        }
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
            // info!("Ping: New Pruned Averages");
            //
            // info!(" ------- Incoming Offset: {offset_millis}, Incoming RTT: {rtt_millis}");
            let offset_avg = self.pruned_offset_avg;
            let rtt_avg = self.pruned_rtt_avg - self.initial_rtt_avg;
            info!(" ------- New Average Offset: {offset_avg}, Average RTT Offset: {rtt_avg}");
        } else {
            // info!("Ping: Pruned out Sample");
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

    pub(crate) fn recv_tick_instant(&mut self, server_tick: &Tick, server_tick_instant: &GameInstant) {

        // only continue if this tick is the most recent
        if !sequence_greater_than(*server_tick, self.server_tick) {
            // We've already received the most recent tick
            return;
        }

        let prev_server_tick_instant = self.tick_to_instant(*server_tick);
        let offset = prev_server_tick_instant.offset_from(&server_tick_instant);

        // {
        //     let server_tick_instant_ms = server_tick_instant.as_millis();
        //     let prev_server_tick_instant_ms = prev_server_tick_instant.as_millis();
        //     info!("Incoming    | Tick: {server_tick}: ({prev_server_tick_instant_ms}) -> ({server_tick_instant_ms}) | diff: {offset}");
        // }

        // // todo: remove
        // let prev_recv_instant = self.client_receiving_instant.as_millis();
        // let prev_send_instant = self.client_sending_instant.as_millis();
        // let prev_svrc_instant = self.server_receivable_instant.as_millis();
        // //

        self.server_tick = *server_tick;
        self.server_tick_instant = server_tick_instant.clone();

        // Adjust tick instants to new incoming instant
        self.client_receiving_instant = self.client_receiving_instant.add_signed_millis(offset);
        self.client_sending_instant = self.client_sending_instant.add_signed_millis(offset);
        self.server_receivable_instant = self.server_receivable_instant.add_signed_millis(offset);

        // {
        //     let recv_tick = self.client_receiving_tick;
        //     let recv_instant = self.client_receiving_instant.as_millis();
        //     let recv_diff = (recv_instant as i32)-(prev_recv_instant as i32);
        //
        //     let send_tick = self.client_sending_tick;
        //     let send_instant = self.client_sending_instant.as_millis();
        //     let send_diff = (send_instant as i32)-(prev_send_instant as i32);
        //
        //     let svrc_tick = self.server_receivable_tick;
        //     let svrc_instant = self.server_receivable_instant.as_millis();
        //     let svrc_diff = (svrc_instant as i32)-(prev_svrc_instant as i32);
        //
        //     info!("Client Recv | Tick: {recv_tick}, ({prev_recv_instant}) -> ({recv_instant}) | diff: {recv_diff}");
        //     info!("Client Send | Tick: {send_tick}, ({prev_send_instant}) -> ({send_instant}) | diff {send_diff}");
        //     info!("Server Recv | Tick: {svrc_tick}, ({prev_svrc_instant}) -> ({svrc_instant}) | diff {svrc_diff}");
        //     info!("...");
        // }
    }

    pub(crate) fn recv_tick_duration_avg(&mut self, server_tick_duration_avg: f32, server_tick_duration_min: f32, server_tick_duration_max: f32) {

        // let prev_sending_instant = self.client_sending_instant.clone();

        let client_receiving_interp = self.get_interp(self.client_receiving_tick, &self.client_receiving_instant);
        let client_sending_interp = self.get_interp(self.client_sending_tick, &self.client_sending_instant);
        let server_receivable_interp = self.get_interp(self.server_receivable_tick, &self.server_receivable_instant);

        // {
        //     let prev_server_tick_duration_avg = self.server_tick_duration_avg;
        //     let diff = server_tick_duration_avg - prev_server_tick_duration_avg;
        //     info!("Incoming    | Duration: ({prev_server_tick_duration_avg}) -> ({server_tick_duration_avg}) | diff: {diff}");
        // }

        // // todo: remove
        // let prev_recv_instant = self.client_receiving_instant.as_millis();
        // let prev_send_instant = self.client_sending_instant.as_millis();
        // let prev_svrc_instant = self.server_receivable_instant.as_millis();
        // //

        self.server_tick_duration_avg = server_tick_duration_avg;
        self.server_tick_duration_min = server_tick_duration_min;
        self.server_tick_duration_max = server_tick_duration_max;

        self.danger_adjust = (((self.server_tick_duration_max - self.server_tick_duration_min) / self.server_tick_duration_min) * 30.0).max(0.0).min(10.0);

        {
            let avg = self.server_tick_duration_avg;
            let min = self.server_tick_duration_min;
            let max = self.server_tick_duration_max;
            let danger = self.danger_adjust;
            info!(" ---------- Avg: {avg}, Min: {min}, Max: {max}, Danger: {danger}");
        }

        // if server_tick_duration_avg < self.server_tick_duration_avg {
        //     // Ticks are getting shorter, need to respond ASAP
        //
        //     //info!("XXXXXXXXXXX     Duration adjusted down!     XXXXXXXXXXXXXXXX");
        //     info!("Duration avg down to: {server_tick_duration_avg}");
        // } else {
        //     // Ticks are getting longer
        //     // This is tricky because if it drops again, it may throw our estimation off
        //     // and we may miss time-critical commands
        //     self.server_tick_duration_avg = (0.999 * self.server_tick_duration_avg) + (0.001 * server_tick_duration_avg);
        //     let server_tick_duration_avg_ms = self.server_tick_duration_avg;
        //     info!("Duration avg up to:   {server_tick_duration_avg_ms}");
        // }
        //self.server_tick_duration_avg = server_tick_duration_avg;

        // Adjust tick instants to new incoming instant
        self.client_receiving_instant = self.instant_from_interp(self.client_receiving_tick, client_receiving_interp);
        self.client_sending_instant = self.instant_from_interp(self.client_sending_tick, client_sending_interp);
        self.server_receivable_instant = self.instant_from_interp(self.server_receivable_tick, server_receivable_interp);

        // let sending_skew_distance = self.client_sending_instant.offset_from(&prev_sending_instant);
        // if sending_skew_distance > self.client_sending_instant_skew_adjust as i32 {
        //     self.client_sending_instant_skew_adjust = sending_skew_distance as u32;
        //     let adjust = self.client_sending_instant_skew_adjust;
        //     info!("Skew Adjust: {adjust} ms");
        // }

        // {
        //     let recv_tick = self.client_receiving_tick;
        //     let recv_instant = self.client_receiving_instant.as_millis();
        //     let recv_diff = (recv_instant as i32)-(prev_recv_instant as i32);
        //
        //     let send_tick = self.client_sending_tick;
        //     let send_instant = self.client_sending_instant.as_millis();
        //     let send_diff = (send_instant as i32)-(prev_send_instant as i32);
        //
        //     let svrc_tick = self.server_receivable_tick;
        //     let svrc_instant = self.server_receivable_instant.as_millis();
        //     let svrc_diff = (svrc_instant as i32)-(prev_svrc_instant as i32);
        //
        //     info!("Client Recv | Tick: {recv_tick}, ({prev_recv_instant}) -> ({recv_instant}) | diff: {recv_diff}");
        //     info!("Client Send | Tick: {send_tick}, ({prev_send_instant}) -> ({send_instant}) | diff {send_diff}");
        //     info!("Server Recv | Tick: {svrc_tick}, ({prev_svrc_instant}) -> ({svrc_instant}) | diff {svrc_diff}");
        //     info!("...");
        // }
    }

    pub(crate) fn check_ticks(&mut self) -> (Option<(Tick, Tick)>, Option<(Tick, Tick)>) {
        // updates client_receiving_tick
        // returns (Some(start_tick, end_tick), None) if a client_receiving_tick has incremented
        // returns (None, Some(start_tick, end_tick)) if a client_sending_tick or server_receivable_tick has incremented
        let prev_client_receiving_tick = self.client_receiving_tick;
        let prev_client_sending_tick = self.client_sending_tick;

        // todo: remove
        let prev_client_receiving_instant = self.client_receiving_instant.clone();
        let prev_client_sending_instant = self.client_sending_instant.clone();
        //

        {
            let time_elapsed = self.last_tick_check_instant.elapsed().as_secs_f32() * 1000.0;
            self.last_tick_check_instant = Instant::now();
            self.accumulator += time_elapsed;
            if self.accumulator < 1.0 {
                return (None, None);
            }
        }
        let millis_elapsed = self.accumulator.round() as u32;
        let millis_elapsed_f32 = millis_elapsed as f32;
        self.accumulator -= millis_elapsed_f32;

        // Target Instants
        let now: GameInstant = self.game_time_now();
        let latency_ms: u32 = self.latency().round() as u32;
        let major_jitter_ms: u32 = (self.jitter() * 3.0).round() as u32;

        let tick_duration_ms: u32 = self.server_tick_duration_avg.round() as u32;

        // find targets
        let client_receiving_target =
            get_client_receiving_target(&now, latency_ms, major_jitter_ms, tick_duration_ms);

        let client_sending_target =
            get_client_sending_target(&now, latency_ms, major_jitter_ms, tick_duration_ms, self.danger_adjust);
        let server_receivable_target =
            get_server_receivable_target(&now, latency_ms, major_jitter_ms, tick_duration_ms);

        // set default next instant
        let client_receiving_default_next = self.client_receiving_instant.add_millis(millis_elapsed);
        let client_sending_default_next = self.client_sending_instant.add_millis(millis_elapsed);
        let server_receivable_default_next = self.server_receivable_instant.add_millis(millis_elapsed);

        // find speeds
        let client_receiving_speed =
            offset_to_speed(client_receiving_default_next.offset_from(&client_receiving_target));
        let client_sending_speed =
            offset_to_speed(client_sending_default_next.offset_from(&client_sending_target));
        let server_receivable_speed =
            offset_to_speed(server_receivable_default_next.offset_from(&server_receivable_target));

        {
            let client_receiving_instant = client_receiving_default_next.as_millis();
            let client_sending_instant = client_sending_default_next.as_millis();
            let client_receiving_target_ms = client_receiving_target.as_millis();
            let client_sending_target_ms = client_sending_target.as_millis();
            //info!("elapsed: {millis_elapsed} ms");
            if client_receiving_speed != 1.0 {
                info!("RECV | INSTANT: {client_receiving_instant} -> TARGET: {client_receiving_target_ms} = SPEED: {client_receiving_speed}");
            }
            if client_sending_speed != 1.0 {
                info!("SEND | INSTANT: {client_sending_instant} -> TARGET: {client_sending_target_ms} = SPEED: {client_sending_speed}");
            }
        }

        // apply speeds
        self.client_receiving_instant = self
            .client_receiving_instant
            .add_millis((millis_elapsed_f32 * client_receiving_speed) as u32);
        if self.client_receiving_instant.is_more_than(&client_receiving_target) {
            self.client_receiving_instant = client_receiving_target;
        }
        self.client_sending_instant = self
            .client_sending_instant
            .add_millis((millis_elapsed_f32 * client_sending_speed) as u32);
        if self.client_sending_instant.is_more_than(&client_sending_target) {
            self.client_sending_instant = client_sending_target;
        }
        self.server_receivable_instant = self
            .server_receivable_instant
            .add_millis((millis_elapsed_f32 * server_receivable_speed) as u32);
        if self.server_receivable_instant.is_more_than(&server_receivable_target) {
            self.server_receivable_instant = server_receivable_target;
        }

        // convert current instants into ticks
        let new_client_receiving_tick = instant_to_tick(&self.server_tick, &self.server_tick_instant, self.server_tick_duration_avg, &self.client_receiving_instant);
        let new_client_sending_tick = instant_to_tick(&self.server_tick, &self.server_tick_instant, self.server_tick_duration_avg, &self.client_sending_instant);
        let new_server_receivable_tick = instant_to_tick(&self.server_tick, &self.server_tick_instant, self.server_tick_duration_avg, &self.server_receivable_instant);

        // make sure nothing ticks backwards
        if sequence_less_than(new_client_receiving_tick, self.client_receiving_tick) {
            warn!("Client Receiving Tick attempted to Tick Backwards");
        } else {
            self.client_receiving_tick = new_client_receiving_tick;
        }
        if sequence_less_than(new_client_sending_tick, self.client_sending_tick) {
            warn!("Client Sending Tick attempted to Tick Backwards");
        } else {
            self.client_sending_tick = new_client_sending_tick;
        }
        if sequence_less_than(new_server_receivable_tick, self.server_receivable_tick) {
            warn!("Server Receivable Tick attempted to Tick Backwards");
        } else {
            self.server_receivable_tick = new_server_receivable_tick;
        }

        let receiving_incremented = self.client_receiving_tick != prev_client_receiving_tick;
        let sending_incremented = self.client_sending_tick != prev_client_sending_tick;

        // if receiving_incremented {
        //     let a = self.client_receiving_tick;
        //     let b = prev_client_receiving_instant.as_millis();
        //     let c = self.client_receiving_instant.as_millis();
        //     info!("RECV | Tick: {prev_client_receiving_tick} -> {a}, Instant: {b} -> {c}");
        // }
        // if sending_incremented {
        //     let a = self.client_sending_tick;
        //     let b = prev_client_sending_instant.as_millis();
        //     let c = self.client_sending_instant.as_millis();
        //     info!("SEND | Tick: {prev_client_sending_tick} -> {a}, Instant: {b} -> {c}");
        // }

        let output_receiving = match receiving_incremented {
            true => Some((prev_client_receiving_tick, self.client_receiving_tick)),
            false => None,
        };
        let output_sending = match sending_incremented {
            true => Some((prev_client_sending_tick, self.client_sending_tick)),
            false => None,
        };

        return (output_receiving, output_sending);
    }

    // Stats

    pub(crate) fn interpolation(&self) -> f32 {
        todo!()
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

    pub(crate) fn tick_to_instant(&self, tick: Tick) -> GameInstant {
        let tick_diff = wrapping_diff(self.server_tick, tick);
        let tick_diff_duration = ((tick_diff as f32) * self.server_tick_duration_avg).round() as i32;
        return self.server_tick_instant.add_signed_millis(tick_diff_duration);
    }

    pub(crate) fn get_interp(&self, tick: Tick, instant: &GameInstant) -> f32 {
        let output = (self.tick_to_instant(tick).offset_from(&instant) as f32) / self.server_tick_duration_avg;
        output
    }

    pub(crate) fn instant_from_interp(&self, tick: Tick, interp: f32) -> GameInstant {
        let tick_length_interped = (interp * self.server_tick_duration_avg).round() as i32;
        return self.tick_to_instant(tick).add_signed_millis(tick_length_interped);
    }
}

fn instant_to_tick(
    server_tick: &Tick,
    server_tick_instant: &GameInstant,
    server_tick_duration_avg: f32,
    instant: &GameInstant
) -> Tick {
    let offset_ms = server_tick_instant.offset_from(instant);
    let offset_ticks_f32 = (offset_ms as f32) / server_tick_duration_avg;
    return server_tick
        .clone()
        .wrapping_add_signed(offset_ticks_f32 as i16);
}

fn get_client_receiving_target(
    now: &GameInstant,
    latency: u32,
    jitter: u32,
    tick_duration: u32,
) -> GameInstant {
    now.sub_millis(latency + jitter + tick_duration)
}

fn get_client_sending_target(
    now: &GameInstant,
    latency: u32,
    jitter: u32,
    tick_duration: u32,
    danger: f32,
) -> GameInstant {
    let millis = latency + jitter + (tick_duration * 4) + (tick_duration as f32 * danger).round() as u32;
    now.add_millis(millis)
}

fn get_server_receivable_target(now: &GameInstant, latency: u32, jitter: u32, tick_duration: u32) -> GameInstant {
    let millis = (((latency + (tick_duration * 2)) as i32) - (jitter as i32)).max(0) as u32;
    now.add_millis(millis)
}

fn offset_to_speed(mut offset: i32) -> f32 {
    if offset <= OFFSET_MIN {
        let under = (OFFSET_MIN - offset) as f32;
        return (RANGE_MIN / (under + RANGE_MIN)).max(SPEED_MIN);
    }

    if offset >= OFFSET_MAX {
        let over = (offset - OFFSET_MAX) as f32;
        return (1.0 + (over / RANGE_MAX)).min(SPEED_MAX);
    }

    return SAFE_SPEED;
}

const OFFSET_MIN: i32 = -50;
const OFFSET_MAX: i32 = 50;
const SAFE_SPEED: f32 = 1.0;
const RANGE_MAX: f32 = 20.0;
const RANGE_MIN: f32 = 20.0;
const SPEED_MAX: f32 = 10.0;
const SPEED_MIN: f32 = 1.0 / SPEED_MAX;

const INV_OFFSET_MIN: i32 = OFFSET_MIN * -1;

// Tests
#[cfg(test)]
mod offset_to_speed_tests {
    use crate::connection::time_manager::{offset_to_speed, OFFSET_MAX, OFFSET_MIN, SAFE_SPEED};

    #[test]
    fn min_speed() {
        assert_eq!(offset_to_speed(OFFSET_MIN), SAFE_SPEED);
    }

    #[test]
    fn max_speed() {
        assert_eq!(offset_to_speed(OFFSET_MAX), SAFE_SPEED);
    }

    #[test]
    fn middle_speed() {
        let middle_offset = ((OFFSET_MAX - OFFSET_MIN) / 2) + OFFSET_MIN;
        assert_eq!(offset_to_speed(middle_offset), SAFE_SPEED);
    }

    #[test]
    fn over_max_speed() {
        let offset = OFFSET_MAX + 5;

        // TODO: derive these values?
        assert_eq!(offset_to_speed(offset), 1.5);
    }

    #[test]
    fn under_max_speed() {
        let offset = OFFSET_MIN - 5;

        // TODO: derive these values?
        assert_eq!(offset_to_speed(offset), 0.6666667);
    }
}
