/// The key that represents an Replicate in the Client's scope, that is being
/// synced to the Client
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct LocalObjectKey(u16);

/// The key that authoritatively represents an Entity in the Server
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct EntityKey(u16);

/// The key that represents an Entity in the Client's scope, that is being
/// synced to the Client
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct LocalEntityKey(u16);

/// The key that represents an Component in the Client's scope, that is being
/// synced to the Client
pub type LocalComponentKey = LocalObjectKey;

// FromU16

/// Indicates the key can be constructed from a u16
pub trait NaiaKey<Impl = Self>: Eq + PartialEq + Clone + Copy {
    /// Create new Key from a u16
    fn from_u16(k: u16) -> Impl;
    /// Convert Key to a u16
    fn to_u16(&self) -> u16;
}

impl NaiaKey for LocalObjectKey {
    fn from_u16(k: u16) -> Self {
        LocalObjectKey(k)
    }
    fn to_u16(&self) -> u16 {
        self.0
    }
}

impl NaiaKey for EntityKey {
    fn from_u16(k: u16) -> Self {
        EntityKey(k)
    }
    fn to_u16(&self) -> u16 {
        self.0
    }
}

impl NaiaKey for LocalEntityKey {
    fn from_u16(k: u16) -> Self {
        LocalEntityKey(k)
    }
    fn to_u16(&self) -> u16 {
        self.0
    }
}

// Pawn Key

/// Pawn Key
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum PawnKey {
    /// Replicate
    Replicate(LocalObjectKey),
    /// Entity
    Entity(LocalEntityKey),
}

impl PawnKey {
    /// Convert to u16
    pub fn to_u16(&self) -> u16 {
        match self {
            PawnKey::Replicate(key) => key.to_u16(),
            PawnKey::Entity(key) => key.to_u16(),
        }
    }
}