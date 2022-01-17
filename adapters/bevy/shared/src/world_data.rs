use std::{
    any::Any,
    collections::{HashMap, HashSet},
};

use bevy::ecs::entity::Entity;

use naia_shared::{ProtocolType, ReplicateSafe};

use super::component_access::{ComponentAccess, ComponentAccessor};

#[derive(Debug)]
pub struct WorldData<P: ProtocolType> {
    entities: HashSet<Entity>,
    kind_to_accessor_map: HashMap<P::Kind, Box<dyn Any>>,
}

impl<P: ProtocolType> WorldData<P> {
    pub fn new() -> Self {
        WorldData {
            entities: HashSet::new(),
            kind_to_accessor_map: HashMap::new(),
        }
    }

    // Entities //

    pub(crate) fn get_entities(&self) -> Vec<Entity> {
        let mut output = Vec::new();

        for entity in &self.entities {
            output.push(*entity);
        }

        return output;
    }

    pub(crate) fn spawn_entity(&mut self, entity: &Entity) {
        self.entities.insert(*entity);
    }

    pub(crate) fn despawn_entity(&mut self, entity: &Entity) {
        self.entities.remove(&entity);
    }

    // Components

    pub(crate) fn get_component_access(
        &self,
        component_kind: &P::Kind,
    ) -> Option<&Box<dyn ComponentAccess<P>>> {
        if let Some(accessor_any) = self.kind_to_accessor_map.get(component_kind) {
            return accessor_any.downcast_ref::<Box<dyn ComponentAccess<P>>>();
        }
        return None;
    }

    pub(crate) fn has_kind(&self, component_kind: &P::Kind) -> bool {
        return self.kind_to_accessor_map.contains_key(component_kind);
    }

    pub(crate) fn put_kind<R: ReplicateSafe<P>>(&mut self, component_kind: &P::Kind) {
        self.kind_to_accessor_map
            .insert(*component_kind, ComponentAccessor::<P, R>::new());
    }
}

unsafe impl<P: ProtocolType> Send for WorldData<P> {}

unsafe impl<P: ProtocolType> Sync for WorldData<P> {}
