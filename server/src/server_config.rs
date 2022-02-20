use std::default::Default;

use naia_shared::{ConnectionConfig, SocketConfig};

/// Contains Config properties which will be used by the Server
#[derive(Clone)]
pub struct ServerConfig {
    /// Used to configure the Server's underlying socket
    pub socket: SocketConfig,
    /// Used to configure the connections with Clients
    pub connection: ConnectionConfig,
    /// Determines whether to require that the Client send some auth message
    /// in order to connect.
    pub require_auth: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            socket: SocketConfig::default(),
            connection: ConnectionConfig::default(),
            require_auth: true,
        }
    }
}
