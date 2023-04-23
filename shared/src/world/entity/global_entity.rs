use crate::BigMapKey;
use naia_serde::{BitReader, BitWrite, Serde, SerdeErr};

// GlobalEntity
#[derive(PartialEq, Eq, Hash, Clone, Copy, Debug)]
pub struct GlobalEntity(u64);

impl BigMapKey for GlobalEntity {
    fn to_u64(&self) -> u64 {
        self.0
    }

    fn from_u64(value: u64) -> Self {
        GlobalEntity(value)
    }
}

impl Serde for GlobalEntity {
    fn ser(&self, _: &mut dyn BitWrite) {
        panic!("shouldn't call this");
    }

    fn de(_: &mut BitReader) -> Result<Self, SerdeErr> {
        panic!("shouldn't call this");
    }

    fn bit_length(&self) -> u32 {
        panic!("shouldn't call this");
    }
}
