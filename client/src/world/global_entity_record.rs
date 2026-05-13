use std::collections::HashSet;

use naia_shared::ComponentKind;

use crate::world::entity_owner::EntityOwner;
use naia_shared::Publicity;

pub struct GlobalEntityRecord {
    component_kinds: HashSet<ComponentKind>,
    owner: EntityOwner,
    replication_config: Publicity,
    is_replicating: bool,
    pub(crate) is_static: bool,
}

impl GlobalEntityRecord {
    pub fn new(owner: EntityOwner) -> Self {
        if owner == EntityOwner::Local {
            panic!("Should not insert Local entity in this record");
        }

        // Host-owned entities always start public, client-owned entities always start private
        let replication_config = if owner.is_server() {
            Publicity::Public
        } else {
            Publicity::Private
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

    pub fn owner(&self) -> EntityOwner {
        self.owner
    }

    pub fn replication_config(&self) -> Publicity {
        self.replication_config
    }

    pub fn is_replicating(&self) -> bool {
        self.is_replicating
    }

    pub fn component_kinds(&self) -> &HashSet<ComponentKind> {
        &self.component_kinds
    }

    pub(crate) fn set_owner(&mut self, owner: EntityOwner) {
        self.owner = owner;
    }

    pub(crate) fn set_replication_config(&mut self, replication_config: Publicity) {
        self.replication_config = replication_config;
    }

    pub(crate) fn has_component(&self, component_kind: &ComponentKind) -> bool {
        self.component_kinds.contains(component_kind)
    }

    pub(crate) fn insert_component(&mut self, component_kind: ComponentKind) {
        if !self.component_kinds.insert(component_kind) {
            panic!("Attempted to insert a component that already exists in the global entity record: {:?}", component_kind);
        }
    }

    pub(crate) fn remove_component(&mut self, component_kind: &ComponentKind) {
        let result = self.component_kinds.remove(component_kind);
        if !result {
            panic!("Attempted to remove a component that does not exist in the global entity record: {:?}", component_kind);
        }
    }
}
