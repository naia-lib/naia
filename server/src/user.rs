use crate::naia_server::Timestamp;
use std::net::SocketAddr;

#[allow(missing_docs)]
#[allow(unused_doc_comments)]
pub mod user_key {
    /// The Key used to get a reference of a User
    new_key_type! { pub struct UserKey; }
}

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
