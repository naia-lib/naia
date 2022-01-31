use std::fmt;

use nanoserde::{DeBin, SerBin};

/// Standard Naia Key trait
pub trait NaiaKey<Impl = Self>: Eq + PartialEq + Clone + Copy + fmt::Display {
    /// Create new Key from a u16
    fn from_u16(k: u16) -> Impl;
    /// Convert Key to a u16
    fn to_u16(&self) -> u16;
}

// Local Entity

/// An Entity in the Client's scope, that is being
/// synced to the Client
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, DeBin, SerBin)]
pub struct EntityNetId(u16);

impl NaiaKey for EntityNetId {
    fn from_u16(k: u16) -> Self {
        EntityNetId(k)
    }

    fn to_u16(&self) -> u16 {
        self.0
    }
}

impl fmt::Display for EntityNetId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// Local Component Key

/// The key that represents a Component in the Client's scope, that is
/// being synced to the Client
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct LocalComponentKey(u16);

impl NaiaKey for LocalComponentKey {
    fn from_u16(k: u16) -> Self {
        LocalComponentKey(k)
    }

    fn to_u16(&self) -> u16 {
        self.0
    }
}

impl fmt::Display for LocalComponentKey {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
