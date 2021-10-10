use std::any::TypeId;

use bevy::ecs::world::{Mut, World};

use naia_server::{ImplRef, ProtocolType, Ref, Replicate, WorldMutType, WorldRefType};

use super::{entity::Entity, world_data::WorldData};

// WorldProxy

pub trait WorldProxy<'w> {
    fn proxy(self) -> WorldRef<'w>;
}

impl<'w> WorldProxy<'w> for &'w World {
    fn proxy(self) -> WorldRef<'w> {
        return WorldRef::new(self);
    }
}

// WorldProxyMut

pub trait WorldProxyMut<'w> {
    fn proxy_mut(self) -> WorldMut<'w>;
}

impl<'w> WorldProxyMut<'w> for &'w mut World {
    fn proxy_mut(self) -> WorldMut<'w> {
        return WorldMut::new(self);
    }
}

// WorldRef private static methods

fn get_world_data<P: ProtocolType>(world: &World) -> Option<&WorldData<P>> {
    return world.get_resource::<WorldData<P>>();
}

fn has_entity(world: &World, entity: &Entity) -> bool {
    return world.get_entity(**entity).is_some();
}

fn entities<P: ProtocolType>(world: &World) -> Vec<Entity> {
    if let Some(world_data) = get_world_data::<P>(world) {
        return world_data.get_entities();
    } else {
        return Vec::new();
    }
}

fn has_component<P: ProtocolType, R: Replicate<P>>(world: &World, entity: &Entity) -> bool {
    return world.get::<Ref<R>>(**entity).is_some();
}

fn has_component_of_type(world: &World, entity: &Entity, type_id: &TypeId) -> bool {
    return world.entity(**entity).contains_type_id(*type_id);
}

fn get_component<P: ProtocolType, R: Replicate<P>>(
    world: &World,
    entity: &Entity,
) -> Option<Ref<R>> {
    return world
        .get::<Ref<R>>(**entity)
        .map_or(None, |v| Some(v.clone()));
}

fn get_component_from_type<P: ProtocolType>(
    world: &World,
    entity: &Entity,
    type_id: &TypeId,
) -> Option<P> {
    if let Some(world_data) = get_world_data(world) {
        return world_data.get_component(world, entity, type_id);
    }
    return None;
}

fn get_components<P: ProtocolType>(world: &World, entity: &Entity) -> Vec<P> {
    let mut protocols = Vec::new();

    let components = world.components();

    for component_id in world.entity(**entity).archetype().components() {
        if let Some(component_info) = components.get_info(component_id) {
            if let Some(type_id) = component_info.type_id() {
                let protocol_opt: Option<P> = get_component_from_type(world, entity, &type_id);
                if protocol_opt.is_some() {
                    protocols.push(protocol_opt.unwrap().clone());
                }
            }
        }
    }

    return protocols;
}

// Wrapper WorldRef & WorldMut

pub struct WorldRef<'w> {
    world: &'w World,
}

impl<'w> WorldRef<'w> {
    pub fn new(world: &'w World) -> Self {
        WorldRef { world }
    }
}

impl<'w, P: 'static + ProtocolType> WorldRefType<P, Entity> for WorldRef<'w> {
    fn has_entity(&self, entity: &Entity) -> bool {
        return has_entity(self.world, entity);
    }

    fn entities(&self) -> Vec<Entity> {
        return entities::<P>(self.world);
    }

    fn has_component<R: Replicate<P>>(&self, entity: &Entity) -> bool {
        return has_component::<P, R>(self.world, entity);
    }

    fn has_component_of_type(&self, entity: &Entity, type_id: &TypeId) -> bool {
        return has_component_of_type(self.world, entity, type_id);
    }

    fn get_component<R: Replicate<P>>(&self, entity: &Entity) -> Option<Ref<R>> {
        return get_component(self.world, entity);
    }

    fn get_component_from_type(&self, entity: &Entity, type_id: &TypeId) -> Option<P> {
        return get_component_from_type(self.world, entity, type_id);
    }

    fn get_components(&self, entity: &Entity) -> Vec<P> {
        return get_components(self.world, entity);
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

    pub(crate) fn get_or_init_world_data<P: ProtocolType>(&mut self) -> Mut<WorldData<P>> {
        return self
            .world
            .get_resource_or_insert_with(|| WorldData::<P>::new());
    }
}

impl<'w, P: 'static + ProtocolType> WorldRefType<P, Entity> for WorldMut<'w> {
    fn has_entity(&self, entity: &Entity) -> bool {
        return has_entity(self.world, entity);
    }

    fn entities(&self) -> Vec<Entity> {
        return entities::<P>(self.world);
    }

    fn has_component<R: Replicate<P>>(&self, entity: &Entity) -> bool {
        return has_component::<P, R>(self.world, entity);
    }

    fn has_component_of_type(&self, entity: &Entity, type_id: &TypeId) -> bool {
        return has_component_of_type(self.world, entity, type_id);
    }

    fn get_component<R: Replicate<P>>(&self, entity: &Entity) -> Option<Ref<R>> {
        return get_component(self.world, entity);
    }

    fn get_component_from_type(&self, entity: &Entity, type_id: &TypeId) -> Option<P> {
        return get_component_from_type(self.world, entity, type_id);
    }

    fn get_components(&self, entity: &Entity) -> Vec<P> {
        return get_components(self.world, entity);
    }
}

impl<'w, P: 'static + ProtocolType> WorldMutType<P, Entity> for WorldMut<'w> {
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

    fn insert_component<R: ImplRef<P>>(&mut self, entity: &Entity, component_ref: R) {
        // cache type id for later
        // todo: can we initialize this map on startup via Protocol derive?
        let mut world_data: Mut<WorldData<P>> = self.get_or_init_world_data();
        let inner_type_id = component_ref.dyn_ref().borrow().get_type_id();
        if !world_data.has_type(&inner_type_id) {
            world_data.put_type::<R>(&inner_type_id);
        }

        // insert into ecs
        self.world.entity_mut(**entity).insert(component_ref);
    }

    fn remove_component<R: Replicate<P>>(&mut self, entity: &Entity) {
        self.world.entity_mut(**entity).remove::<Ref<R>>();
    }
}
