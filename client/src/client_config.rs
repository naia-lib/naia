use std::{default::Default, time::Duration, net::SocketAddr};

/// Contains Config properties which will be used by a Server or Client
#[derive(Clone, Debug)]
pub struct ClientConfig {
    /// Socket address of the Server
    pub server_address: SocketAddr,
    /// The duration between the resend of certain connection handshake messages
    pub send_handshake_interval: Duration,
    /// The duration to wait for communication from a remote host before
    /// initiating a disconnect
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
}

impl Default for ClientConfig {
    fn default() -> Self {
        let server_address: SocketAddr = "127.0.0.1:14191"
            .parse()
            .expect("couldn't parse input IP address");
        Self {
            server_address,
            disconnection_timeout_duration: Duration::from_secs(10),
            heartbeat_interval: Duration::from_secs(4),
            send_handshake_interval: Duration::from_secs(1),
            ping_interval: Duration::from_secs(1),
            rtt_sample_size: 20,
        }
    }
}
