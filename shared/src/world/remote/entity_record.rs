use std::collections::HashSet;

use crate::{ComponentKind, EntityHandle, NetEntity};

pub struct EntityRecord {
    pub net_entity: NetEntity,
    pub component_kinds: HashSet<ComponentKind>,
    pub entity_handle: EntityHandle,
}

impl EntityRecord {
    pub fn new(net_entity: NetEntity, entity_handle: EntityHandle) -> Self {
        EntityRecord {
            net_entity,
            component_kinds: HashSet::new(),
            entity_handle,
        }
    }
}
