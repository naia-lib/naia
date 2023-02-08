use std::any::TypeId;
use std::ops::{Deref, DerefMut};

use naia_serde::SerdeInternal;

use crate::Components;

pub type PacketIndex = u16;
pub type Tick = u16;
pub type MessageIndex = u16;
pub type ShortMessageIndex = u8;
pub enum HostType {
    Server,
    Client,
}
pub type NetId = u16;

/// ComponentId - should be one unique value for each type of Component
#[derive(Eq, Hash, Copy, Clone, PartialEq)]
pub struct ComponentId {
    type_id: TypeId,
}

impl From<TypeId> for ComponentId {
    fn from(type_id: TypeId) -> Self {
        Self {
            type_id
        }
    }
}
impl Deref for ComponentId {
    type Target = TypeId;

    fn deref(&self) -> &Self::Target {
        &self.type_id
    }
}
impl DerefMut for ComponentId {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.type_id
    }
}

// MessageId
#[derive(Eq, Hash, Copy, Clone, PartialEq)]
pub struct MessageId {
    type_id: TypeId,
}

impl From<TypeId> for MessageId {
    fn from(type_id: TypeId) -> Self {
        Self {
            type_id
        }
    }
}
impl Deref for MessageId {
    type Target = TypeId;

    fn deref(&self) -> &Self::Target {
        &self.type_id
    }
}
impl DerefMut for MessageId {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.type_id
    }
}

// ChannelId
#[derive(Eq, Hash, Copy, Clone, PartialEq)]
pub struct ChannelId {
    type_id: TypeId,
}

impl From<TypeId> for ChannelId {
    fn from(type_id: TypeId) -> Self {
        Self {
            type_id
        }
    }
}
impl Deref for ChannelId {
    type Target = TypeId;

    fn deref(&self) -> &Self::Target {
        &self.type_id
    }
}
impl DerefMut for ChannelId {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.type_id
    }
}
