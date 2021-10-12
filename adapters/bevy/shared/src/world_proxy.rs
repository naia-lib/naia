use std::any::TypeId;

use bevy::ecs::world::{Mut, World};

use naia_shared::{
    ImplRef, ProtocolRefExtractor, ProtocolType, Ref, Replicate, WorldMutType, WorldRefType,
};

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
}

// WorldMut

pub struct WorldMut<'w> {
    world: &'w mut World,
}

impl<'w> WorldMut<'w> {
    pub fn new(world: &'w mut World) -> Self {
        WorldMut { world }
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
}

impl<'w, P: 'static + ProtocolType> WorldMutType<P, Entity> for WorldMut<'w> {
    fn spawn_entity(&mut self) -> Entity {
        let entity = Entity::new(self.world.spawn().id());

        let mut world_data = get_world_data_unchecked_mut::<P>(&mut self.world);
        world_data.spawn_entity(&entity);

        return entity;
    }

    fn despawn_entity(&mut self, entity: &Entity) {
        let mut world_data = get_world_data_unchecked_mut::<P>(&self.world);
        world_data.despawn_entity(entity);

        self.world.despawn(**entity);
    }

    fn get_components(&mut self, entity: &Entity) -> Vec<P> {
        return get_components(self.world, entity);
    }

    fn insert_component<I: ImplRef<P>>(&mut self, entity: &Entity, component_ref: I) {
        // cache type id for later
        // todo: can we initialize this map on startup via Protocol derive?
        let mut world_data = get_world_data_unchecked_mut(&self.world);
        let inner_type_id = component_ref.dyn_ref().borrow().get_type_id();
        if !world_data.has_type(&inner_type_id) {
            world_data.put_type::<I>(&inner_type_id, &TypeId::of::<I>());
        }

        // insert into ecs
        self.world.entity_mut(**entity).insert(component_ref);
    }

    fn remove_component<R: Replicate<P>>(&mut self, entity: &Entity) {
        self.world.entity_mut(**entity).remove::<Ref<R>>();
    }

    fn remove_component_by_type(&mut self, entity: &Entity, type_id: &TypeId) {
        self.world.resource_scope(|world: &mut World, data: Mut<WorldData<P>>| {
            if let Some(accessor) = data.get_component_access(type_id) {
                accessor.remove_component(world, entity);
            }
        });
    }
}

impl<'w, P: ProtocolType> ProtocolRefExtractor<P, Entity> for WorldMut<'w> {
    fn extract<I: ImplRef<P>>(&mut self, entity: &Entity, impl_ref: I) {
        self.insert_component::<I>(entity, impl_ref);
    }
}

// private static methods

fn has_entity(world: &World, entity: &Entity) -> bool {
    return world.get_entity(**entity).is_some();
}

fn entities<P: ProtocolType>(world: &World) -> Vec<Entity> {
    let world_data = get_world_data::<P>(world);
    return world_data.get_entities();
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
    let world_data = get_world_data(world);
    if let Some(component_access) = world_data.get_component_access(type_id) {
        return component_access.get_component(world, entity);
    }
    return None;
}

fn get_components<P: ProtocolType>(world: &mut World, entity: &Entity) -> Vec<P> {
    let mut protocols = Vec::new();

    let components = world.components();
    let world_data = get_world_data::<P>(world);

    for component_id in world.entity(**entity).archetype().components() {
        let ref_type = {
            let component_info = components
                .get_info(component_id)
                .expect("Components need info to instantiate");
            let ref_type = component_info
                .type_id()
                .expect("Components need type_id to instantiate");
            ref_type
        };

        if let Some(rep_type) = world_data.type_convert_ref_to_rep(&ref_type) {
            let protocol: P = get_component_from_type(world, entity, &rep_type).expect(
                "Need to be able to extract the protocol from the component to instantiate",
            );
            protocols.push(protocol.clone());
        }
    }

    return protocols;
}

fn get_world_data<P: ProtocolType>(world: &World) -> &WorldData<P> {
    return world
        .get_resource::<WorldData<P>>()
        .expect("Need to instantiate by adding WorldData<Protocol> resource at startup!");
}

fn get_world_data_unchecked_mut<P: ProtocolType>(world: &World) -> Mut<WorldData<P>> {
    unsafe {
        return world
            .get_resource_unchecked_mut::<WorldData<P>>()
            .expect("Need to instantiate by adding WorldData<Protocol> resource at startup!");
    }
}
