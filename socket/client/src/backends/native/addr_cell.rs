use std::{sync::{Arc, Mutex}, net::SocketAddr};
use crossbeam::channel::Receiver;

use crate::server_addr::ServerAddr;

// AddrCell
#[derive(Clone)]
pub struct AddrCell {
    cell: Arc<Mutex<Option<SocketAddr>>>,
    addr_receiver: Receiver<SocketAddr>,
}

impl AddrCell {
    pub(crate) fn new(addr_receiver: Receiver<SocketAddr>) -> Self {
        AddrCell {
            cell: Arc::new(Mutex::new(None)),
            addr_receiver,
        }
    }
}

impl AddrCell {
    pub fn get(&self) -> ServerAddr {
        if let Ok(x) = self.cell.lock() {
            if let Some(addr) = x.as_ref() {
                return ServerAddr::Found(*addr);
            }
        }

        // at this point, haven't received cell
        if let Ok(new_addr) = self.addr_receiver.recv() {
            let current_addr_result = ServerAddr::Found(new_addr.clone());
            if let Ok(mut x) = self.cell.as_ref().lock() {
                *x = Some(new_addr);
            }
            return current_addr_result;
        }

        return ServerAddr::Finding;
    }
}