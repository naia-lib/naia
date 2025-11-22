use std::{net::SocketAddr, sync::{Arc, Mutex}};

use tokio::sync::mpsc;

use crate::shared::LocalTransportQueues;
use super::auth::{LocalServerAuthReceiver, LocalServerAuthSender, ServerAuthIo};
use super::data::{LocalServerReceiver, LocalServerSender};

pub struct LocalServerSocket {
    auth_io: Arc<Mutex<ServerAuthIo>>,
    sender: LocalServerSender,
    receiver: LocalServerReceiver,
}

impl LocalServerSocket {
    pub(crate) fn new(
        shared: LocalTransportQueues,
        client_addr: SocketAddr,
        _server_addr: SocketAddr,
        auth_requests_rx: mpsc::UnboundedReceiver<Vec<u8>>,
        auth_responses_tx: mpsc::UnboundedSender<Vec<u8>>,
        data_tx: mpsc::UnboundedSender<Vec<u8>>,
        data_rx: mpsc::UnboundedReceiver<Vec<u8>>,
    ) -> Self {
        let auth_io = Arc::new(Mutex::new(ServerAuthIo::new(
            Arc::new(Mutex::new(auth_requests_rx)),
            auth_responses_tx,
            shared.server_data_addr,
        )));

        Self {
            auth_io,
            sender: LocalServerSender::new(data_tx, client_addr),
            receiver: LocalServerReceiver::new(data_rx, client_addr),
        }
    }

    pub fn listen_with_auth(
        self,
    ) -> (
        LocalServerAuthSender,
        LocalServerAuthReceiver,
        LocalServerSender,
        LocalServerReceiver,
    ) {
        let LocalServerSocket {
            auth_io,
            sender,
            receiver,
        } = self;
        
        let auth_sender = LocalServerAuthSender::new(auth_io.clone());
        let auth_receiver = LocalServerAuthReceiver::new(auth_io);
        
        (auth_sender, auth_receiver, sender, receiver)
    }
}

