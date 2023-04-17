use naia_serde::{BitReader, BitWrite, Serde, SerdeErr, UnsignedVariableInteger};

use crate::world::entity::owned_entity::OwnedEntity;

// LocalEntity
#[derive(Copy, Eq, Hash, Clone, PartialEq)]
pub struct LocalEntity(pub u16);

impl From<LocalEntity> for u16 {
    fn from(entity: LocalEntity) -> u16 {
        entity.0
    }
}

impl From<u16> for LocalEntity {
    fn from(value: u16) -> Self {
        LocalEntity(value)
    }
}

impl LocalEntity {
    pub fn to_host_owned(self) -> OwnedEntity {
        OwnedEntity::Host(self.0)
    }
    pub fn to_remote_owned(self) -> OwnedEntity {
        OwnedEntity::Remote(self.0)
    }
}

impl Serde for LocalEntity {
    fn ser(&self, writer: &mut dyn BitWrite) {
        UnsignedVariableInteger::<7>::new(self.0).ser(writer);
    }

    fn de(reader: &mut BitReader) -> Result<Self, SerdeErr> {
        let value = UnsignedVariableInteger::<7>::de(reader)?.get();
        Ok(LocalEntity(value as u16))
    }

    fn bit_length(&self) -> u32 {
        UnsignedVariableInteger::<7>::new(self.0).bit_length()
    }
}
