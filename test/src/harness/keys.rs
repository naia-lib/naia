/// ClientKey - A copyable, comparable key for identifying clients in tests
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct ClientKey(u32);

impl ClientKey {
    pub(crate) fn new(id: u32) -> Self {
        Self(id)
    }

    pub(crate) fn id(&self) -> u32 {
        self.0
    }
}

/// EntityKey - A copyable, comparable key representing a logical game entity in tests
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct EntityKey(u32);

impl EntityKey {
    // Private constructor - only EntityRegistry should create EntityKeys
    pub(crate) fn new(id: u32) -> Self {
        Self(id)
    }
}

