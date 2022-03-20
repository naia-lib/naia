use std::{default::Default, net::SocketAddr};

/// List of addresses needed to start listening on a ServerSocket
#[derive(Clone)]
pub struct ServerAddrs {
    /// IP Address to listen on for the signaling portion of WebRTC
    pub session_listen_addr: SocketAddr,
    /// IP Address to listen on for UDP WebRTC data channels
    pub webrtc_listen_addr: SocketAddr,
    /// The public WebRTC IP address to advertise
    pub public_webrtc_url: String,
}

impl ServerAddrs {
    /// Create a new ServerSocketAddrs instance which will be used to start
    /// listening on a ServerSocket
    pub fn new(
        session_listen_addr: SocketAddr,
        webrtc_listen_addr: SocketAddr,
        public_webrtc_url: &str,
    ) -> Self {
        ServerAddrs {
            session_listen_addr,
            webrtc_listen_addr,
            public_webrtc_url: public_webrtc_url.to_string(),
        }
    }
}

impl Default for ServerAddrs {
    fn default() -> Self {
        ServerAddrs::new(
            "127.0.0.1:14191"
                .parse()
                .expect("could not parse HTTP address/port"),
            "127.0.0.1:14192"
                .parse()
                .expect("could not parse WebRTC data address/port"),
            "http://127.0.0.1:14192",
        )
    }
}
