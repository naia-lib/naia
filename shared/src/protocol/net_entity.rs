// Local Entity

use crate::serde;
use naia_serde::{BitReader, BitWrite, SerdeErr, UnsignedVariableInteger};

// An Entity in the Client's scope, that is being
// synced to the Client
#[derive(Copy, Eq, Hash, Clone, PartialEq)]
pub struct NetEntity(u16);

impl From<u16> for NetEntity {
    fn from(value: u16) -> Self {
        NetEntity(value)
    }
}

impl Into<u16> for NetEntity {
    fn into(self) -> u16 {
        self.0
    }
}

impl serde::Serde for NetEntity {
    fn ser<S: BitWrite>(&self, writer: &mut S) {
        UnsignedVariableInteger::<7>::new(self.0).ser(writer);
    }

    fn de(reader: &mut BitReader) -> Result<Self, SerdeErr> {
        let value = UnsignedVariableInteger::<7>::de(reader).unwrap().get();
        return Ok(NetEntity(value as u16));
    }
}
