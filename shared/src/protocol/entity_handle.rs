use crate::BigMapKey;
use naia_serde::{BitReader, BitWrite, Serde, SerdeErr};

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
