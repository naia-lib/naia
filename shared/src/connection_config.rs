use std::{default::Default, time::Duration};

/// Contains Config properties which will be used by a Server or Client
#[derive(Clone, Debug)]
pub struct ConnectionConfig {
    /// The duration to wait for communication from a remote host before
    /// initiating a disconnect
    pub disconnection_timeout_duration: Duration,
    /// The duration to wait before sending a heartbeat message to a remote
    /// host, if the host has not already sent another message within that time.
    pub heartbeat_interval: Duration,
    /// The duration to wait before sending a ping message to the remote host,
    /// in order to estimate RTT time
    pub ping_interval: Duration,
    /// Number of samples to measure RTT & Jitter by. A higher number will
    /// smooth out RTT measurements, but at the cost of responsiveness.
    pub rtt_sample_size: u16,
}

impl ConnectionConfig {
    /// Creates a new ConnectionConfig, used to initialize a Connection
    pub fn new(
        disconnection_timeout_duration: Duration,
        heartbeat_interval: Duration,
        ping_interval: Duration,
        rtt_sample_size: u16,
    ) -> Self {
        ConnectionConfig {
            disconnection_timeout_duration,
            heartbeat_interval,
            ping_interval,
            rtt_sample_size,
        }
    }
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            disconnection_timeout_duration: Duration::from_secs(10),
            heartbeat_interval: Duration::from_secs(4),
            ping_interval: Duration::from_secs(1),
            rtt_sample_size: 20,
        }
    }
}
