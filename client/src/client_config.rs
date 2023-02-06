use std::{default::Default, time::Duration};

use crate::tick::tick_manager::TickManagerConfig;
use naia_shared::ConnectionConfig;

/// Contains Config properties which will be used by a Server or Client
#[derive(Clone)]
pub struct ClientConfig {
    /// Used to configure the connection with the Server
    pub connection: ConnectionConfig,
    /// The duration between the resend of certain connection handshake messages
    pub send_handshake_interval: Duration,
    /// Configuration for options related to the Tick Manager,
    pub tick_manager_config: Option<TickManagerConfig>,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            connection: ConnectionConfig::default(),
            send_handshake_interval: Duration::from_millis(250),
            tick_manager_config: None,
        }
    }
}
