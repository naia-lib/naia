use std::sync::{Arc, Mutex};

use local_transport_shared::LocalTransportHub;

use super::auth::{LocalServerAuthReceiver, LocalServerAuthSender, ServerAuthIo};
use super::data::{LocalServerReceiver, LocalServerSender};

pub struct LocalServerSocket {
    hub: LocalTransportHub,
}

impl LocalServerSocket {
    pub fn new(hub: LocalTransportHub) -> Self {
        Self { hub }
    }

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
}

