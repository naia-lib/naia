use hecs::{Component as HecsComponent, Ref as HecsRef, RefMut as HecsMut, World};

use naia_shared::{
    ComponentDynMut, ComponentDynRef, ComponentMut, ComponentMutTrait, ComponentRef,
    ComponentRefTrait, ProtocolInserter, ProtocolType, Replicate, ReplicateSafe, WorldMutType,
    WorldRefType,
};

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

// ComponentRef
struct RefWrapper<'a, T: HecsComponent>(HecsRef<'a, T>);

impl<'a, P: ProtocolType, R: ReplicateSafe<P>> ComponentRefTrait<P, R> for RefWrapper<'a, R> {
    fn component_deref(&self) -> &R {
        return &self.0;
    }
}

// ComponentMut
struct MutWrapper<'a, T: HecsComponent>(HecsMut<'a, T>);

impl<'a, P: ProtocolType, R: ReplicateSafe<P>> ComponentRefTrait<P, R> for MutWrapper<'a, R> {
    fn component_deref(&self) -> &R {
        return &self.0;
    }
}

impl<'a, P: ProtocolType, R: ReplicateSafe<P>> ComponentMutTrait<P, R> for MutWrapper<'a, R> {
    fn component_deref_mut(&mut self) -> &mut R {
        return &mut self.0;
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

    fn has_component<R: ReplicateSafe<P>>(&self, entity: &Entity) -> bool {
        return has_component::<P, R>(self.world, entity);
    }

    fn has_component_of_kind(&self, entity: &Entity, component_kind: &P::Kind) -> bool {
        return has_component_of_kind::<P>(self.world, self.world_data, entity, component_kind);
    }

    fn get_component<'a, R: ReplicateSafe<P>>(
        &'a self,
        entity: &Entity,
    ) -> Option<ComponentRef<'a, P, R>> {
        return get_component::<P, R>(self.world, entity);
    }

    fn get_component_of_kind(
        &self,
        entity: &Entity,
        component_kind: &P::Kind,
    ) -> Option<ComponentDynRef<'_, P>> {
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

    fn has_component<R: ReplicateSafe<P>>(&self, entity: &Entity) -> bool {
        return has_component::<P, R>(self.world, entity);
    }

    fn has_component_of_kind(&self, entity: &Entity, component_kind: &P::Kind) -> bool {
        return has_component_of_kind::<P>(self.world, self.world_data, entity, component_kind);
    }

    fn get_component<'a, R: ReplicateSafe<P>>(
        &'a self,
        entity: &Entity,
    ) -> Option<ComponentRef<'a, P, R>> {
        return get_component::<P, R>(self.world, entity);
    }

    fn get_component_of_kind(
        &self,
        entity: &Entity,
        component_kind: &P::Kind,
    ) -> Option<ComponentDynRef<'_, P>> {
        return get_component_of_kind(self.world, self.world_data, entity, component_kind);
    }
}

impl<'w, 'd, P: ProtocolType> WorldMutType<P, Entity> for WorldMut<'w, 'd, P> {
    fn get_component_mut<'a, R: ReplicateSafe<P>>(
        &'a mut self,
        entity: &Entity,
    ) -> Option<ComponentMut<'a, P, R>> {
        if let Ok(hecs_mut) = self.world.get_mut::<R>(**entity) {
            let wrapper = MutWrapper(hecs_mut);
            let component_mut = ComponentMut::new(wrapper);
            return Some(component_mut);
        }
        return None;
    }

    fn get_component_mut_of_kind(
        &mut self,
        entity: &Entity,
        component_type: &P::Kind,
    ) -> Option<ComponentDynMut<'_, P>> {
        if let Some(access) = self.world_data.get_component_access(component_type) {
            return access.get_component_mut(self.world, entity);
        }
        return None;
    }

    fn get_component_kinds(&mut self, entity: &Entity) -> Vec<P::Kind> {
        let mut kinds = Vec::new();

        if let Ok(entity_ref) = self.world.entity(**entity) {
            for component_type in entity_ref.component_types() {
                let component_kind = P::type_to_kind(component_type);
                kinds.push(component_kind);
            }
        }

        return kinds;
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

    fn insert_component<R: ReplicateSafe<P>>(&mut self, entity: &Entity, component_ref: R) {
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
        return self.world.remove_one::<R>(**entity).ok();
    }

    fn remove_component_of_kind(&mut self, entity: &Entity, component_kind: &P::Kind) -> Option<P> {
        if let Some(accessor) = self.world_data.get_component_access(component_kind) {
            return accessor.remove_component(self.world, entity);
        }
        return None;
    }
}

impl<'w, 'd, P: ProtocolType> ProtocolInserter<P, Entity> for WorldMut<'w, 'd, P> {
    fn insert<I: ReplicateSafe<P>>(&mut self, entity: &Entity, impl_ref: I) {
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

fn has_component<P: ProtocolType, R: ReplicateSafe<P>>(world: &World, entity: &Entity) -> bool {
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

fn get_component<'a, P: ProtocolType, R: ReplicateSafe<P>>(
    world: &'a World,
    entity: &Entity,
) -> Option<ComponentRef<'a, P, R>> {
    if let Ok(hecs_ref) = world.get::<R>(**entity) {
        let wrapper = RefWrapper(hecs_ref);
        let component_ref = ComponentRef::new(wrapper);
        return Some(component_ref);
    }
    return None;
}

fn get_component_of_kind<'a, P: ProtocolType>(
    world: &'a World,
    world_data: &WorldData<P>,
    entity: &Entity,
    component_kind: &P::Kind,
) -> Option<ComponentDynRef<'a, P>> {
    if let Some(access) = world_data.get_component_access(component_kind) {
        return access.get_component(world, entity);
    }
    return None;
}
