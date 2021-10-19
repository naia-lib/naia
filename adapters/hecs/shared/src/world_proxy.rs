use std::{any::TypeId, ops::Deref};

use hecs::World;

use naia_shared::{ProtocolType, Ref, Replicate, WorldMutType, WorldRefType};

use super::{entity::Entity, world_data::WorldData};

// WorldProxy

pub trait WorldProxy<'w, 'd> {
    fn proxy(self, data: &'d WorldData) -> WorldRef<'w, 'd>;
}

impl<'w, 'd> WorldProxy<'w, 'd> for &'w World {
    fn proxy(self, data: &'d WorldData) -> WorldRef<'w, 'd> {
        return WorldRef::new(self, data);
    }
}

// WorldProxyMut

pub trait WorldProxyMut<'w, 'd> {
    fn proxy_mut(self, data: &'d mut WorldData) -> WorldMut<'w, 'd>;
}

impl<'w, 'd> WorldProxyMut<'w, 'd> for &'w mut World {
    fn proxy_mut(self, data: &'d mut WorldData) -> WorldMut<'w, 'd> {
        return WorldMut::new(self, data);
    }
}

// WorldRef

pub struct WorldRef<'w, 'd> {
    world: &'w World,
    world_data: &'d WorldData,
}

impl<'w, 'd> WorldRef<'w, 'd> {
    pub fn new(world: &'w World, data: &'d WorldData) -> Self {
        WorldRef {
            world,
            world_data: data,
        }
    }
}

impl<'w, 'd, P: 'static + ProtocolType> WorldRefType<P, Entity> for WorldRef<'w, 'd> {
    fn has_entity(&self, entity: &Entity) -> bool {
        return has_entity(self.world, entity);
    }

    fn entities(&self) -> Vec<Entity> {
        return entities(self.world);
    }

    fn has_component<R: Replicate<P>>(&self, entity: &Entity) -> bool {
        return has_component::<P, R>(self.world, entity);
    }

    fn has_component_of_kind(&self, entity: &Entity, type_id: &TypeId) -> bool {
        return has_component_of_type::<P>(self.world, self.world_data, entity, type_id);
    }

    fn get_component<R: Replicate<P>>(&self, entity: &Entity) -> Option<Ref<R>> {
        return get_component(self.world, entity);
    }

    fn get_component_of_kind(&self, entity: &Entity, type_id: &TypeId) -> Option<P> {
        return get_component_from_type(self.world, self.world_data, entity, type_id);
    }
}

// WorldMut

pub struct WorldMut<'w, 'd> {
    world: &'w mut World,
    world_data: &'d mut WorldData,
}

impl<'w, 'd> WorldMut<'w, 'd> {
    pub fn new(world: &'w mut World, data: &'d mut WorldData) -> Self {
        WorldMut {
            world,
            world_data: data,
        }
    }
}

impl<'w, 'd, P: 'static + ProtocolType> WorldRefType<P, Entity> for WorldMut<'w, 'd> {
    fn has_entity(&self, entity: &Entity) -> bool {
        return has_entity(self.world, entity);
    }

    fn entities(&self) -> Vec<Entity> {
        return entities(self.world);
    }

    fn has_component<R: Replicate<P>>(&self, entity: &Entity) -> bool {
        return has_component::<P, R>(self.world, entity);
    }

    fn has_component_of_kind(&self, entity: &Entity, type_id: &TypeId) -> bool {
        return has_component_of_type::<P>(self.world, self.world_data, entity, type_id);
    }

    fn get_component<R: Replicate<P>>(&self, entity: &Entity) -> Option<Ref<R>> {
        return get_component(self.world, entity);
    }

    fn get_component_of_kind(&self, entity: &Entity, type_id: &TypeId) -> Option<P> {
        return get_component_from_type(self.world, self.world_data, entity, type_id);
    }
}

impl<'w, 'd, P: ProtocolType> WorldMutType<P, Entity> for WorldMut<'w, 'd> {
    fn spawn_entity(&mut self) -> Entity {
        let entity = self.world.spawn(());
        return Entity::new(entity);
    }

    fn despawn_entity(&mut self, entity: &Entity) {
        self.world
            .despawn(**entity)
            .expect("error despawning Entity");
    }

    fn get_components(&mut self, entity: &Entity) -> Vec<P> {
        let mut protocols = Vec::new();

        if let Ok(entity_ref) = self.world.entity(**entity) {
            for ref_type in entity_ref.component_types() {
                if let Some(rep_type) = self.world_data.type_convert_ref_to_rep(&ref_type) {
                    if let Some(component) = self.get_component_of_kind(entity, &rep_type) {
                        protocols.push(component);
                    }
                }
            }
        }

        return protocols;
    }

    fn insert_component<R: Replicate<P>>(&mut self, entity: &Entity, component_ref: R) {
        // cache type id for later
        // todo: can we initialize this map on startup via Protocol derive?
        let inner_type_id = component_ref.dyn_ref().borrow().get_type_id();
        if !self.world_data.has_type(&inner_type_id) {
            self.world_data
                .put_type::<P, R>(&inner_type_id, &TypeId::of::<R>());
        }

        self.world
            .insert_one(**entity, component_ref)
            .expect("error inserting Component");
    }

    fn remove_component<R: Replicate<P>>(&mut self, entity: &Entity) {
        self.world
            .remove_one::<Ref<R>>(**entity)
            .expect("error removing Component");
    }

    fn remove_component_of_kind(&mut self, entity: &Entity, type_id: &TypeId) {
        if let Some(accessor) = self.world_data.get_component_access::<P>(type_id) {
            accessor.remove_component(self.world, entity);
        }
    }
}

//impl<'w, 'd, P: ProtocolType> ProtocolExtractor<P, Entity> for WorldMut<'w,
// 'd> {    fn extract<I: Replicate<P>>(&mut self, entity: &Entity, impl_ref: I)
// {        self.insert_component::<I>(entity, impl_ref);
//    }
//}

// private static methods
fn has_entity(world: &World, entity: &Entity) -> bool {
    return world.contains(**entity);
}

fn entities(world: &World) -> Vec<Entity> {
    let mut output = Vec::new();

    for (entity, _) in world.iter() {
        output.push(Entity::new(entity));
    }

    return output;
}

fn has_component<P: ProtocolType, R: Replicate<P>>(world: &World, entity: &Entity) -> bool {
    let result = world.get::<Ref<R>>(**entity);
    return result.is_ok();
}

fn has_component_of_type<P: ProtocolType>(
    world: &World,
    world_data: &WorldData,
    entity: &Entity,
    type_id: &TypeId,
) -> bool {
    return get_component_from_type::<P>(world, world_data, entity, type_id).is_some();
}

fn get_component<P: ProtocolType, R: Replicate<P>>(
    world: &World,
    entity: &Entity,
) -> Option<Ref<R>> {
    return world
        .get::<Ref<R>>(**entity)
        .map_or(None, |v| Some(v.deref().clone()));
}

fn get_component_from_type<P: ProtocolType>(
    world: &World,
    world_data: &WorldData,
    entity: &Entity,
    type_id: &TypeId,
) -> Option<P> {
    if let Some(access) = world_data.get_component_access(type_id) {
        return access.get_component(world, entity);
    }
    return None;
}
