use std::{
    net::SocketAddr,
    sync::Arc,
};

use tokio::sync::RwLock;

use crate::transport::ServerAddr;

// MaybeAddr
struct MaybeAddr(pub(crate) ServerAddr);

// AddrCell
#[derive(Clone)]
pub struct AddrCell {
    cell: Arc<RwLock<MaybeAddr>>,
}

impl Default for AddrCell {
    fn default() -> Self {
        Self {
            cell: Arc::new(RwLock::new(MaybeAddr(ServerAddr::Finding))),
        }
    }
}

impl AddrCell {
    pub async fn recv(&self, addr: &SocketAddr) {
        let mut cell = self.cell.write().await;
        cell.0 = ServerAddr::Found(*addr);
    }

    pub fn get(&self) -> ServerAddr {
        match self.cell.try_read() {
            Ok(addr) => addr.0,
            Err(_) => ServerAddr::Finding,
        }
    }
}