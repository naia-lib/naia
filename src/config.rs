use std::{default::Default, time::Duration};

#[derive(Clone, Debug)]

pub struct Config {
    pub tick_interval: Duration,
    pub send_handshake_interval: Duration,
    pub disconnection_timeout_duration: Duration,
    pub heartbeat_interval: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            tick_interval: Duration::from_secs(1),
            disconnection_timeout_duration: Duration::from_secs(10),
            heartbeat_interval: Duration::from_secs(4),
            send_handshake_interval: Duration::from_secs(1),
        }
    }
}