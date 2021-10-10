use std::{
    any::TypeId,
    collections::{HashMap, HashSet},
};

use bevy::ecs::world::World;

use naia_shared::{ImplRef, ProtocolType};

use super::{
    component_access::{ComponentAccess, ComponentAccessor},
    entity::Entity,
};

pub struct WorldData<P: ProtocolType> {
    entities: HashSet<Entity>,
    rep_type_to_accessor_map: HashMap<TypeId, Box<dyn ComponentAccess<P>>>,
}

impl<P: ProtocolType> WorldData<P> {
    pub fn new() -> Self {
        WorldData {
            entities: HashSet::new(),
            rep_type_to_accessor_map: HashMap::new(),
        }
    }

    pub(crate) fn get_component(
        &self,
        world: &World,
        entity: &Entity,
        type_id: &TypeId,
    ) -> Option<P> {
        if let Some(accessor) = self.rep_type_to_accessor_map.get(type_id) {
            return accessor.get_component(world, entity);
        }
        return None;
    }

    pub(crate) fn has_type(&self, type_id: &TypeId) -> bool {
        return self.rep_type_to_accessor_map.contains_key(type_id);
    }

    pub(crate) fn put_type<R: ImplRef<P>>(&mut self, rep_type_id: &TypeId) {
        self.rep_type_to_accessor_map
            .insert(*rep_type_id, ComponentAccessor::<P, R>::new());
    }

    pub(crate) fn spawn_entity(&mut self, entity: &Entity) {
        self.entities.insert(*entity);
    }

    pub(crate) fn despawn_entity(&mut self, entity: &Entity) {
        self.entities.remove(&entity);
    }

    pub(crate) fn get_entities(&self) -> Vec<Entity> {
        let mut output = Vec::new();

        for entity in &self.entities {
            output.push(*entity);
        }

        return output;
    }
}
