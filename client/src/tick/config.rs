#[derive(Copy, Clone)]
pub struct TickConfig {
    /// The minimum of measured latency to the Server that the Client use to
    /// ensure packets arrive in time. Should be fine if this is 0,
    /// but you'll increase the chance that packets always arrive to be
    /// processed by the Server with a higher number. This is especially
    /// helpful early on in the connection, when estimates of latency are
    /// less accurate.
    pub minimum_latency_millis: f32,
    /// Minimum size of the jitter buffer for packets received from the server. In ticks.
    pub minimum_recv_jitter_buffer_size: u8,
    /// Minimum size of the jitter buffer for packets sent to the server. In ticks.
    pub minimum_send_jitter_buffer_size: u8,
    /// Offset to use to compute the tick_offset_avg and tick_offset_speed_avg. Lower means more smoothing
    pub tick_offset_smooth_factor: f32,
}

impl Default for TickConfig {
    fn default() -> Self {
        Self {
            minimum_latency_millis: 0.0,
            minimum_recv_jitter_buffer_size: 1,
            minimum_send_jitter_buffer_size: 10,
            tick_offset_smooth_factor: 0.10,
        }
    }
}
