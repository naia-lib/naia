use std::collections::HashSet;

use naia_shared::{ComponentKind, EntityHandle};

use crate::world::entity_owner::EntityOwner;

pub struct GlobalEntityRecord {
    pub entity_handle: EntityHandle,
    pub component_kinds: HashSet<ComponentKind>,
    pub owner: EntityOwner,
}

impl GlobalEntityRecord {
    pub fn new(entity_handle: EntityHandle, owner: EntityOwner) -> Self {
        if owner == EntityOwner::Local {
            panic!("Should not insert Local entity in this record");
        }
        Self {
            entity_handle,
            component_kinds: HashSet::new(),
            owner,
        }
    }
}
