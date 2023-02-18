use std::{default::Default, time::Duration};

/// Contains Config properties which will be used by a Server or Client
#[derive(Clone, Debug)]
pub struct TimeConfig {
    /// The duration to wait before sending a ping message to the remote host,
    /// in order to estimate RTT time
    pub ping_interval: Duration,
    /// The initial estimate for the RTT
    pub rtt_initial_estimate: Duration,
    /// The initial estimate for Jitter
    pub jitter_initial_estimate: Duration,
}

impl TimeConfig {
    /// Creates a new MonitorConfig, used to monitor statistics about the
    /// network
    pub fn new(
        ping_interval: Duration,
        rtt_initial_estimate: Duration,
        jitter_initial_estimate: Duration,
    ) -> Self {
        TimeConfig {
            ping_interval,
            rtt_initial_estimate,
            jitter_initial_estimate,
        }
    }
}

impl Default for TimeConfig {
    fn default() -> Self {
        Self {
            ping_interval: Duration::from_secs(1),
            rtt_initial_estimate: Duration::from_millis(200),
            jitter_initial_estimate: Duration::from_millis(20),
        }
    }
}
