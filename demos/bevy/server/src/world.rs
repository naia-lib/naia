
use std::any::TypeId;

use bevy::{
    ecs::{entity::Entity, world::World},
};

use naia_server::{ProtocolType, WorldType, Replicate, ImplRef, Ref, KeyType};

// Key

#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct EntityKey(Entity);

impl EntityKey {
    pub fn new(entity: Entity) -> Self {
        return EntityKey(entity);
    }
}

impl KeyType for EntityKey {}

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

pub struct WorldRef<'w> {
    world: &'w World,
}

impl<'w> WorldRef<'w> {
    pub fn new(world: &'w World) -> Self {
        WorldRef {
            world,
        }
    }
}

impl<'w, P: 'static + ProtocolType> WorldType<P, EntityKey> for WorldRef<'w> {

    fn has_entity(&self, entity_key: &EntityKey) -> bool {
        unimplemented!()
    }

    fn entities(&self) -> Vec<EntityKey> {
        unimplemented!()
    }

    fn spawn_entity(&mut self) -> EntityKey {
        unimplemented!()
    }

    fn despawn_entity(&mut self, entity_key: &EntityKey) {
        unimplemented!()
    }

    fn has_component<R: Replicate<P>>(&self, entity_key: &EntityKey) -> bool {
        unimplemented!()
    }

    fn has_component_of_type(&self, entity_key: &EntityKey, type_id: &TypeId) -> bool {
        unimplemented!()
    }

    fn get_component<R: Replicate<P>>(&self, entity_key: &EntityKey) -> Option<Ref<R>> {
        unimplemented!()
    }

    fn get_component_from_type(&self, entity_key: &EntityKey, type_id: &TypeId) -> Option<P> {
        unimplemented!()
    }

    fn get_components(&self, entity_key: &EntityKey) -> Vec<P> {
        unimplemented!()
    }

    fn insert_component<R: ImplRef<P>>(&mut self, entity_key: &EntityKey, component_ref: R) {
        unimplemented!()
    }

    fn remove_component<R: Replicate<P>>(&mut self, entity_key: &EntityKey) {
        unimplemented!()
    }
}