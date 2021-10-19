use std::ops::Deref;

use hecs::World;

use naia_shared::{ProtocolType, ProtocolExtractor, Replicate, WorldMutType, WorldRefType};

use super::{entity::Entity, world_data::WorldData};

// WorldProxy

pub trait WorldProxy<'w, 'd, P: ProtocolType> {
    fn proxy(self, data: &'d WorldData<P>) -> WorldRef<'w, 'd, P>;
}

impl<'w, 'd, P: ProtocolType> WorldProxy<'w, 'd, P> for &'w World {
    fn proxy(self, data: &'d WorldData<P>) -> WorldRef<'w, 'd, P> {
        return WorldRef::new(self, data);
    }
}

// WorldProxyMut

pub trait WorldProxyMut<'w, 'd, P: ProtocolType> {
    fn proxy_mut(self, data: &'d mut WorldData<P>) -> WorldMut<'w, 'd, P>;
}

impl<'w, 'd, P: ProtocolType> WorldProxyMut<'w, 'd, P> for &'w mut World {
    fn proxy_mut(self, data: &'d mut WorldData<P>) -> WorldMut<'w, 'd, P> {
        return WorldMut::new(self, data);
    }
}

// WorldRef

pub struct WorldRef<'w, 'd, P: ProtocolType> {
    world: &'w World,
    world_data: &'d WorldData<P>,
}

impl<'w, 'd, P: ProtocolType> WorldRef<'w, 'd, P> {
    pub fn new(world: &'w World, data: &'d WorldData<P>) -> Self {
        WorldRef {
            world,
            world_data: data,
        }
    }
}

impl<'w, 'd, P: ProtocolType> WorldRefType<P, Entity> for WorldRef<'w, 'd, P> {
    fn has_entity(&self, entity: &Entity) -> bool {
        return has_entity(self.world, entity);
    }

    fn entities(&self) -> Vec<Entity> {
        return entities(self.world);
    }

    fn has_component<R: Replicate<P>>(&self, entity: &Entity) -> bool {
        return has_component::<P, R>(self.world, entity);
    }

    fn has_component_of_kind(&self, entity: &Entity, component_kind: &P::Kind) -> bool {
        return has_component_of_kind::<P>(self.world, self.world_data, entity, component_kind);
    }

    fn get_component<R: Replicate<P>>(&self, entity: &Entity) -> Option<&R> {
        return get_component(self.world, entity);
    }

    fn get_component_of_kind(&self, entity: &Entity, component_kind: &P::Kind) -> Option<&P> {
        return get_component_of_kind(&self.world, self.world_data, entity, component_kind);
    }
}

// WorldMut

pub struct WorldMut<'w, 'd, P: ProtocolType> {
    world: &'w mut World,
    world_data: &'d mut WorldData<P>,
}

impl<'w, 'd, P: ProtocolType> WorldMut<'w, 'd, P> {
    pub fn new(world: &'w mut World, data: &'d mut WorldData<P>) -> Self {
        WorldMut {
            world,
            world_data: data,
        }
    }
}

impl<'w, 'd, P: ProtocolType> WorldRefType<P, Entity> for WorldMut<'w, 'd, P> {
    fn has_entity(&self, entity: &Entity) -> bool {
        return has_entity(self.world, entity);
    }

    fn entities(&self) -> Vec<Entity> {
        return entities(self.world);
    }

    fn has_component<R: Replicate<P>>(&self, entity: &Entity) -> bool {
        return has_component::<P, R>(self.world, entity);
    }

    fn has_component_of_kind(&self, entity: &Entity, component_kind: &P::Kind) -> bool {
        return has_component_of_kind::<P>(self.world, self.world_data, entity, component_kind);
    }

    fn get_component<R: Replicate<P>>(&self, entity: &Entity) -> Option<&R> {
        return get_component(self.world, entity);
    }

    fn get_component_of_kind(&self, entity: &Entity, component_kind: &P::Kind) -> Option<&P> {
        return get_component_of_kind(self.world, self.world_data, entity, component_kind);
    }
}

impl<'w, 'd, P: ProtocolType> WorldMutType<P, Entity> for WorldMut<'w, 'd, P> {
    fn get_component_mut<R: Replicate<P>>(&mut self, entity: &Entity) -> Option<&mut R> {
        return self.world
            .get_mut::<R>(**entity)
            .map_or(None, |v| Some(&mut v));
    }

    fn get_component_mut_of_kind(
        &mut self,
        entity: &Entity,
        component_type: &P::Kind,
    ) -> Option<&mut P> {
        if let Some(access) = self.world_data.get_component_access(component_type) {
            return access.get_component_mut(self.world, entity);
        }
        return None;
    }

    fn copy_components(&mut self, entity: &Entity) -> Vec<P> {
        let mut protocols = Vec::new();

        if let Ok(entity_ref) = self.world.entity(**entity) {
            for component_type in entity_ref.component_types() {
                let component_kind = P::type_to_kind(component_type);
                if let Some(component) = self.get_component_of_kind(entity, &component_kind) {
                    protocols.push(component.clone());
                }
            }
        }

        return protocols;
    }

    fn spawn_entity(&mut self) -> Entity {
        let entity = self.world.spawn(());
        return Entity::new(entity);
    }

    fn despawn_entity(&mut self, entity: &Entity) {
        self.world
            .despawn(**entity)
            .expect("error despawning Entity");
    }

    fn insert_component<R: Replicate<P>>(&mut self, entity: &Entity, component_ref: R) {
        // cache type id for later
        // todo: can we initialize this map on startup via Protocol derive?
        let component_kind = component_ref.get_kind();
        if !self.world_data.has_kind(&component_kind) {
            self.world_data.put_kind::<R>(&component_kind);
        }

        self.world
            .insert_one(**entity, component_ref)
            .expect("error inserting Component");
    }

    fn remove_component<R: Replicate<P>>(&mut self, entity: &Entity) -> Option<R> {
        return self.world
            .remove_one::<R>(**entity).ok();
    }

    fn remove_component_of_kind(&mut self, entity: &Entity, component_kind: &P::Kind) -> Option<P> {
        if let Some(accessor) = self.world_data.get_component_access(component_kind) {
            return accessor.remove_component(self.world, entity);
        }
        return None;
    }
}

impl<'w, 'd, P: ProtocolType> ProtocolExtractor<P, Entity> for WorldMut<'w, 'd, P> {
    fn extract<I: Replicate<P>>(&mut self, entity: &Entity, impl_ref: I) {
        self.insert_component::<I>(entity, impl_ref);
    }
}

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
    let result = world.get::<R>(**entity);
    return result.is_ok();
}

fn has_component_of_kind<P: ProtocolType>(
    world: &World,
    world_data: &WorldData<P>,
    entity: &Entity,
    component_kind: &P::Kind,
) -> bool {
    return get_component_of_kind::<P>(world, world_data, entity, component_kind).is_some();
}

fn get_component<'a, P: ProtocolType, R: Replicate<P>>(
    world: &'a World,
    entity: &Entity,
) -> Option<&'a R> {
    return world
        .get::<R>(**entity)
        .map_or(None, |v| Some(v.deref().clone()));
}

fn get_component_of_kind<'a, P: ProtocolType>(
    world: &'a World,
    world_data: &'a WorldData<P>,
    entity: &Entity,
    component_kind: &P::Kind,
) -> Option<&'a P> {
    if let Some(access) = world_data.get_component_access(component_kind) {
        return access.get_component(world, entity);
    }
    return None;
}
