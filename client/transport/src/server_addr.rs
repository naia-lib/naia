use std::net::SocketAddr;

/// The server's socket address, if it has been found
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ServerAddr {
    /// Client has found the server's socket address
    Found(SocketAddr),
    /// Client is still finding the server's socket address
    Finding,
}

impl ServerAddr {
    pub fn is_found(&self) -> bool {
        match self {
            ServerAddr::Found(_) => true,
            ServerAddr::Finding => false,
        }
    }
}
