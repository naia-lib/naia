use std::{any::TypeId, ops::Deref};

use hecs::World;

use naia_server::{ImplRef, ProtocolType, Ref, Replicate, WorldMutType};

use super::{
    entity::Entity,
    world_data::{get_world_data, get_world_data_mut},
};

// WorldProxy

pub trait WorldProxy<'w> {
    fn proxy(self) -> WorldMut<'w>;
}

impl<'w> WorldProxy<'w> for &'w mut World {
    fn proxy(self) -> WorldMut<'w> {
        return WorldMut::new(self);
    }
}

// WorldMut

pub struct WorldMut<'w> {
    world: &'w mut World,
}

impl<'w> WorldMut<'w> {
    pub fn new(world: &'w mut World) -> Self {
        WorldMut { world }
    }

    pub(crate) fn get_component_ref<P: ProtocolType, R: ImplRef<P>>(
        &self,
        entity: &Entity,
    ) -> Option<R> {
        return self
            .world
            .get::<R>(**entity)
            .map_or(None, |v| Some(v.deref().clone_ref()));
    }
}

impl<'w, P: ProtocolType> WorldMutType<P, Entity> for WorldMut<'w> {
    fn has_entity(&self, entity_key: &Entity) -> bool {
        return self.world.contains(**entity_key);
    }

    fn entities(&self) -> Vec<Entity> {
        let mut output = Vec::new();

        for (entity, _) in self.world.iter() {
            output.push(Entity::new(entity));
        }

        return output;
    }

    fn spawn_entity(&mut self) -> Entity {
        let entity = self.world.spawn(());
        return Entity::new(entity);
    }

    fn despawn_entity(&mut self, entity_key: &Entity) {
        self.world
            .despawn(**entity_key)
            .expect("error despawning Entity");
    }

    fn has_component<R: Replicate<P>>(&self, entity_key: &Entity) -> bool {
        let result = self.world.get::<Ref<R>>(**entity_key);
        return result.is_ok();
    }

    fn has_component_of_type(&self, entity_key: &Entity, type_id: &TypeId) -> bool {
        return WorldMutType::<P, Entity>::get_component_from_type(self, entity_key, type_id)
            .is_some();
    }

    fn get_component<R: Replicate<P>>(&self, entity_key: &Entity) -> Option<Ref<R>> {
        return self
            .world
            .get::<Ref<R>>(**entity_key)
            .map_or(None, |v| Some(v.deref().clone()));
    }

    fn get_component_from_type(&self, entity_key: &Entity, type_id: &TypeId) -> Option<P> {
        let world_data_ref = get_world_data();
        let world_data = world_data_ref.lock().unwrap();

        return world_data.get_component(self, entity_key, type_id);
    }

    fn get_components(&self, entity_key: &Entity) -> Vec<P> {
        let world_data_ref = get_world_data();
        let world_data = world_data_ref.lock().unwrap();

        let mut protocols = Vec::new();

        if let Ok(entity_ref) = self.world.entity(**entity_key) {
            for ref_type in entity_ref.component_types() {
                if let Some(rep_type) = world_data.type_convert_ref_to_rep(&ref_type) {
                    if let Some(component) = WorldMutType::<P, Entity>::get_component_from_type(
                        self, entity_key, &rep_type,
                    ) {
                        protocols.push(component);
                    }
                }
            }
        }

        return protocols;
    }

    fn insert_component<R: ImplRef<P>>(&mut self, entity_key: &Entity, component_ref: R) {
        let world_data_ref = get_world_data_mut();
        let world_data = world_data_ref.get_mut().unwrap();

        // cache type id for later
        // todo: can we initialize this map on startup via Protocol derive?
        let inner_type_id = component_ref.dyn_ref().borrow().get_type_id();
        if !world_data.has_type(&inner_type_id) {
            world_data.put_type::<P, R>(&inner_type_id, &TypeId::of::<R>());
        }

        // insert into ecs
        self.world
            .insert_one(**entity_key, component_ref)
            .expect("error inserting Component");
    }

    fn remove_component<R: Replicate<P>>(&mut self, entity_key: &Entity) {
        // remove from ecs
        self.world
            .remove_one::<Ref<R>>(**entity_key)
            .expect("error removing Component");
    }
}
