use crate::{BigMapKey, NetEntity, NetEntityHandleConverter};
use naia_serde::{BitReader, BitWrite, BitWriter, Serde, SerdeErr};

// EntityHandle
#[derive(PartialEq, Eq, Hash, Clone, Copy)]
pub struct EntityHandle(u64);

impl BigMapKey for EntityHandle {
    fn to_u64(&self) -> u64 {
        self.0
    }

    fn from_u64(value: u64) -> Self {
        EntityHandle(value)
    }
}

impl Serde for EntityHandle {
    fn ser(&self, _: &mut dyn BitWrite) {
        panic!("shouldn't call this");
    }

    fn de(_: &mut BitReader) -> Result<Self, SerdeErr> {
        panic!("shouldn't call this");
    }
}


// TODO: create trait for this?

impl EntityHandle {
    pub(crate) fn write(&self, writer: &mut dyn BitWrite, converter: &dyn NetEntityHandleConverter) {
        converter.handle_to_net_entity(self).ser(writer);
    }

    pub(crate) fn read(reader: &mut BitReader, converter: &dyn NetEntityHandleConverter) -> Result<Self, SerdeErr> {
        let net_entity = NetEntity::de(reader)?;
        Ok(converter.net_entity_to_handle(&net_entity))
    }

    pub(crate) fn read_write(reader: &mut BitReader, writer: &mut BitWriter) -> Result<(), SerdeErr> {
        NetEntity::de(reader)?.ser(writer);
        Ok(())
    }
}