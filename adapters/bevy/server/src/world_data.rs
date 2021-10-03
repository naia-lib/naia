use std::{
    any::TypeId,
    collections::{HashMap, HashSet},
};

use naia_server::{ImplRef, ProtocolType};

use super::{
    component_access::{ComponentAccess, ComponentAccessor},
    entity::Entity,
};

pub struct WorldMetadata<P: ProtocolType> {
    entities: HashSet<Entity>,
    rep_type_to_handler_map: HashMap<TypeId, Box<dyn ComponentAccess<P>>>,
    ref_type_to_rep_type_map: HashMap<TypeId, TypeId>,
}

impl<P: ProtocolType> WorldMetadata<P> {
    pub fn new() -> Self {
        WorldMetadata {
            entities: HashSet::new(),
            rep_type_to_handler_map: HashMap::new(),
            ref_type_to_rep_type_map: HashMap::new(),
        }
    }

    pub(crate) fn get_handler(&self, type_id: &TypeId) -> Option<&Box<dyn ComponentAccess<P>>> {
        return self.rep_type_to_handler_map.get(type_id);
    }

    pub(crate) fn has_type(&self, type_id: &TypeId) -> bool {
        return self.rep_type_to_handler_map.contains_key(type_id);
    }

    pub(crate) fn put_type<R: ImplRef<P>>(&mut self, rep_type_id: &TypeId, ref_type_id: &TypeId) {
        self.rep_type_to_handler_map
            .insert(*rep_type_id, ComponentAccessor::<P, R>::new());
        self.ref_type_to_rep_type_map
            .insert(*ref_type_id, *rep_type_id);
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
