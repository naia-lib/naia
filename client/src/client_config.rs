use std::{default::Default, time::Duration};

use naia_shared::{ConnectionConfig, SocketConfig};

/// Contains Config properties which will be used by a Server or Client
#[derive(Clone)]
pub struct ClientConfig {
    /// Used to configure the Server's underlying socket
    pub socket: SocketConfig,
    /// Used to configure the connection with the Server
    pub connection: ConnectionConfig,
    /// The duration between the resend of certain connection handshake messages
    pub send_handshake_interval: Duration,
    /// The minimum of measured latency to the Server that the Client use to
    /// ensure packets arrive in time. Should be fine if this is 0,
    /// but you'll increase the chance that packets always arrive to be
    /// processed by the Server with a higher number. This is especially
    /// helpful early on in the connection, when estimates of latency are
    /// less accurate.
    pub minimum_latency: Option<Duration>,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            socket: SocketConfig::default(),
            connection: ConnectionConfig::default(),
            send_handshake_interval: Duration::from_secs(1),
            minimum_latency: None,
        }
    }
}
