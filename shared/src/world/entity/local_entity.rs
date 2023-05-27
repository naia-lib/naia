use naia_serde::{BitReader, BitWrite, ConstBitLength, Serde, SerdeErr, UnsignedVariableInteger};

use crate::{EntityDoesNotExistError, GlobalEntity, LocalEntityAndGlobalEntityConverter};

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum OwnedLocalEntity {
    Host(u16),
    Remote(u16),
}

impl OwnedLocalEntity {
    fn is_host(&self) -> bool {
        match self {
            Self::Host(_) => true,
            Self::Remote(_) => false,
        }
    }

    fn value(&self) -> u16 {
        match self {
            Self::Host(value) => *value,
            Self::Remote(value) => *value,
        }
    }

    pub fn ser(&self, writer: &mut dyn BitWrite) {
        self.is_host().ser(writer);
        UnsignedVariableInteger::<7>::new(self.value()).ser(writer);
    }

    pub fn de(reader: &mut BitReader) -> Result<Self, SerdeErr> {
        let is_host = bool::de(reader)?;
        let value = UnsignedVariableInteger::<7>::de(reader)?.get();
        match is_host {
            true => Ok(Self::Host(value as u16)),
            false => Ok(Self::Remote(value as u16)),
        }
    }

    pub fn bit_length(&self) -> u32 {
        bool::const_bit_length() + UnsignedVariableInteger::<7>::new(self.value()).bit_length()
    }

    pub(crate) fn convert_to_global(
        &self,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
    ) -> Result<GlobalEntity, EntityDoesNotExistError> {
        match self {
            OwnedLocalEntity::Host(host_entity) => {
                converter.host_entity_to_global_entity(&HostEntity::new(*host_entity))
            }
            OwnedLocalEntity::Remote(remote_entity) => {
                converter.remote_entity_to_global_entity(&RemoteEntity::new(*remote_entity))
            }
        }
    }

    pub(crate) fn take_remote(&self) -> RemoteEntity {
        let OwnedLocalEntity::Remote(remote_entity) = self else {
            panic!("Expected RemoteEntity")
        };
        RemoteEntity::new(*remote_entity)
    }

    pub(crate) fn to_reversed(&self) -> OwnedLocalEntity {
        match self {
            OwnedLocalEntity::Host(host_entity) => OwnedLocalEntity::Remote(*host_entity),
            OwnedLocalEntity::Remote(remote_entity) => OwnedLocalEntity::Host(*remote_entity),
        }
    }
}

#[derive(Copy, Eq, Hash, Clone, PartialEq, Debug)]
pub struct HostEntity(u16);

impl HostEntity {
    pub fn new(id: u16) -> Self {
        Self(id)
    }

    pub fn value(&self) -> u16 {
        self.0
    }

    pub fn to_remote(self) -> RemoteEntity {
        RemoteEntity::new(self.0)
    }

    pub fn ser(&self, writer: &mut dyn BitWrite) {
        UnsignedVariableInteger::<7>::new(self.value()).ser(writer);
    }

    pub fn bit_length(&self) -> u32 {
        UnsignedVariableInteger::<7>::new(self.value()).bit_length()
    }

    pub fn copy_to_owned(&self) -> OwnedLocalEntity {
        OwnedLocalEntity::Host(self.value())
    }
}

// RemoteEntity
#[derive(Copy, Eq, Hash, Clone, PartialEq, Debug)]
pub struct RemoteEntity(u16);

impl RemoteEntity {
    pub fn new(id: u16) -> Self {
        Self(id)
    }

    pub fn value(&self) -> u16 {
        self.0
    }

    pub fn to_host(self) -> HostEntity {
        HostEntity::new(self.0)
    }

    pub fn de(reader: &mut BitReader) -> Result<Self, SerdeErr> {
        let value = UnsignedVariableInteger::<7>::de(reader)?.get();
        Ok(Self(value as u16))
    }

    pub fn copy_to_owned(&self) -> OwnedLocalEntity {
        OwnedLocalEntity::Remote(self.value())
    }
}
