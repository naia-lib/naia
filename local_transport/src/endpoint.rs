use std::sync::{Arc, Mutex};

use crate::hub::LocalTransportHub;
use crate::client::LocalClientSocket;
use crate::server::{LocalServerAuthReceiver, LocalServerAuthSender, LocalServerReceiver, LocalServerSender, LocalServerSocket, ServerAuthIo};

/// Server endpoint that manages multiple client connections via the hub
pub struct LocalServerEndpoint {
    hub: LocalTransportHub,
}

impl LocalServerEndpoint {
    pub(crate) fn new(hub: LocalTransportHub) -> Self {
        Self { hub }
    }

    /// Get the transport handles for NaiaServer (same API as LocalServerSocket::listen_with_auth)
    pub fn listen_with_auth(
        self,
    ) -> (
        LocalServerAuthSender,
        LocalServerAuthReceiver,
        LocalServerSender,
        LocalServerReceiver,
    ) {
        let hub = self.hub;
        
        let auth_io = Arc::new(Mutex::new(ServerAuthIo::new(hub.clone())));
        let auth_sender = LocalServerAuthSender::new(auth_io.clone());
        let auth_receiver = LocalServerAuthReceiver::new(auth_io);
        
        let sender = LocalServerSender::new(hub.clone());
        let receiver = LocalServerReceiver::new(hub);
        
        (auth_sender, auth_receiver, sender, receiver)
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
    pub(crate) fn new(socket: LocalClientSocket) -> Self {
        Self { socket }
    }

    /// Get the client socket (same API as LocalClientSocket)
    pub fn into_socket(self) -> LocalClientSocket {
        self.socket
    }
}

