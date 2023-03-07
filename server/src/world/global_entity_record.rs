use std::collections::HashSet;

use naia_shared::{ComponentKind, EntityHandle};

pub struct GlobalEntityRecord {
    pub entity_handle: EntityHandle,
    pub component_kinds: HashSet<ComponentKind>,
}

impl GlobalEntityRecord {
    pub fn new(entity_handle: EntityHandle) -> Self {
        Self {
            entity_handle,
            component_kinds: HashSet::new(),
        }
    }
}
