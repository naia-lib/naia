// Local Entity

use naia_serde::{BitReader, BitWrite, ConstBitLength, Serde, SerdeErr, UnsignedVariableInteger};

// An Entity in the Client's scope, that is being
// synced to the Client
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

impl ConstBitLength for NetEntity {
    fn const_bit_length() -> u32 {
        <u16 as ConstBitLength>::const_bit_length()
    }
}
