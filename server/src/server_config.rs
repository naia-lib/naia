use std::{default::Default, net::SocketAddr, time::Duration};

/// A collection of IP addresses describing which IP to listen on for new
/// sessions, which to dedicate to UDP traffic, and which to advertise publicly
#[derive(Clone, Debug)]
pub struct ServerAddresses {
    /// IP Address to listen on for the signaling portion of WebRTC
    pub session_listen_addr: SocketAddr,
    /// IP Address to listen on for UDP WebRTC data channels
    pub webrtc_listen_addr: SocketAddr,
    /// The public WebRTC IP address to advertise
    pub public_webrtc_addr: SocketAddr,
}

impl ServerAddresses {
    /// Create a new ServerAddresses config struct from component addresses
    pub fn new(
        session_listen_addr: SocketAddr,
        webrtc_listen_addr: SocketAddr,
        public_webrtc_addr: SocketAddr,
    ) -> Self {
        ServerAddresses {
            session_listen_addr,
            webrtc_listen_addr,
            public_webrtc_addr,
        }
    }
}

/// Contains Config properties which will be used by a Server or Client
#[derive(Clone, Debug)]
pub struct ServerConfig {
    /// Socket addresses to bind to for various server functions
    pub socket_addresses: ServerAddresses,
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

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            socket_addresses: ServerAddresses::new(
                "127.0.0.1:14191"
                    .parse()
                    .expect("could not parse HTTP address/port"),
                "127.0.0.1:14192"
                    .parse()
                    .expect("could not parse WebRTC data address/port"),
                "127.0.0.1:14192"
                    .parse()
                    .expect("could not parse advertised public WebRTC data address/port"),
            ),
            disconnection_timeout_duration: Duration::from_secs(10),
            heartbeat_interval: Duration::from_secs(4),
            send_handshake_interval: Duration::from_secs(1),
            ping_interval: Duration::from_secs(1),
            rtt_sample_size: 20,
        }
    }
}
