use std::{default::Default, time::Duration};

use crate::connection::bandwidth::BandwidthConfig;

/// Contains Config properties which will be used by a Server or Client
#[derive(Clone, Debug)]
pub struct ConnectionConfig {
    /// The duration to wait for communication from a remote host before
    /// initiating a disconnect
    pub disconnection_timeout_duration: Duration,
    /// The duration to wait before sending a heartbeat message to a remote
    /// host, if the host has not already sent another message within that time
    pub heartbeat_interval: Duration,
    /// The duration over which to measure bandwidth. Set to None to avoid
    /// measure bandwidth at all. This is a telemetry/averaging window — NOT
    /// the outbound cap; for that, see `bandwidth`.
    pub bandwidth_measure_duration: Option<Duration>,
    /// Outbound bandwidth budget (token-bucket cap) applied by the unified
    /// priority-sort send loop. Distinct from `bandwidth_measure_duration`.
    pub bandwidth: BandwidthConfig,
}

impl ConnectionConfig {
    /// Creates a new ConnectionConfig, used to initialize a Connection.
    /// Uses default `BandwidthConfig`; set the `.bandwidth` field directly
    /// after construction if a non-default budget is required.
    pub fn new(
        disconnection_timeout_duration: Duration,
        heartbeat_interval: Duration,
        bandwidth_measure_duration: Option<Duration>,
    ) -> Self {
        Self {
            disconnection_timeout_duration,
            heartbeat_interval,
            bandwidth_measure_duration,
            bandwidth: BandwidthConfig::default(),
        }
    }
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            disconnection_timeout_duration: Duration::from_secs(30),
            heartbeat_interval: Duration::from_secs(4),
            bandwidth_measure_duration: None,
            bandwidth: BandwidthConfig::default(),
        }
    }
}
