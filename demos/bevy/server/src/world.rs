
use std::any::TypeId;

use bevy::{
    ecs::{entity::Entity, world::World},
};

use naia_server::{ProtocolType, WorldType, Replicate, ImplRef, Ref, KeyType};

// Key

#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct Key(Entity);

impl Key {
    pub fn new(entity: Entity) -> Self {
        return Key(entity);
    }
}

impl KeyType for Key {}

// WorldMetadata

pub struct WorldMetadata {

}

impl WorldMetadata {
    pub fn new() -> Self {
        WorldMetadata {

        }
    }
}

// WorldRef

pub struct WorldRef {
    world: &World,
}

impl WorldRef {
    pub fn new(world: &World) -> Self {
        WorldRef {
            world,
        }
    }
}

impl<P: 'static + ProtocolType> WorldType<P> for WorldRef {
    type EntityKey = Key;

    fn has_entity(&self, entity_key: &Self::EntityKey) -> bool {
        unimplemented!()
    }

    fn entities(&self) -> Vec<Self::EntityKey> {
        unimplemented!()
    }

    fn spawn_entity(&mut self) -> Self::EntityKey {
        unimplemented!()
    }

    fn despawn_entity(&mut self, entity_key: &Self::EntityKey) {
        unimplemented!()
    }

    fn has_component<R: Replicate<P>>(&self, entity_key: &Self::EntityKey) -> bool {
        unimplemented!()
    }

    fn has_component_of_type(&self, entity_key: &Self::EntityKey, type_id: &TypeId) -> bool {
        unimplemented!()
    }

    fn get_component<R: Replicate<P>>(&self, entity_key: &Self::EntityKey) -> Option<Ref<R>> {
        unimplemented!()
    }

    fn get_component_from_type(&self, entity_key: &Self::EntityKey, type_id: &TypeId) -> Option<P> {
        unimplemented!()
    }

    fn get_components(&self, entity_key: &Self::EntityKey) -> Vec<P> {
        unimplemented!()
    }

    fn insert_component<R: ImplRef<P>>(&mut self, entity_key: &Self::EntityKey, component_ref: R) {
        unimplemented!()
    }

    fn remove_component<R: Replicate<P>>(&mut self, entity_key: &Self::EntityKey) {
        unimplemented!()
    }
}