//! In-memory local transport layers used by `transport_local` feature flag for quick E2E tests.
//!
//! Provides paired client/server sockets with types that match the transport-level trait signatures.
//! The transport layer modules wrap these types to implement the transport traits.

mod runtime;
mod shared;
pub mod client;
pub mod server;

pub use shared::{
    ClientIdentityReceiverResult, ClientServerAddr,
    ClientSendError, ClientRecvError, ServerSendError, ServerRecvError,
};

pub use client::{LocalClientSocket, LocalClientSender, LocalClientReceiver, LocalClientIdentity};
pub use server::{LocalServerSocket, LocalServerSender, LocalServerReceiver, LocalServerAuthSender, LocalServerAuthReceiver};

use client::LocalAddrCell;
use shared::{create_auth_channels, LocalTransportQueues};

/// Paired sockets for the client and server sides.
pub struct LocalSocketPair {
    pub client_socket: LocalClientSocket,
    pub server_socket: LocalServerSocket,
}

impl LocalSocketPair {
    pub fn new() -> Self {
        let (shared, client_addr, server_addr) = LocalTransportQueues::new();
        
        // Create 1:1 auth channels (not broadcast!)
        let (auth_req_tx, auth_req_rx, auth_resp_tx, auth_resp_rx) = create_auth_channels();
        
        // Create addr_cell for client
        let addr_cell = LocalAddrCell::new();
        
        let client = LocalClientSocket::new(
            shared.clone(),
            client_addr,
            server_addr,
            auth_req_tx,
            auth_resp_rx, // Client owns the response receiver
            addr_cell,
        );
        
        let server = LocalServerSocket::new(
            shared,
            client_addr,
            server_addr,
            auth_req_rx, // Server owns the request receiver
            auth_resp_tx,
        );
        
        Self {
            client_socket: client,
            server_socket: server,
        }
    }
}
