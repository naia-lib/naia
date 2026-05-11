/// Snapshot of per-connection network diagnostics.
///
/// All fields are rolling averages or short-window estimates; they are
/// computed on demand (poll-style) and incur no per-tick allocation.
/// Obtain via `Server::connection_stats(&user_key)` or `Client::connection_stats()`.
#[derive(Clone, Debug)]
pub struct ConnectionStats {
    /// Round-trip time in milliseconds (EWMA).
    pub rtt_ms: f32,
    /// RTT 50th-percentile in milliseconds, estimated from the last 32 samples.
    pub rtt_p50_ms: f32,
    /// RTT 99th-percentile in milliseconds, estimated from the last 32 samples.
    pub rtt_p99_ms: f32,
    /// Jitter in milliseconds (EWMA of half the absolute RTT deviation).
    pub jitter_ms: f32,
    /// Fraction of sent data-packets that were not acknowledged in the last
    /// 64-packet window. Range: 0.0 (no loss) – 1.0 (total loss).
    pub packet_loss_pct: f32,
    /// Rolling-average outgoing bandwidth in kilobits per second.
    pub kbps_sent: f32,
    /// Rolling-average incoming bandwidth in kilobits per second.
    pub kbps_recv: f32,
}
