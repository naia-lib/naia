use crate::naia_server::Timestamp;
use std::net::SocketAddr;

new_key_type! { pub struct UserKey; }

#[derive(Clone)]
pub struct User {
    pub address: SocketAddr,
    pub timestamp: Timestamp,
}

impl User {
    pub fn new(address: SocketAddr, timestamp: Timestamp) -> User {
        User { address, timestamp }
    }
}
