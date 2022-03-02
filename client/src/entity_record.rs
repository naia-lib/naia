use std::collections::{HashMap, HashSet};

use naia_shared::{LocalComponentKey, NetEntity, ProtocolKindType};

pub struct EntityRecord<K: ProtocolKindType> {
    pub net_entity: NetEntity,
    pub component_kinds: HashSet<K>
}

impl<K: ProtocolKindType> EntityRecord<K> {
    pub fn new(net_entity: NetEntity) -> Self {
        EntityRecord {
            net_entity,
            component_kinds: HashSet::new(),
        }
    }
}
