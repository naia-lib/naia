use crate::{serde, derive_serde};

// Local Entity

// An Entity in the Client's scope, that is being
// synced to the Client
#[derive(Copy, Eq, Hash)]
#[derive_serde]
pub struct NetEntity(u16);

impl From<u16> for NetEntity {
    fn from(value: u16) -> Self {
        NetEntity(value)
    }
}

impl Into<u16> for NetEntity {
    fn into(self) -> u16 {
        self.0
    }
}

// Local Component Key

// The key that represents a Component in the Client's scope, that is
// being synced to the Client
#[derive(Copy, Eq, Hash)]
#[derive_serde]
pub struct LocalComponentKey(u16);

impl From<u16> for LocalComponentKey {
    fn from(value: u16) -> Self {
        LocalComponentKey(value)
    }
}

impl Into<u16> for LocalComponentKey {
    fn into(self) -> u16 {
        self.0
    }
}