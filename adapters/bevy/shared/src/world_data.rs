use std::{
    any::Any,
    collections::{HashMap, HashSet},
};

use bevy_ecs::entity::Entity;

use naia_shared::{Protocolize, ReplicateSafe};

use super::component_access::{ComponentAccess, ComponentAccessor};

#[derive(Debug)]
pub struct WorldData<P: Protocolize> {
    entities: HashSet<Entity>,
    kind_to_accessor_map: HashMap<P::Kind, Box<dyn Any>>,
}

impl<P: Protocolize> WorldData<P> {
    pub fn new() -> Self {
        WorldData {
            entities: HashSet::new(),
            kind_to_accessor_map: HashMap::new(),
        }
    }

    // Entities //

    pub(crate) fn entities(&self) -> Vec<Entity> {
        let mut output = Vec::new();

        for entity in &self.entities {
            output.push(*entity);
        }

        output
    }

    pub(crate) fn spawn_entity(&mut self, entity: &Entity) {
        self.entities.insert(*entity);
    }

    pub(crate) fn despawn_entity(&mut self, entity: &Entity) {
        self.entities.remove(entity);
    }

    // Components

    pub(crate) fn component_access(
        &self,
        component_kind: &P::Kind,
    ) -> Option<&Box<dyn ComponentAccess<P>>> {
        if let Some(accessor_any) = self.kind_to_accessor_map.get(component_kind) {
            return accessor_any.downcast_ref::<Box<dyn ComponentAccess<P>>>();
        }
        None
    }

    pub(crate) fn has_kind(&self, component_kind: &P::Kind) -> bool {
        self.kind_to_accessor_map.contains_key(component_kind)
    }

    pub(crate) fn put_kind<R: ReplicateSafe<P>>(&mut self, component_kind: &P::Kind) {
        self.kind_to_accessor_map
            .insert(*component_kind, ComponentAccessor::<P, R>::new());
    }
}

unsafe impl<P: Protocolize> Send for WorldData<P> {}
unsafe impl<P: Protocolize> Sync for WorldData<P> {}
