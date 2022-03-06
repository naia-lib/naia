use crate::BigMapKey;

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
