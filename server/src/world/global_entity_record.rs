use std::collections::HashSet;

use naia_shared::ComponentKind;

use crate::{EntityOwner, ReplicationConfig};

pub struct GlobalEntityRecord {
    pub component_kinds: HashSet<ComponentKind>,
    pub owner: EntityOwner,
    pub replication_config: ReplicationConfig,
    pub is_replicating: bool,
    pub is_static: bool,
}

impl GlobalEntityRecord {
    pub fn new(owner: EntityOwner) -> Self {
        let replication_config = match &owner {
            EntityOwner::Server => ReplicationConfig::public(),
            EntityOwner::Client(_) | EntityOwner::ClientWaiting(_) => ReplicationConfig::private(),
            EntityOwner::ClientPublic(_) => {
                panic!("Should not be able to insert a ClientPublic record this way");
            }
            EntityOwner::Local => {
                panic!("Should not be able to insert Local entity in this record");
            }
        };
        Self {
            component_kinds: HashSet::new(),
            owner,
            replication_config,
            is_replicating: true,
            is_static: false,
        }
    }

    pub fn new_static(owner: EntityOwner) -> Self {
        let mut record = Self::new(owner);
        record.is_static = true;
        record
    }
}
