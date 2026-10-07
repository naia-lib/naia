use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use crate::{server_addr::ServerAddr, wasm_utils::candidate_to_addr};

// MaybeAddr
struct MaybeAddr(pub(crate) ServerAddr);

/// Tracks the server's data-channel address, which is not known until an ICE
/// candidate carrying it arrives over signaling.
#[derive(Clone)]
pub struct AddrCell {
    cell: Arc<Mutex<MaybeAddr>>,
}

impl AddrCell {
    /// Creates a new `AddrCell` that reports `Finding` until an address is
    /// set.
    pub fn new() -> Self {
        AddrCell {
            cell: Arc::new(Mutex::new(MaybeAddr(ServerAddr::Finding))),
        }
    }

    /// Parses an ICE candidate string and, if it yields a usable address,
    /// records it.
    pub fn receive_candidate(&self, candidate_str: &str) {
        self.cell
            .lock()
            .expect("This should never happen, receive_candidate() should only be called once ever during the session initialization")
            .0 = candidate_to_addr(candidate_str);
    }

    /// Returns the server's data-channel address, or `Finding` if it has not
    /// arrived yet. Non-blocking: a contended lock also reports `Finding`.
    pub fn get(&self) -> ServerAddr {
        match self.cell.try_lock() {
            Ok(addr) => addr.0,
            Err(_) => ServerAddr::Finding,
        }
    }

    /// Directly records the server's resolved address.
    pub fn set_addr(&mut self, addr: &SocketAddr) {
        self.cell.lock().expect("cannot borrow AddrCell.cell!").0 = ServerAddr::Found(*addr);
    }
}
