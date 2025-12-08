
use naia_server::transport::local::LocalServerSocket;
use naia_client::transport::local::LocalClientSocket;
use naia_shared::transport::local::LocalTransportHub;

/// Server endpoint that manages multiple client connections via the hub
pub struct LocalServerEndpoint {
    hub: LocalTransportHub,
}

impl LocalServerEndpoint {
    pub fn new(hub: LocalTransportHub) -> Self {
        Self { hub }
    }

    /// Convert to LocalServerSocket (for backwards compatibility with test helpers)
    pub fn into_socket(self) -> LocalServerSocket {
        LocalServerSocket::new(self.hub)
    }
}

/// Client endpoint representing a single client connection
pub struct LocalClientEndpoint {
    socket: LocalClientSocket,
}

impl LocalClientEndpoint {
    pub fn new(socket: LocalClientSocket) -> Self {
        Self { socket }
    }

    /// Get the client socket (same API as LocalClientSocket)
    pub fn into_socket(self) -> LocalClientSocket {
        self.socket
    }
}

