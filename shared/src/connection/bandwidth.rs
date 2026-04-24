/// Per-connection outbound bandwidth budget. Applied symmetrically to
/// server-outbound and client-outbound send loops.
///
/// Not to be confused with `ConnectionConfig::bandwidth_measure_duration`,
/// which is a telemetry/averaging window — this is the actual token-bucket cap
/// consumed by the unified priority-sort send loop.
#[derive(Clone, Debug)]
pub struct BandwidthConfig {
    /// Target outbound bytes-per-second per connection. Budget accumulates as
    /// `target_bytes_per_sec × dt` each tick; surplus carries into the next
    /// tick (Fiedler token-bucket).
    pub target_bytes_per_sec: u32,
}

impl BandwidthConfig {
    /// 512 kbps — generous default; overridable.
    pub const DEFAULT_TARGET_BYTES_PER_SEC: u32 = 64_000;
}

impl Default for BandwidthConfig {
    fn default() -> Self {
        Self {
            target_bytes_per_sec: Self::DEFAULT_TARGET_BYTES_PER_SEC,
        }
    }
}
