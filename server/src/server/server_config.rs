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
    /// Maximum number of concurrent users that have sent an auth request but
    /// have not yet completed the handshake.
    ///
    /// Every inbound auth request allocates a user record before anything about
    /// the sender has been verified, so without a ceiling a source-address flood
    /// grows server memory at the attacker's packet rate for the whole
    /// `pending_auth_timeout` window. Once this many pending users are
    /// outstanding, further auth requests are rejected at the door until the
    /// backlog drains. Default: 1,024 — the same ceiling the handshake manager
    /// already applies to pre-auth connection state.
    pub max_pending_auth_users: usize,
    /// Maximum number of replicated entities. Determines the pre-allocated capacity of
    /// `GlobalDirtyBitset`. Default: 65,536. Must be increased if entity count exceeds this.
    pub max_replicated_entities: u32,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            require_auth: true,
            connection: ConnectionConfig::default(),
            ping: PingConfig::default(),
            pending_auth_timeout: Duration::from_secs(10),
            max_pending_auth_users: 1_024,
            max_replicated_entities: 65_536,
        }
    }
}
