use std::{
    default::Default,
    net::{SocketAddr, TcpListener, UdpSocket},
    time::{Duration, Instant},
};

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
        Self {
            session_listen_addr,
            webrtc_listen_addr,
            public_webrtc_url: public_webrtc_url.to_string(),
        }
    }
}

impl ServerAddrs {
    /// Block until both listen ports are free (rebindable) or `timeout`
    /// elapses. Returns true iff both were observed free.
    ///
    /// This is the observable half of shutdown (naia-lib/naia#92):
    /// dropping the listen handles ends the detached tasks, but that exit
    /// is asynchronous — a bare drop cannot promise *when* the ports come
    /// back. Poll `wait_until_free` (or use [`Socket::close`](crate::Socket::close),
    /// which drops and waits) and rebind only on true: no guessed sleeps.
    pub fn wait_until_free(&self, timeout: Duration) -> bool {
        let start = Instant::now();
        while start.elapsed() < timeout {
            if self.ports_free() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        self.ports_free()
    }

    fn ports_free(&self) -> bool {
        TcpListener::bind(self.session_listen_addr).is_ok()
            && UdpSocket::bind(self.webrtc_listen_addr).is_ok()
    }
}

impl Default for ServerAddrs {
    fn default() -> Self {
        Self::new(
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
