use std::{sync::{Arc, Mutex}, net::SocketAddr};

use local_transport_client::{LocalAddrCell, LocalClientSocket};
use local_transport_shared::{LocalTransportHub, FAKE_SERVER_ADDR};

use crate::{LocalClientEndpoint, LocalServerEndpoint};

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
    pub fn server_endpoint(&self) -> LocalServerEndpoint {
        LocalServerEndpoint::new(self.hub.clone())
    }

    /// Create a new client endpoint and register it with the hub
    pub fn connect_client(&self) -> LocalClientEndpoint {
        let (client_addr, auth_req_tx, auth_resp_rx, client_data_tx, client_data_rx) = 
            self.hub.register_client();
        
        let addr_cell = LocalAddrCell::new();
        // For local transport, we know the server address immediately
        addr_cell.set_sync(self.hub.server_addr());

        // Each client gets its own identity token storage (not shared!)
        let identity_token = Arc::new(Mutex::new(None));
        let rejection_code = Arc::new(Mutex::new(None));

        let socket = LocalClientSocket::new_with_tokens(
            client_addr,
            self.hub.server_addr(),
            auth_req_tx,
            auth_resp_rx,
            client_data_tx,
            client_data_rx,
            addr_cell,
            identity_token,
            rejection_code,
        );

        LocalClientEndpoint::new(socket)
    }
}

impl Default for LocalTransportBuilder {
    fn default() -> Self {
        Self::new()
    }
}

