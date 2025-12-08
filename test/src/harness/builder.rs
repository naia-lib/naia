use std::{sync::{Arc, Mutex}, net::SocketAddr};

use naia_client::transport::local::{LocalAddrCell, LocalClientSocket};
use naia_server::transport::local::LocalServerSocket;
use naia_shared::transport::local::{LocalTransportHub, FAKE_SERVER_ADDR};

/// Builder for creating local transport endpoints
pub struct LocalTransportBuilder {
    hub: LocalTransportHub,
}

impl LocalTransportBuilder {
    /// Create a new builder with a shared transport hub
    pub fn new() -> Self {
        let server_addr: SocketAddr = FAKE_SERVER_ADDR.parse().expect("invalid server addr");
        let hub = LocalTransportHub::new(server_addr);
        Self { hub }
    }

    /// Get the server endpoint
    pub fn server_endpoint(&self) -> LocalServerSocket {
        LocalServerSocket::new(self.hub.clone())
    }

    /// Create a new client endpoint and register it with the hub
    pub fn connect_client(&self) -> LocalClientSocket {
        let (client_addr, auth_req_tx, auth_resp_rx, client_data_tx, client_data_rx) = 
            self.hub.register_client();
        
        let addr_cell = LocalAddrCell::new();
        // For local transport, we know the server address immediately
        addr_cell.set_sync(self.hub.server_addr());

        // Each client gets its own identity token storage (not shared!)
        let identity_token = Arc::new(Mutex::new(None));
        let rejection_code = Arc::new(Mutex::new(None));

        LocalClientSocket::new_with_tokens(
            client_addr,
            self.hub.server_addr(),
            auth_req_tx,
            auth_resp_rx,
            client_data_tx,
            client_data_rx,
            addr_cell,
            identity_token,
            rejection_code,
        )
    }
}

impl Default for LocalTransportBuilder {
    fn default() -> Self {
        Self::new()
    }
}

