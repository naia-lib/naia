use std::{net::SocketAddr, sync::Arc};

use tokio::sync::RwLock;

use crate::shared::ClientServerAddr;

// MaybeAddr wrapper
struct MaybeAddr(pub(crate) ClientServerAddr);

// AddrCell equivalent for server address discovery
#[derive(Clone)]
pub(crate) struct LocalAddrCell {
    cell: Arc<RwLock<MaybeAddr>>,
}

impl LocalAddrCell {
    pub(crate) fn new() -> Self {
        Self {
            cell: Arc::new(RwLock::new(MaybeAddr(ClientServerAddr::Finding))),
        }
    }

    pub(crate) async fn recv(&self, addr: SocketAddr) {
        let mut cell = self.cell.write().await;
        cell.0 = ClientServerAddr::Found(addr);
    }

    pub(crate) fn get(&self) -> ClientServerAddr {
        match self.cell.try_read() {
            Ok(addr) => addr.0.clone(),
            Err(_) => ClientServerAddr::Finding,
        }
    }
}

