use std::{default::Default, time::Duration};

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
    /// How long to wait for the application to call `accept_connection` or
    /// `reject_connection` after the network handshake completes.
    ///
    /// If neither is called within this window the connection is auto-rejected.
    /// This prevents unauthenticated clients from holding server memory
    /// indefinitely. Default: 10 seconds.
    pub pending_auth_timeout: Duration,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            require_auth: true,
            connection: ConnectionConfig::default(),
            ping: PingConfig::default(),
            pending_auth_timeout: Duration::from_secs(10),
        }
    }
}
