use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use log::debug;
use tokio::sync::mpsc;

use crate::transport::local::shared::{create_auth_channels, create_data_channels};
use crate::{link_condition_logic, Instant, LinkConditionerConfig, TimeQueue};

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
    // Data channels (client -> server) - server "receives" via this sender (injection)
    #[allow(dead_code)]
    client_data_tx_injection: mpsc::UnboundedSender<Vec<u8>>,

    // Link conditioner configuration (bidirectional)
    // None means no conditioning (perfect connection)
    client_to_server_conditioner: Option<LinkConditionerConfig>,
    server_to_client_conditioner: Option<LinkConditionerConfig>,

    // Time queues for delayed packet delivery
    client_to_server_queue: Arc<Mutex<TimeQueue<Vec<u8>>>>,
    server_to_client_queue: Arc<Mutex<TimeQueue<Vec<u8>>>>,
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
    pub fn register_client(
        &self,
    ) -> (
        SocketAddr,                       // client_addr
        mpsc::UnboundedSender<Vec<u8>>,   // auth_req_tx (client sends)
        mpsc::UnboundedReceiver<Vec<u8>>, // auth_resp_rx (client receives)
        mpsc::UnboundedSender<Vec<u8>>,   // client_data_tx (client sends)
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
        let (client_data_tx, server_data_rx, server_data_tx, client_data_rx) =
            create_data_channels();

        // Store connection (only server-side channels, client-side channels are returned)
        let connection = ClientConnection {
            auth_req_rx: Arc::new(Mutex::new(auth_req_rx)),
            auth_resp_tx: auth_resp_tx.clone(),
            server_data_rx: Arc::new(Mutex::new(server_data_rx)),
            server_data_tx: server_data_tx.clone(),
            client_data_tx_injection: client_data_tx.clone(),
            client_to_server_conditioner: None,
            server_to_client_conditioner: None,
            client_to_server_queue: Arc::new(Mutex::new(TimeQueue::new())),
            server_to_client_queue: Arc::new(Mutex::new(TimeQueue::new())),
        };

        self.connections
            .lock()
            .unwrap()
            .insert(client_addr, connection);

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

    /// Inject a packet from a client (used for testing/fuzzing)
    pub fn inject_client_packet(&self, client_addr: &SocketAddr, data: Vec<u8>) -> bool {
        let connections = self.connections.lock().unwrap();
        if let Some(conn) = connections.get(client_addr) {
            let _ = conn.client_data_tx_injection.send(data);
            return true;
        }
        false
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
    /// Applies link conditioning if configured
    /// Also processes time queues to deliver ready packets to clients
    pub fn try_recv_data(&self) -> Option<(SocketAddr, Vec<u8>)> {
        let paused = *self.traffic_paused.lock().unwrap(); // Single check
        let now = Instant::now();
        let mut connections = self.connections.lock().unwrap();

        // First, deliver any ready packets from server-to-client queues for all clients
        self.deliver_all_queued_packets_to_clients(&mut connections, &now);

        // Then check time queues for client-to-server delayed packets that are now ready
        for (addr, conn) in connections.iter_mut() {
            let mut queue_guard = conn.client_to_server_queue.lock().unwrap();
            if queue_guard.has_item(&now) {
                if let Some(bytes) = queue_guard.pop_item(&now) {
                    return Some((*addr, bytes));
                }
            }
        }

        // Finally check direct channels and apply link conditioning
        for (addr, conn) in connections.iter_mut() {
            let mut rx_guard = conn.server_data_rx.lock().unwrap();
            if paused {
                // Drain ALL packets when paused, not just one
                while rx_guard.try_recv().is_ok() {}
            } else if let Ok(bytes) = rx_guard.try_recv() {
                // Apply link conditioning if configured
                if let Some(ref config) = conn.client_to_server_conditioner {
                    let mut queue_guard = conn.client_to_server_queue.lock().unwrap();
                    link_condition_logic::process_packet(config, &mut queue_guard, bytes);
                    // Packet is now in queue, will be delivered later
                    continue;
                } else {
                    // No conditioning, deliver immediately
                    return Some((*addr, bytes));
                }
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
    /// Applies link conditioning if configured
    /// Also processes time queues to deliver any ready packets
    pub fn send_data(&self, client_addr: &SocketAddr, bytes: Vec<u8>) -> Result<(), ()> {
        let paused = *self.traffic_paused.lock().unwrap(); // Single check
        if paused {
            debug!("[HUB] send_data: Packet dropped (traffic paused)");
            return Err(()); // Drop packet
        }

        let now = Instant::now();
        let mut connections = self.connections.lock().unwrap();

        // First, deliver any ready packets from server-to-client queues for all clients
        self.deliver_all_queued_packets_to_clients(&mut connections, &now);

        if let Some(conn) = connections.get_mut(client_addr) {
            // Apply link conditioning if configured
            if let Some(ref config) = conn.server_to_client_conditioner {
                let packet_len = bytes.len();
                let mut queue_guard = conn.server_to_client_queue.lock().unwrap();
                let queue_len_before = queue_guard.len();
                link_condition_logic::process_packet(config, &mut queue_guard, bytes);
                let queue_len_after = queue_guard.len();
                // Packet queued with link conditioner - use debug logging instead
                debug!(
                    "[HUB] send_data: Queued packet for client {} ({} bytes, queue: {} -> {})",
                    client_addr, packet_len, queue_len_before, queue_len_after
                );
                // Packet is now in queue, will be delivered later
                Ok(())
            } else {
                // No conditioning, send immediately - use debug logging instead
                debug!(
                    "[HUB] send_data: Sending packet immediately to {} ({} bytes, no conditioner)",
                    client_addr,
                    bytes.len()
                );
                conn.server_data_tx.send(bytes).map_err(|_| ())
            }
        } else {
            println!(
                "[HUB] send_data: Client {} not found in connections",
                client_addr
            );
            Err(())
        }
    }

    /// Deliver any ready packets from server-to-client time queues to client channels
    /// Called periodically to ensure delayed packets are delivered
    /// Returns the number of packets delivered
    fn deliver_all_queued_packets_to_clients(
        &self,
        connections: &mut HashMap<SocketAddr, ClientConnection>,
        now: &Instant,
    ) -> usize {
        let mut total_delivered = 0;
        for (addr, conn) in connections.iter_mut() {
            let mut queue_guard = conn.server_to_client_queue.lock().unwrap();
            let queue_len_before = queue_guard.len();
            let mut delivered_this_client = 0;
            while queue_guard.has_item(now) {
                if let Some(bytes) = queue_guard.pop_item(now) {
                    match conn.server_data_tx.send(bytes) {
                        Ok(()) => {
                            delivered_this_client += 1;
                            total_delivered += 1;
                        }
                        Err(_) => {
                            println!("[HUB] deliver_all_queued: Failed to send packet to client {} (channel closed?)", addr);
                        }
                    }
                }
            }
            if delivered_this_client > 0 {
                debug!(
                    "[HUB] deliver_all_queued: Delivered {} packets to client {} (queue: {} -> {})",
                    delivered_this_client,
                    addr,
                    queue_len_before,
                    queue_guard.len()
                );
            }
        }
        total_delivered
    }

    /// Process time queues for all connections to deliver any ready packets
    /// This should be called periodically (e.g., during each tick) to ensure
    /// delayed packets are delivered even when there's no active send/recv
    pub fn process_time_queues(&self) {
        let now = Instant::now();
        let mut connections = self.connections.lock().unwrap();

        // Deliver ready packets from server-to-client queues
        let delivered = self.deliver_all_queued_packets_to_clients(&mut connections, &now);
        if delivered > 0 {
            println!(
                "[HUB] process_time_queues: Delivered {} total packets",
                delivered
            );
        }

        // Note: client-to-server queues are processed in try_recv_data(),
        // which is called during server receive operations
    }

    /// Configure link conditioner for a specific client connection
    /// `client_to_server` applies to packets from client to server
    /// `server_to_client` applies to packets from server to client
    /// Pass `None` to disable conditioning for that direction
    pub fn configure_link_conditioner(
        &self,
        client_addr: &SocketAddr,
        client_to_server: Option<LinkConditionerConfig>,
        server_to_client: Option<LinkConditionerConfig>,
    ) -> bool {
        let mut connections = self.connections.lock().unwrap();
        if let Some(conn) = connections.get_mut(client_addr) {
            conn.client_to_server_conditioner = client_to_server;
            conn.server_to_client_conditioner = server_to_client;
            true
        } else {
            false
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
