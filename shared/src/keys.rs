use std::fmt;

/// Standard Naia Key trait
pub trait NaiaKey<Impl = Self>: Eq + PartialEq + Clone + Copy + fmt::Display {
    /// Create new Key from a u16
    fn from_u16(k: u16) -> Impl;
    /// Convert Key to a u16
    fn to_u16(&self) -> u16;
}

// Entity Key

/// The key that authoritatively represents an Entity in the Server
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct EntityKey(u16);

impl NaiaKey for EntityKey {
    fn from_u16(k: u16) -> Self {
        EntityKey(k)
    }

    fn to_u16(&self) -> u16 {
        self.0
    }
}

impl fmt::Display for EntityKey {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// Local Entity Key

/// The key that represents an Entity in the Client's scope, that is being
/// synced to the Client
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct LocalEntityKey(u16);

impl NaiaKey for LocalEntityKey {
    fn from_u16(k: u16) -> Self {
        LocalEntityKey(k)
    }

    fn to_u16(&self) -> u16 {
        self.0
    }
}

impl fmt::Display for LocalEntityKey {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// Local Replica Key

/// The key that represents a Object/Component in the Client's scope, that is
/// being synced to the Client
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct LocalReplicaKey(u16);

impl NaiaKey for LocalReplicaKey {
    fn from_u16(k: u16) -> Self {
        LocalReplicaKey(k)
    }

    fn to_u16(&self) -> u16 {
        self.0
    }
}

impl fmt::Display for LocalReplicaKey {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// Local Object Key

/// The key that represents an Object in the Client's scope, that is being
/// synced to the Client
pub type LocalObjectKey = LocalReplicaKey;

// Local Component Key

/// The key that represents an Component in the Client's scope, that is being
/// synced to the Client
pub type LocalComponentKey = LocalReplicaKey;

// Pawn Key

/// Key that represents a Pawn, whether it be of Object or Entity type
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum PawnKey {
    /// Object
    Object(LocalObjectKey),
    /// Entity
    Entity(LocalEntityKey),
}

impl PawnKey {
    /// Convert to u16
    pub fn to_u16(&self) -> u16 {
        match self {
            PawnKey::Object(key) => key.to_u16(),
            PawnKey::Entity(key) => key.to_u16(),
        }
    }
}
