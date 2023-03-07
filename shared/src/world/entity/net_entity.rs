// Local Entity

use naia_serde::{BitReader, BitWrite, ConstBitLength, Serde, SerdeErr, UnsignedVariableInteger};

// OwnedNetEntity
#[derive(Copy, Eq, Hash, Clone, PartialEq)]
pub enum OwnedNetEntity {
    Host(u16),
    Remote(u16),
}

impl OwnedNetEntity {
    pub fn new_host(id: u16) -> Self {
        Self::Host(id)
    }

    pub fn new_remote(id: u16) -> Self {
        Self::Remote(id)
    }

    pub fn is_host(&self) -> bool {
        match self {
            OwnedNetEntity::Host(_) => true,
            OwnedNetEntity::Remote(_) => false,
        }
    }

    pub fn value(&self) -> u16 {
        match self {
            OwnedNetEntity::Host(value) => *value,
            OwnedNetEntity::Remote(value) => *value,
        }
    }

    pub fn to_unowned(self) -> NetEntity {
        NetEntity(self.value())
    }
}

impl Serde for OwnedNetEntity {
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

// UnownedNetEntity
#[derive(Copy, Eq, Hash, Clone, PartialEq)]
pub struct NetEntity(u16);

impl From<NetEntity> for u16 {
    fn from(entity: NetEntity) -> u16 {
        entity.0
    }
}

impl From<u16> for NetEntity {
    fn from(value: u16) -> Self {
        NetEntity(value)
    }
}

impl NetEntity {
    pub fn to_host_owned(self) -> OwnedNetEntity {
        OwnedNetEntity::Host(self.0)
    }
    pub fn to_remote_owned(self) -> OwnedNetEntity {
        OwnedNetEntity::Remote(self.0)
    }
}

impl Serde for NetEntity {
    fn ser(&self, writer: &mut dyn BitWrite) {
        UnsignedVariableInteger::<7>::new(self.0).ser(writer);
    }

    fn de(reader: &mut BitReader) -> Result<Self, SerdeErr> {
        let value = UnsignedVariableInteger::<7>::de(reader)?.get();
        Ok(NetEntity(value as u16))
    }

    fn bit_length(&self) -> u32 {
        UnsignedVariableInteger::<7>::new(self.0).bit_length()
    }
}
