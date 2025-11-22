use crate::hub::LocalTransportHub;
use crate::shared::LocalTransportQueues;
use crate::endpoint::{LocalServerEndpoint, LocalClientEndpoint};
use crate::client::{LocalClientSocket, LocalAddrCell};

/// Builder for creating local transport endpoints
pub struct LocalTransportBuilder {
    hub: LocalTransportHub,
}

impl LocalTransportBuilder {
    /// Create a new builder with a shared transport hub
    pub fn new() -> Self {
        let (shared, _client_addr, server_addr) = LocalTransportQueues::new();
        let hub = LocalTransportHub::new(shared, server_addr);
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

        let socket = LocalClientSocket::new(
            self.hub.shared().clone(),
            client_addr,
            self.hub.server_addr(),
            auth_req_tx,
            auth_resp_rx,
            client_data_tx,
            client_data_rx,
            addr_cell,
        );

        LocalClientEndpoint::new(socket)
    }

    /// Convenience method for single-client setup (backwards compatibility)
    pub fn single_connection(&self) -> (LocalServerEndpoint, LocalClientEndpoint) {
        let server = self.server_endpoint();
        let client = self.connect_client();
        (server, client)
    }
}

impl Default for LocalTransportBuilder {
    fn default() -> Self {
        Self::new()
    }
}

