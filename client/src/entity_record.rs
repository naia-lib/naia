use std::collections::HashSet;

use naia_shared::{EntityHandle, NetEntity, ProtocolKindType};

pub struct EntityRecord<K: ProtocolKindType> {
    pub net_entity: NetEntity,
    pub component_kinds: HashSet<K>,
    pub entity_handle: Option<EntityHandle>,
}

impl<K: ProtocolKindType> EntityRecord<K> {
    pub fn new(net_entity: NetEntity) -> Self {
        EntityRecord {
            net_entity,
            component_kinds: HashSet::new(),
            entity_handle: None,
        }
    }
}
