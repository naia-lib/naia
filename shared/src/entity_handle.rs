use crate::BigMapKey;
use naia_serde::{BitReader, BitWrite, Serde, SerdeErr};

// EntityHandle
#[derive(PartialEq, Eq, Hash, Clone, Copy)]
pub struct EntityHandle {
    inner: Option<EntityHandleInner>,
}

impl EntityHandle {
    pub fn from_u64(value: u64) -> Self {
        Self {
            inner: Some(EntityHandleInner::from_u64(value)),
        }
    }

    pub fn empty() -> Self {
        Self { inner: None }
    }

    pub fn inner(&self) -> Option<&EntityHandleInner> {
        return self.inner.as_ref();
    }
}

impl Serde for EntityHandle {
    fn ser<S: BitWrite>(&self, _: &mut S) {
        panic!("shouldn't call this");
    }

    fn de(_: &mut BitReader) -> Result<Self, SerdeErr> {
        panic!("shouldn't call this");
    }
}

// EntityHandleInner
#[derive(PartialEq, Eq, Hash, Clone, Copy)]
pub struct EntityHandleInner(u64);

impl EntityHandleInner {
    pub fn to_outer(self) -> EntityHandle {
        return EntityHandle { inner: Some(self) };
    }
}

impl BigMapKey for EntityHandleInner {
    fn to_u64(&self) -> u64 {
        self.0
    }

    fn from_u64(value: u64) -> Self {
        EntityHandleInner(value)
    }
}
