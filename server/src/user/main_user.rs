use std::net::SocketAddr;

use crate::{server::MainServer, UserKey};

// MainUser

#[derive(Clone)]
pub struct MainUser {
    auth_addr: Option<SocketAddr>,
    data_addr: Option<SocketAddr>,
}

impl MainUser {
    pub fn new(auth_addr: SocketAddr) -> Self {
        Self {
            auth_addr: Some(auth_addr),
            data_addr: None,
        }
    }

    pub fn has_address(&self) -> bool {
        self.data_addr.is_some()
    }

    pub fn address(&self) -> SocketAddr {
        self.data_addr.unwrap()
    }

    pub fn address_opt(&self) -> Option<SocketAddr> {
        self.data_addr
    }

    pub(crate) fn set_address(&mut self, addr: &SocketAddr) {
        self.data_addr = Some(*addr);
    }

    pub(crate) fn take_auth_address(&mut self) -> SocketAddr {
        self.auth_addr.take().unwrap()
    }
}

// MainUserRef

pub struct MainUserRef<'s> {
    server: &'s MainServer,
    key: UserKey,
}

impl<'s> MainUserRef<'s> {
    pub(crate) fn new(server: &'s MainServer, key: &UserKey) -> Self {
        Self { server, key: *key }
    }

    pub fn key(&self) -> UserKey {
        self.key
    }

    pub fn address(&self) -> SocketAddr {
        self.server.user_address(&self.key).unwrap()
    }
}
