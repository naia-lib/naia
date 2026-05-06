use std::{
    net::SocketAddr,
    sync::{Arc, RwLock},
};

use naia_shared::transport::local::ClientServerAddr;

// MaybeAddr wrapper
struct MaybeAddr(pub(crate) ClientServerAddr);

// AddrCell equivalent for server address discovery
#[derive(Clone)]
pub struct LocalAddrCell {
    cell: Arc<RwLock<MaybeAddr>>,
}

impl Default for LocalAddrCell {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalAddrCell {
    pub fn new() -> Self {
        Self {
            cell: Arc::new(RwLock::new(MaybeAddr(ClientServerAddr::Finding))),
        }
    }

    pub(crate) fn set(&self, addr: SocketAddr) {
        let mut cell = self.cell.write().unwrap();
        cell.0 = ClientServerAddr::Found(addr);
    }

    pub(crate) fn get(&self) -> ClientServerAddr {
        let cell = self.cell.read().unwrap();
        cell.0.clone()
    }

    /// Set the server address synchronously (for testing/local transport where we know it immediately)
    pub fn set_sync(&self, addr: SocketAddr) {
        self.set(addr);
    }
}
