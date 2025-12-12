use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use tokio::sync::mpsc;

use crate::transport::local::shared::{create_auth_channels, create_data_channels};

/// Per-client connection state stored in the hub
/// Only stores server-side channels (what the server needs to receive/send)
struct ClientConnection {
    // Auth channels (client -> server) - server receives
    auth_req_rx: Arc<Mutex<mpsc::UnboundedReceiver<Vec<u8>>>>,
    // Auth channels (server -> client) - server sends
    auth_resp_tx: mpsc::UnboundedSender<Vec<u8>>,
    
    // Data channels (client -> server) - server receives
    server_data_rx: Arc<Mutex<mpsc::UnboundedReceiver<Vec<u8>>>>,
    // Data channels (server -> client) - server sends
    server_data_tx: mpsc::UnboundedSender<Vec<u8>>,
}

/// Shared transport hub managing multiple client connections
#[derive(Clone)]
pub struct LocalTransportHub {
    server_addr: SocketAddr,
    connections: Arc<Mutex<HashMap<SocketAddr, ClientConnection>>>,
    next_client_id: Arc<Mutex<u16>>,
    traffic_paused: Arc<Mutex<bool>>,
}

impl LocalTransportHub {
    pub fn new(server_addr: SocketAddr) -> Self {
        Self {
            // shared,
            server_addr,
            connections: Arc::new(Mutex::new(HashMap::new())),
            next_client_id: Arc::new(Mutex::new(1)),
            traffic_paused: Arc::new(Mutex::new(false)),
        }
    }

    /// Register a new client connection and return its address and channel handles
    pub fn register_client(&self) -> (
        SocketAddr, // client_addr
        mpsc::UnboundedSender<Vec<u8>>, // auth_req_tx (client sends)
        mpsc::UnboundedReceiver<Vec<u8>>, // auth_resp_rx (client receives)
        mpsc::UnboundedSender<Vec<u8>>, // client_data_tx (client sends)
        mpsc::UnboundedReceiver<Vec<u8>>, // client_data_rx (client receives)
    ) {
        // Generate unique client address
        let client_id = {
            let mut id = self.next_client_id.lock().unwrap();
            let current = *id;
            *id = current.wrapping_add(1);
            current
        };
        
        // Create fake client address based on ID
        let client_addr: SocketAddr = format!("127.0.0.1:{}", 12345 + client_id)
            .parse()
            .expect("invalid client addr");

        // Create 1:1 auth channels
        let (auth_req_tx, auth_req_rx, auth_resp_tx, auth_resp_rx) = create_auth_channels();
        
        // Create 1:1 data channels
        let (client_data_tx, server_data_rx, server_data_tx, client_data_rx) = create_data_channels();

        // Store connection (only server-side channels, client-side channels are returned)
        let connection = ClientConnection {
            auth_req_rx: Arc::new(Mutex::new(auth_req_rx)),
            auth_resp_tx: auth_resp_tx.clone(),
            server_data_rx: Arc::new(Mutex::new(server_data_rx)),
            server_data_tx: server_data_tx.clone(),
        };

        self.connections.lock().unwrap().insert(client_addr, connection);

        (
            client_addr,
            auth_req_tx,
            auth_resp_rx,
            client_data_tx,
            client_data_rx,
        )
    }
    //
    // /// Get the shared queues (for identity token, etc.)
    // pub fn shared(&self) -> &LocalTransportQueues {
    //     &self.shared
    // }

    /// Get the server address
    pub fn server_addr(&self) -> SocketAddr {
        self.server_addr
    }

    /// Try to receive an auth request from any client (returns (client_addr, bytes))
    /// Returns None if traffic is paused (packets are dropped)
    pub fn try_recv_auth_request(&self) -> Option<(SocketAddr, Vec<u8>)> {
        let paused = *self.traffic_paused.lock().unwrap(); // Single check
        let connections = self.connections.lock().unwrap();
        
        for (addr, conn) in connections.iter() {
            let mut rx_guard = conn.auth_req_rx.lock().unwrap();
            if paused {
                // Drain ALL packets when paused, not just one
                while rx_guard.try_recv().is_ok() {}
            } else if let Ok(bytes) = rx_guard.try_recv() {
                return Some((*addr, bytes));
            }
        }
        None
    }

    /// Try to receive a data packet from any client (returns (client_addr, bytes))
    /// Returns None if traffic is paused (packets are dropped)
    pub fn try_recv_data(&self) -> Option<(SocketAddr, Vec<u8>)> {
        let paused = *self.traffic_paused.lock().unwrap(); // Single check
        let connections = self.connections.lock().unwrap();
        
        for (addr, conn) in connections.iter() {
            let mut rx_guard = conn.server_data_rx.lock().unwrap();
            if paused {
                // Drain ALL packets when paused, not just one
                while rx_guard.try_recv().is_ok() {}
            } else if let Ok(bytes) = rx_guard.try_recv() {
                return Some((*addr, bytes));
            }
        }
        None
    }

    /// Send auth response to a specific client
    /// Returns Err(()) if traffic is paused (packets are dropped)
    pub fn send_auth_response(&self, client_addr: &SocketAddr, bytes: Vec<u8>) -> Result<(), ()> {
        let paused = *self.traffic_paused.lock().unwrap(); // Single check
        if paused {
            return Err(()); // Drop packet
        }
        
        let connections = self.connections.lock().unwrap();
        if let Some(conn) = connections.get(client_addr) {
            conn.auth_resp_tx.send(bytes).map_err(|_| ())
        } else {
            Err(())
        }
    }

    /// Send data packet to a specific client
    /// Returns Err(()) if traffic is paused (packets are dropped)
    pub fn send_data(&self, client_addr: &SocketAddr, bytes: Vec<u8>) -> Result<(), ()> {
        let paused = *self.traffic_paused.lock().unwrap(); // Single check
        if paused {
            return Err(()); // Drop packet
        }
        
        let connections = self.connections.lock().unwrap();
        if let Some(conn) = connections.get(client_addr) {
            conn.server_data_tx.send(bytes).map_err(|_| ())
        } else {
            Err(())
        }
    }

    /// Pause all traffic (drop all packets)
    pub fn pause_traffic(&self) {
        *self.traffic_paused.lock().unwrap() = true;
    }

    /// Resume normal traffic delivery
    pub fn resume_traffic(&self) {
        *self.traffic_paused.lock().unwrap() = false;
    }

    /// Check if traffic is paused
    pub fn is_traffic_paused(&self) -> bool {
        *self.traffic_paused.lock().unwrap()
    }
}

