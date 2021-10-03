use std::{any::TypeId, ops::Deref};

use bevy::ecs::world::{Mut, World};

use naia_server::{ImplRef, ProtocolType, Ref, Replicate, WorldType};

use super::{entity::Entity, world_data::WorldData};

// WorldAdapt

pub trait WorldAdapt<'w> {
    fn adapt(self) -> WorldAdapter<'w>;
}

impl<'w> WorldAdapt<'w> for &'w mut World {
    fn adapt(self) -> WorldAdapter<'w> {
        return WorldAdapter::new(self);
    }
}

// WorldAdapter

pub struct WorldAdapter<'w> {
    world: &'w mut World,
}

impl<'w> WorldAdapter<'w> {
    pub fn new(world: &'w mut World) -> Self {
        WorldAdapter { world }
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

    pub(crate) fn get_world_data<P: ProtocolType>(&self) -> Option<&WorldData<P>> {
        return self.world.get_resource();
    }

    pub(crate) fn get_or_init_world_data<P: ProtocolType>(&mut self) -> Mut<WorldData<P>> {
        return self
            .world
            .get_resource_or_insert_with(|| WorldData::<P>::new());
    }
}

impl<'w, P: 'static + ProtocolType> WorldType<P, Entity> for WorldAdapter<'w> {
    fn has_entity(&self, entity: &Entity) -> bool {
        return self.world.get_entity(**entity).is_some();
    }

    fn entities(&self) -> Vec<Entity> {
        if let Some(world_data) = self.get_world_data::<P>() {
            return world_data.get_entities();
        } else {
            return Vec::new();
        }
    }

    fn spawn_entity(&mut self) -> Entity {
        let entity = Entity::new(self.world.spawn().id());

        let mut world_data: Mut<WorldData<P>> = self.get_or_init_world_data();
        world_data.spawn_entity(&entity);

        return entity;
    }

    fn despawn_entity(&mut self, entity: &Entity) {
        let mut world_data: Mut<WorldData<P>> = self.get_or_init_world_data();
        world_data.despawn_entity(entity);

        self.world.despawn(**entity);
    }

    fn has_component<R: Replicate<P>>(&self, entity: &Entity) -> bool {
        return self.world.get::<Ref<R>>(**entity).is_some();
    }

    fn has_component_of_type(&self, entity: &Entity, type_id: &TypeId) -> bool {
        return self.world.entity(**entity).contains_type_id(*type_id);
    }

    fn get_component<R: Replicate<P>>(&self, entity: &Entity) -> Option<Ref<R>> {
        return self
            .world
            .get::<Ref<R>>(**entity)
            .map_or(None, |v| Some(v.clone()));
    }

    fn get_component_from_type(&self, entity: &Entity, type_id: &TypeId) -> Option<P> {
        if let Some(world_data) = self.get_world_data() {
            return world_data.get_component(self, entity, type_id);
        }
        return None;
    }

    fn get_components(&self, entity: &Entity) -> Vec<P> {
        let mut protocols = Vec::new();

        let components = self.world.components();

        for component_id in self.world.entity(**entity).archetype().components() {
            if let Some(component_info) = components.get_info(component_id) {
                if let Some(type_id) = component_info.type_id() {
                    let protocol_opt: Option<P> = self.get_component_from_type(entity, &type_id);
                    if protocol_opt.is_some() {
                        protocols.push(protocol_opt.unwrap().clone());
                    }
                }
            }
        }

        return protocols;
    }

    fn insert_component<R: ImplRef<P>>(&mut self, entity: &Entity, component_ref: R) {
        // cache type id for later
        // todo: can we initialize this map on startup via Protocol derive?
        let mut world_data: Mut<WorldData<P>> = self.get_or_init_world_data();
        let inner_type_id = component_ref.dyn_ref().borrow().get_type_id();
        if !world_data.has_type(&inner_type_id) {
            world_data.put_type::<R>(&inner_type_id, &TypeId::of::<R>());
        }

        // insert into ecs
        self.world.entity_mut(**entity).insert(component_ref);
    }

    fn remove_component<R: Replicate<P>>(&mut self, entity: &Entity) {
        self.world.entity_mut(**entity).remove::<Ref<R>>();
    }
}
