use std::{default::Default, time::Duration};

use naia_shared::ConnectionConfig;

/// Contains Config properties which will be used by a Server or Client
#[derive(Clone)]
pub struct ClientConfig {
    /// Used to configure the connection with the Server
    pub connection: ConnectionConfig,
    /// The duration between the resend of certain connection handshake messages
    pub send_handshake_interval: Duration,
    /// The duration to wait before sending a ping message to the remote host,
    /// in order to estimate RTT time
    pub ping_interval: Duration,
    /// The number of network samples to take before completing the Connection Handshake.
    /// Increase this for greater accuracy of network statistics, at the cost of the handshake
    /// taking longer. Keep in mind that the network measurements affect how likely commands
    /// are able to arrive at the server before processing.
    pub handshake_pings: u8,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            connection: ConnectionConfig::default(),
            send_handshake_interval: Duration::from_millis(250),
            ping_interval: Duration::from_secs(1),
            handshake_pings: 10,
        }
    }
}
