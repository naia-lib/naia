use std::{default::Default, time::Duration};

use naia_shared::ConnectionConfig;

use crate::connection::time_config::TimeConfig;

/// Contains Config properties which will be used by a Server or Client
#[derive(Clone)]
pub struct ClientConfig {
    /// Used to configure the connection with the Server
    pub connection: ConnectionConfig,
    /// The duration between the resend of certain connection handshake messages
    pub send_handshake_interval: Duration,
    /// Configuration used to monitor the ping & jitter on the network
    pub time: TimeConfig,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            connection: ConnectionConfig::default(),
            send_handshake_interval: Duration::from_millis(250),
            time: TimeConfig::default(),
        }
    }
}
