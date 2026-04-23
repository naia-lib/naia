/// ClientKey - A copyable, comparable key for identifying clients in tests
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct ClientKey(u32);

impl ClientKey {
    pub(crate) fn new(id: u32) -> Self {
        Self(id)
    }

    /// Creates an invalid client key for testing "unknown client" scenarios.
    /// This key will not correspond to any real client in the scenario.
    pub fn invalid() -> Self {
        Self(u32::MAX)
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

    /// Creates an invalid entity key for testing "unknown entity" scenarios.
    /// This key will not correspond to any real entity in the scenario.
    pub fn invalid() -> Self {
        Self(u32::MAX)
    }
}
