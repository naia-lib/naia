use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use crate::{server_addr::ServerAddr, wasm_utils::candidate_to_addr};

// MaybeAddr
struct MaybeAddr(pub(crate) ServerAddr);

// AddrCell
#[derive(Clone)]
pub struct AddrCell {
    cell: Arc<Mutex<MaybeAddr>>,
}

impl AddrCell {
    pub fn new() -> Self {
        AddrCell {
            cell: Arc::new(Mutex::new(MaybeAddr(ServerAddr::Finding))),
        }
    }

    pub fn receive_candidate(&self, candidate_str: &str) {
        self.cell
            .lock()
            .expect("This should never happen, receive_candidate() should only be called once ever during the session initialization")
            .0 = candidate_to_addr(candidate_str);
    }

    pub fn get(&self) -> ServerAddr {
        match self.cell.try_lock() {
            Ok(addr) => addr.0,
            Err(_) => ServerAddr::Finding,
        }
    }

    pub fn set_addr(&mut self, addr: &SocketAddr) {
        self.cell.lock().expect("cannot borrow AddrCell.cell!").0 = ServerAddr::Found(addr.clone());
    }
}
