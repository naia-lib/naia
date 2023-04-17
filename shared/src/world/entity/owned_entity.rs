use naia_serde::{BitReader, BitWrite, ConstBitLength, Serde, SerdeErr, UnsignedVariableInteger};

use crate::LocalEntity;

// OwnedEntity
#[derive(Copy, Eq, Hash, Clone, PartialEq)]
pub enum OwnedEntity {
    Host(u16),
    Remote(u16),
}

impl OwnedEntity {
    pub fn new_host(id: u16) -> Self {
        Self::Host(id)
    }

    pub fn new_remote(id: u16) -> Self {
        Self::Remote(id)
    }

    pub fn is_host(&self) -> bool {
        match self {
            OwnedEntity::Host(_) => true,
            OwnedEntity::Remote(_) => false,
        }
    }

    pub fn value(&self) -> u16 {
        match self {
            OwnedEntity::Host(value) => *value,
            OwnedEntity::Remote(value) => *value,
        }
    }

    pub fn to_unowned(self) -> LocalEntity {
        LocalEntity(self.value())
    }

    pub fn to_reversed(self) -> Self {
        match self {
            OwnedEntity::Host(value) => OwnedEntity::Remote(value),
            OwnedEntity::Remote(value) => OwnedEntity::Host(value),
        }
    }
}

impl Serde for OwnedEntity {
    fn ser(&self, writer: &mut dyn BitWrite) {
        self.is_host().ser(writer);
        UnsignedVariableInteger::<7>::new(self.value()).ser(writer);
    }

    fn de(reader: &mut BitReader) -> Result<Self, SerdeErr> {
        let is_host = bool::de(reader)?;
        let value = UnsignedVariableInteger::<7>::de(reader)?.get();
        match is_host {
            true => Ok(Self::Host(value as u16)),
            false => Ok(Self::Remote(value as u16)),
        }
    }

    fn bit_length(&self) -> u32 {
        bool::const_bit_length() + UnsignedVariableInteger::<7>::new(self.value()).bit_length()
    }
}
