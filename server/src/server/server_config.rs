use std::default::Default;

use naia_shared::ConnectionConfig;

use crate::connection::ping_config::PingConfig;

/// Contains Config properties which will be used by the Server
#[derive(Clone)]
pub struct ServerConfig {
    /// Determines whether to require that the Client send some auth message
    /// in order to connect.
    pub require_auth: bool,
    /// Used to configure the connections with Clients
    pub connection: ConnectionConfig,
    /// Configuration used to monitor the ping & jitter on the network
    pub ping: PingConfig,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            require_auth: true,
            connection: ConnectionConfig::default(),
            ping: PingConfig::default(),
        }
    }
}
