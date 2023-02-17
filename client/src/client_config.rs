use std::{default::Default, time::Duration};

use crate::connection::ping_config::PingConfig;
use naia_shared::ConnectionConfig;

use crate::TickConfig;

/// Contains Config properties which will be used by a Server or Client
#[derive(Clone)]
pub struct ClientConfig {
    /// Used to configure the connection with the Server
    pub connection: ConnectionConfig,
    /// The duration between the resend of certain connection handshake messages
    pub send_handshake_interval: Duration,
    /// Configuration for options related to Tick syncing function
    pub tick_config: TickConfig,
    /// Configuration used to monitor the ping & jitter on the network
    pub ping_config: PingConfig,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            connection: ConnectionConfig::default(),
            send_handshake_interval: Duration::from_millis(250),
            tick_config: TickConfig::default(),
            ping_config: PingConfig::default(),
        }
    }
}
