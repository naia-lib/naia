use std::{any::TypeId, collections::HashMap, marker::PhantomData, ops::Deref};

use hecs::{Entity, World as HecsWorld};

use naia_server::{ImplRef, KeyType, ProtocolType, Ref, Replicate, WorldType};

// Key

#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct EntityKey(Entity);

impl EntityKey {
    pub fn new(entity: Entity) -> Self {
        return EntityKey(entity);
    }
}

impl KeyType for EntityKey {}

// Handler
pub struct Handler<P: ProtocolType, R: ImplRef<P>> {
    phantom_p: PhantomData<P>,
    phantom_r: PhantomData<R>,
}

impl<P: ProtocolType, R: ImplRef<P>> Handler<P, R> {
    pub fn new() -> Box<dyn HandlerTrait<P>> {
        Box::new(Handler {
            phantom_p: PhantomData::<P>,
            phantom_r: PhantomData::<R>,
        })
    }
}

pub trait HandlerTrait<P: ProtocolType> {
    fn get_component(&self, world: &World<P>, entity_key: &Entity) -> Option<P>;
}

impl<P: ProtocolType, R: ImplRef<P>> HandlerTrait<P> for Handler<P, R> {
    fn get_component(&self, world: &World<P>, entity_key: &Entity) -> Option<P> {
        if let Ok(component_ref) = world.hecs.get::<R>(*entity_key) {
            return Some(component_ref.deref().protocol());
        }
        return None;
    }
}

// World

pub struct World<P: ProtocolType> {
    pub hecs: HecsWorld,
    rep_type_to_handler_map: HashMap<TypeId, Box<dyn HandlerTrait<P>>>,
    ref_type_to_rep_type_map: HashMap<TypeId, TypeId>,
}

impl<P: ProtocolType> World<P> {
    pub fn new() -> Self {
        World {
            hecs: HecsWorld::new(),
            rep_type_to_handler_map: HashMap::new(),
            ref_type_to_rep_type_map: HashMap::new(),
        }
    }
}

impl<P: ProtocolType> WorldType<P, EntityKey> for World<P> {
    fn has_entity(&self, entity_key: &EntityKey) -> bool {
        return self.hecs.contains(entity_key.0);
    }

    fn entities(&self) -> Vec<EntityKey> {
        let mut output = Vec::new();

        for (entity, _) in self.hecs.iter() {
            output.push(EntityKey(entity));
        }

        return output;
    }

    fn spawn_entity(&mut self) -> EntityKey {
        let entity = self.hecs.spawn(());
        return EntityKey(entity);
    }

    fn despawn_entity(&mut self, entity_key: &EntityKey) {
        self.hecs
            .despawn(entity_key.0)
            .expect("error despawning Entity");
    }

    fn has_component<R: Replicate<P>>(&self, entity_key: &EntityKey) -> bool {
        let result = self.hecs.get::<Ref<R>>(entity_key.0);
        return result.is_ok();
    }

    fn has_component_of_type(&self, entity_key: &EntityKey, type_id: &TypeId) -> bool {
        return WorldType::<P, EntityKey>::get_component_from_type(self, entity_key, type_id)
            .is_some();
    }

    fn get_component<R: Replicate<P>>(&self, entity_key: &EntityKey) -> Option<Ref<R>> {
        return self
            .hecs
            .get::<Ref<R>>(entity_key.0)
            .map_or(None, |v| Some(v.deref().clone()));
    }

    fn get_component_from_type(&self, entity_key: &EntityKey, type_id: &TypeId) -> Option<P> {
        if let Some(handler) = self.rep_type_to_handler_map.get(type_id) {
            return handler.get_component(self, &entity_key.0);
        }
        return None;
    }

    fn get_components(&self, entity_key: &EntityKey) -> Vec<P> {
        let mut protocols = Vec::new();

        if let Ok(entity_ref) = self.hecs.entity(entity_key.0) {
            for ref_type in entity_ref.component_types() {
                if let Some(rep_type) = self.ref_type_to_rep_type_map.get(&ref_type) {
                    if let Some(component) = WorldType::<P, EntityKey>::get_component_from_type(
                        self, entity_key, &rep_type,
                    ) {
                        protocols.push(component);
                    }
                }
            }
        }

        return protocols;
    }

    fn insert_component<R: ImplRef<P>>(&mut self, entity_key: &EntityKey, component_ref: R) {
        // cache type id for later
        // todo: can we initialize this map on startup via Protocol derive?
        let inner_type_id = component_ref.dyn_ref().borrow().get_type_id();
        if !self.rep_type_to_handler_map.contains_key(&inner_type_id) {
            self.rep_type_to_handler_map
                .insert(inner_type_id, Handler::<P, R>::new());
            self.ref_type_to_rep_type_map
                .insert(TypeId::of::<R>(), inner_type_id);
        }

        // insert into ecs
        self.hecs
            .insert_one(entity_key.0, component_ref)
            .expect("error inserting Component");
    }

    fn remove_component<R: Replicate<P>>(&mut self, entity_key: &EntityKey) {
        // remove from ecs
        self.hecs
            .remove_one::<Ref<R>>(entity_key.0)
            .expect("error removing Component");
    }
}
