use std::collections::HashSet;

use naia_shared::{ComponentKind, GlobalEntity};

use crate::{world::entity_owner::EntityOwner, ReplicationConfig};

pub struct GlobalEntityRecord {
    pub global_entity: GlobalEntity,
    pub component_kinds: HashSet<ComponentKind>,
    pub owner: EntityOwner,
    pub replication_config: ReplicationConfig,
    pub is_replicating: bool,
}

impl GlobalEntityRecord {
    pub fn new(global_entity: GlobalEntity, owner: EntityOwner) -> Self {
        if owner == EntityOwner::Local {
            panic!("Should not insert Local entity in this record");
        }

        // Host-owned entities always start public, client-owned entities always start private
        let replication_config = if owner.is_server() {
            ReplicationConfig::Public
        } else {
            ReplicationConfig::Private
        };

        Self {
            global_entity,
            component_kinds: HashSet::new(),
            owner,
            replication_config,
            is_replicating: true,
        }
    }
}
