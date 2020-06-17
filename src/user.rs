
use std::{
    net::SocketAddr,
};

new_key_type! { pub struct UserKey; }

pub struct User {
    pub address: SocketAddr,
}

impl User {
    pub fn new(address: SocketAddr) -> User {
        User {
            address
        }
    }
}