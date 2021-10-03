
use std::{any::TypeId, collections::HashMap, marker::PhantomData, ops::Deref};

use bevy::{
    ecs::{entity::Entity, world::{World, Mut}, component::StorageType},
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

pub struct WorldMetadata<P: ProtocolType> {
    rep_type_to_handler_map: HashMap<TypeId, Box<dyn HandlerTrait<P>>>,
    ref_type_to_rep_type_map: HashMap<TypeId, TypeId>,
}

impl<P: ProtocolType> WorldMetadata<P> {
    pub fn new() -> Self {
        WorldMetadata {
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
    fn get_component(&self, world: &WorldRef, entity_key: &EntityKey) -> Option<P>;
}

impl<P: ProtocolType, R: ImplRef<P>> HandlerTrait<P> for Handler<P, R> {
    fn get_component(&self, world: &WorldRef, entity_key: &EntityKey) -> Option<P> {
        if let Some(component_ref) = world.get_component_ref::<P, R>(entity_key) {
            return Some(component_ref.protocol());
        }
        return None;
    }
}

// WorldRef

pub struct WorldRef<'w> {
    world: &'w mut World,
}

impl<'w> WorldRef<'w> {
    pub fn new(world: &'w mut World,) -> Self {
        WorldRef {
            world,
        }
    }

    pub(crate) fn get_component_ref<P: ProtocolType, R: ImplRef<P>>(&self, entity_key: &EntityKey) -> Option<R> {
        return self
            .world
            .get::<R>(entity_key.0)
            .map_or(None, |v| Some(v.deref().clone_ref()));
    }

    pub(crate) fn get_metadata<P: ProtocolType>(&self) -> Option<&WorldMetadata<P>> {
        return self.world.get_resource();
    }

    pub(crate) fn get_or_init_metadata<P: ProtocolType>(&mut self) -> Mut<WorldMetadata<P>> {
        return self.world.get_resource_or_insert_with(|| WorldMetadata::<P>::new());
    }
}

impl<'w, P: 'static + ProtocolType> WorldType<P, EntityKey> for WorldRef<'w> {

    fn has_entity(&self, entity_key: &EntityKey) -> bool {
        return self.world.get_entity(entity_key.0).is_some();
    }

    fn entities(&self) -> Vec<EntityKey> {
        unimplemented!()
    }

    fn spawn_entity(&mut self) -> EntityKey {
        return EntityKey::new(self.world.spawn().id());
    }

    fn despawn_entity(&mut self, entity_key: &EntityKey) {
        self.world.despawn(entity_key.0);
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
        unimplemented!()
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