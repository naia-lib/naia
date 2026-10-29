use super::interface::*;
use crate::webrtc::util::error::*;
use crate::webrtc::util::{ifaces, Conn};

use async_net::UdpSocket;
use ipnet::IpNet;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

// Net represents the local network stack: the system's real interfaces and
// real UDP sockets. (The upstream "virtual network" test scaffolding this
// module once carried was compile-time unreachable in this client and has
// been removed.)
pub(crate) struct Net {
    ifs: Vec<Interface>,
}

impl Net {
    pub(crate) fn new() -> Self {
        let interfaces = ifaces::ifaces().unwrap_or_default();

        let mut m: HashMap<String, Vec<IpNet>> = HashMap::new();
        for iface in interfaces {
            if let Some(addrs) = m.get_mut(&iface.name) {
                if let Some(addr) = iface.addr {
                    if let Ok(inet) = Interface::convert(addr, iface.mask) {
                        addrs.push(inet);
                    }
                }
            } else if let Some(addr) = iface.addr {
                if let Ok(inet) = Interface::convert(addr, iface.mask) {
                    m.insert(iface.name, vec![inet]);
                }
            }
        }

        let mut ifs = vec![];
        for (name, addrs) in m.into_iter() {
            ifs.push(Interface::new(name, addrs));
        }

        Net { ifs }
    }

    // Interfaces returns a list of the system's network interfaces.
    pub(crate) async fn get_interfaces(&self) -> Vec<Interface> {
        self.ifs.clone()
    }

    pub(crate) async fn bind(&self, addr: SocketAddr) -> Result<Arc<dyn Conn + Send + Sync>> {
        Ok(Arc::new(UdpSocket::bind(addr).await?))
    }
}
