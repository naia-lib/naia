use std::{default::Default, time::Duration};

use naia_shared::SocketConfig;

/// Contains Config properties which will be used by the Server
#[derive(Clone)]
pub struct ServerConfig {
    /// Used to configure the Server's underlying socket
    pub socket_config: SocketConfig,
    /// The duration between the resend of certain connection handshake messages
    pub send_handshake_interval: Duration,
    /// The duration to wait for communication from a remote host before
    /// initiating a disconnect
    // Keep in mind that the disconnect timeout duration should always be at least
    // 2x greater than the remote host's heartbeat interval, to make it so that at the
    // worst case, the remote host would need to miss 2 server heartbeats before
    // triggering a disconnection
    pub disconnection_timeout_duration: Duration,
    /// The duration to wait before sending a heartbeat message to a remote
    /// host, if the host has not already sent another message within that time.
    pub heartbeat_interval: Duration,
    /// The duration to wait before sending a ping message to the remote host,
    /// in order to estimate RTT time
    pub ping_interval: Duration,
    /// Number of samples to measure RTT & Jitter by. A higher number will
    /// smooth out RTT measurements, but at the cost of responsiveness.
    pub rtt_sample_size: u16,
    /// Determines whether to require that the Client send some auth message
    /// in order to connect.
    pub require_auth: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            socket_config: SocketConfig::default(),
            disconnection_timeout_duration: Duration::from_secs(10),
            heartbeat_interval: Duration::from_secs(3),
            send_handshake_interval: Duration::from_secs(1),
            ping_interval: Duration::from_secs(1),
            rtt_sample_size: 20,
            require_auth: true,
        }
    }
}
