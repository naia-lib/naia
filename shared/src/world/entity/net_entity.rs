
use naia_serde::{BitReader, BitWrite, Serde, SerdeErr, UnsignedVariableInteger};

use crate::world::entity::owned_net_entity::OwnedNetEntity;

// UnownedNetEntity
#[derive(Copy, Eq, Hash, Clone, PartialEq)]
pub struct NetEntity(pub u16);

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
