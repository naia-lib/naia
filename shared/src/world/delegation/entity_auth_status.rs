use naia_serde::SerdeInternal;

use crate::HostType;

#[derive(SerdeInternal, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EntityAuthStatus {
    // as far as we know, no authority over entity has been granted
    Available,
    // host has requested authority, but it has not yet been granted
    Requested,
    // host has been granted authority over entity
    Granted,
    // host has released authority, but it has not yet completed
    Releasing,
    // host has been denied authority over entity (another host has claimed it)
    Denied,
}

pub struct HostEntityAuthStatus {
    host_type: HostType,
    auth_status: EntityAuthStatus,
}

impl HostEntityAuthStatus {
    pub fn new(host_type: HostType, auth_status: EntityAuthStatus) -> Self {
        Self {
            host_type,
            auth_status,
        }
    }

    pub fn can_request(&self) -> bool {
        match (self.host_type, self.auth_status) {
            (HostType::Client, EntityAuthStatus::Available) => true,
            (HostType::Client, EntityAuthStatus::Requested) => false,
            (HostType::Client, EntityAuthStatus::Granted) => false,
            (HostType::Client, EntityAuthStatus::Releasing) => false,
            (HostType::Client, EntityAuthStatus::Denied) => false,
            (HostType::Server, EntityAuthStatus::Available) => todo!(),
            (HostType::Server, EntityAuthStatus::Requested) => todo!(),
            (HostType::Server, EntityAuthStatus::Granted) => todo!(),
            (HostType::Server, EntityAuthStatus::Releasing) => todo!(),
            (HostType::Server, EntityAuthStatus::Denied) => todo!(),
        }
    }

    pub fn can_release(&self) -> bool {
        match (self.host_type, self.auth_status) {
            (HostType::Client, EntityAuthStatus::Available) => false,
            (HostType::Client, EntityAuthStatus::Requested) => true,
            (HostType::Client, EntityAuthStatus::Granted) => true,
            (HostType::Client, EntityAuthStatus::Releasing) => false,
            (HostType::Client, EntityAuthStatus::Denied) => false,
            (HostType::Server, EntityAuthStatus::Available) => todo!(),
            (HostType::Server, EntityAuthStatus::Requested) => todo!(),
            (HostType::Server, EntityAuthStatus::Granted) => todo!(),
            (HostType::Server, EntityAuthStatus::Releasing) => todo!(),
            (HostType::Server, EntityAuthStatus::Denied) => todo!(),
        }
    }

    pub fn can_mutate(&self) -> bool {
        match (self.host_type, self.auth_status) {
            (HostType::Client, EntityAuthStatus::Available) => false,
            (HostType::Client, EntityAuthStatus::Requested) => true,
            (HostType::Client, EntityAuthStatus::Granted) => true,
            (HostType::Client, EntityAuthStatus::Releasing) => false,
            (HostType::Client, EntityAuthStatus::Denied) => false,
            (HostType::Server, EntityAuthStatus::Available) => true,
            (HostType::Server, EntityAuthStatus::Requested) => true,
            (HostType::Server, EntityAuthStatus::Granted) => true,
            (HostType::Server, EntityAuthStatus::Releasing) => true,
            (HostType::Server, EntityAuthStatus::Denied) => true,
        }
    }

    pub fn can_read(&self) -> bool {
        match (self.host_type, self.auth_status) {
            (HostType::Client, EntityAuthStatus::Available) => true,
            (HostType::Client, EntityAuthStatus::Requested) => false,
            (HostType::Client, EntityAuthStatus::Granted) => false,
            (HostType::Client, EntityAuthStatus::Releasing) => true,
            (HostType::Client, EntityAuthStatus::Denied) => true,
            (HostType::Server, EntityAuthStatus::Available) => true,
            (HostType::Server, EntityAuthStatus::Requested) => true,
            (HostType::Server, EntityAuthStatus::Granted) => true,
            (HostType::Server, EntityAuthStatus::Releasing) => true,
            (HostType::Server, EntityAuthStatus::Denied) => true,
        }
    }

    pub fn can_write(&self) -> bool {
        match (self.host_type, self.auth_status) {
            (HostType::Client, EntityAuthStatus::Available) => false,
            (HostType::Client, EntityAuthStatus::Requested) => false,
            (HostType::Client, EntityAuthStatus::Granted) => true,
            (HostType::Client, EntityAuthStatus::Releasing) => false,
            (HostType::Client, EntityAuthStatus::Denied) => false,
            (HostType::Server, EntityAuthStatus::Available) => true,
            (HostType::Server, EntityAuthStatus::Requested) => true,
            (HostType::Server, EntityAuthStatus::Granted) => true,
            (HostType::Server, EntityAuthStatus::Releasing) => true,
            (HostType::Server, EntityAuthStatus::Denied) => true,
        }
    }

    pub fn status(&self) -> EntityAuthStatus {
        self.auth_status
    }
}

impl EntityAuthStatus {
    pub fn is_available(&self) -> bool {
        matches!(self, EntityAuthStatus::Available)
    }

    pub fn is_requested(&self) -> bool {
        matches!(self, EntityAuthStatus::Requested)
    }

    pub fn is_granted(&self) -> bool {
        matches!(self, EntityAuthStatus::Granted)
    }

    pub fn is_denied(&self) -> bool {
        matches!(self, EntityAuthStatus::Denied)
    }

    pub fn is_releasing(&self) -> bool {
        matches!(self, EntityAuthStatus::Releasing)
    }
}
