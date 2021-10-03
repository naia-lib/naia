use std::{
    any::TypeId,
    collections::{HashMap, HashSet},
    marker::PhantomData,
    ops::Deref,
};

use bevy::ecs::{
    entity::Entity,
    world::{Mut, World},
};

use naia_server::{ImplRef, KeyType, ProtocolType, Ref, Replicate, WorldType};

// testing...

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
        entity_key: &EntityKey,
    ) -> Option<R> {
        return self
            .world
            .get::<R>(entity_key.0)
            .map_or(None, |v| Some(v.deref().clone_ref()));
    }

    pub(crate) fn get_metadata<P: ProtocolType>(&self) -> Option<&WorldMetadata<P>> {
        return self.world.get_resource();
    }

    pub(crate) fn get_or_init_metadata<P: ProtocolType>(&mut self) -> Mut<WorldMetadata<P>> {
        return self
            .world
            .get_resource_or_insert_with(|| WorldMetadata::<P>::new());
    }
}

impl<'w, P: 'static + ProtocolType> WorldType<P, EntityKey> for WorldAdapter<'w> {
    fn has_entity(&self, entity_key: &EntityKey) -> bool {
        return self.world.get_entity(entity_key.0).is_some();
    }

    fn entities(&self) -> Vec<EntityKey> {
        if let Some(world_metadata) = self.get_metadata::<P>() {
            return world_metadata.get_entities();
        } else {
            return Vec::new();
        }
    }

    fn spawn_entity(&mut self) -> EntityKey {
        let entity = self.world.spawn().id();

        let mut world_metadata: Mut<WorldMetadata<P>> = self.get_or_init_metadata();
        world_metadata.spawn_entity(&entity);

        return EntityKey::new(entity);
    }

    fn despawn_entity(&mut self, entity_key: &EntityKey) {
        let entity = entity_key.0;

        let mut world_metadata: Mut<WorldMetadata<P>> = self.get_or_init_metadata();
        world_metadata.despawn_entity(&entity);

        self.world.despawn(entity);
    }

    fn has_component<R: Replicate<P>>(&self, entity_key: &EntityKey) -> bool {
        return self.world.get::<Ref<R>>(entity_key.0).is_some();
    }

    fn has_component_of_type(&self, entity_key: &EntityKey, type_id: &TypeId) -> bool {
        return self.world.entity(entity_key.0).contains_type_id(*type_id);
    }

    fn get_component<R: Replicate<P>>(&self, entity_key: &EntityKey) -> Option<Ref<R>> {
        return self
            .world
            .get::<Ref<R>>(entity_key.0)
            .map_or(None, |v| Some(v.clone()));
    }

    fn get_component_from_type(&self, entity_key: &EntityKey, type_id: &TypeId) -> Option<P> {
        if let Some(world_metadata) = self.get_metadata() {
            if let Some(handler) = world_metadata.get_handler(type_id) {
                return handler.get_component(self, entity_key);
            }
        }
        return None;
    }

    fn get_components(&self, entity_key: &EntityKey) -> Vec<P> {
        let mut protocols = Vec::new();

        let components = self.world.components();

        for component_id in self.world.entity(entity_key.0).archetype().components() {
            if let Some(component_info) = components.get_info(component_id) {
                if let Some(type_id) = component_info.type_id() {
                    let protocol_opt: Option<P> =
                        self.get_component_from_type(entity_key, &type_id);
                    if protocol_opt.is_some() {
                        protocols.push(protocol_opt.unwrap().clone());
                    }
                }
            }
        }

        return protocols;
    }

    fn insert_component<R: ImplRef<P>>(&mut self, entity_key: &EntityKey, component_ref: R) {
        // cache type id for later
        // todo: can we initialize this map on startup via Protocol derive?
        let mut world_metadata: Mut<WorldMetadata<P>> = self.get_or_init_metadata();
        let inner_type_id = component_ref.dyn_ref().borrow().get_type_id();
        if !world_metadata.has_type(&inner_type_id) {
            world_metadata.put_type::<R>(&inner_type_id, &TypeId::of::<R>());
        }

        // insert into ecs
        self.world.entity_mut(entity_key.0).insert(component_ref);
    }

    fn remove_component<R: Replicate<P>>(&mut self, entity_key: &EntityKey) {
        self.world.entity_mut(entity_key.0).remove::<Ref<R>>();
    }
}

// EntityKey

#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct EntityKey(Entity);

impl EntityKey {
    pub fn new(entity: Entity) -> Self {
        return EntityKey(entity);
    }
}

impl KeyType for EntityKey {}

impl Deref for EntityKey {
    type Target = Entity;
    fn deref(&self) -> &Self::Target {
        return &self.0;
    }
}

// WorldMetadata

pub struct WorldMetadata<P: ProtocolType> {
    entities: HashSet<Entity>,
    rep_type_to_handler_map: HashMap<TypeId, Box<dyn HandlerTrait<P>>>,
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

    pub(crate) fn get_handler(&self, type_id: &TypeId) -> Option<&Box<dyn HandlerTrait<P>>> {
        return self.rep_type_to_handler_map.get(type_id);
    }

    pub(crate) fn has_type(&self, type_id: &TypeId) -> bool {
        return self.rep_type_to_handler_map.contains_key(type_id);
    }

    pub(crate) fn put_type<R: ImplRef<P>>(&mut self, rep_type_id: &TypeId, ref_type_id: &TypeId) {
        self.rep_type_to_handler_map
            .insert(*rep_type_id, Handler::<P, R>::new());
        self.ref_type_to_rep_type_map
            .insert(*ref_type_id, *rep_type_id);
    }

    pub(crate) fn spawn_entity(&mut self, entity: &Entity) {
        self.entities.insert(*entity);
    }

    pub(crate) fn despawn_entity(&mut self, entity: &Entity) {
        self.entities.remove(&entity);
    }

    pub(crate) fn get_entities(&self) -> Vec<EntityKey> {
        let mut output = Vec::new();

        for entity in &self.entities {
            output.push(EntityKey(*entity));
        }

        return output;
    }
}

// Handler
pub struct Handler<P: ProtocolType, R: ImplRef<P>> {
    phantom_p: PhantomData<P>,
    phantom_r: PhantomData<R>,
}

impl<P: 'static + ProtocolType, R: ImplRef<P>> Handler<P, R> {
    pub fn new() -> Box<dyn HandlerTrait<P>> {
        Box::new(Handler {
            phantom_p: PhantomData::<P>,
            phantom_r: PhantomData::<R>,
        })
    }
}

pub trait HandlerTrait<P: ProtocolType>: Send + Sync {
    fn get_component(&self, world: &WorldAdapter, entity_key: &EntityKey) -> Option<P>;
}

impl<P: ProtocolType, R: ImplRef<P>> HandlerTrait<P> for Handler<P, R> {
    fn get_component(&self, world: &WorldAdapter, entity_key: &EntityKey) -> Option<P> {
        if let Some(component_ref) = world.get_component_ref::<P, R>(entity_key) {
            return Some(component_ref.protocol());
        }
        return None;
    }
}