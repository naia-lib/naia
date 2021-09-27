use std::{any::TypeId, ops::Deref};

use hecs::{Entity, World as HecsWorld};

use naia_server::{ImplRef, KeyType, ProtocolType, Ref, Replicate, WorldType};

// Key

#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct Key(Entity);

impl Key {
    pub fn new(entity: Entity) -> Self {
        return Key(entity);
    }
}

impl KeyType for Key {}

// World

pub struct World {
    pub hecs: HecsWorld,
}

impl World {
    pub fn new() -> Self {
        World {
            hecs: HecsWorld::new(),
        }
    }
}

impl<P: ProtocolType> WorldType<P> for World {
    type EntityKey = Key;

    fn has_entity(&self, entity_key: &Self::EntityKey) -> bool {
        return self.hecs.contains(entity_key.0);
    }

    fn entities(&self) -> Vec<Self::EntityKey> {
        let mut output = Vec::new();

        for (entity, _) in self.hecs.iter() {
            output.push(Key(entity));
        }

        return output;
    }

    fn spawn_entity(&mut self) -> Self::EntityKey {
        let entity = self.hecs.spawn(());
        return Key(entity);
    }

    fn despawn_entity(&mut self, entity_key: &Self::EntityKey) {
        self.hecs
            .despawn(entity_key.0)
            .expect("error despawning Entity");
    }

    fn has_component<R: Replicate<P>>(&self, entity_key: &Self::EntityKey) -> bool {
        let result = self.hecs.get::<Ref<R>>(entity_key.0);
        return result.is_ok();
    }

    fn has_component_of_type(&self, entity_key: &Self::EntityKey, type_id: &TypeId) -> bool {
        unimplemented!()
    }

    fn get_component<R: Replicate<P>>(&self, entity_key: &Self::EntityKey) -> Option<Ref<R>> {
        return self
            .hecs
            .get::<Ref<R>>(entity_key.0)
            .map_or(None, |v| Some(v.deref().clone()));
    }

    fn get_component_from_type(&self, entity_key: &Self::EntityKey, type_id: &TypeId) -> Option<P> {
        unimplemented!()
    }

    fn get_components(&self, entity_key: &Self::EntityKey) -> Vec<P> {
        unimplemented!()
    }

    fn insert_component<R: ImplRef<P>>(&mut self, entity_key: &Self::EntityKey, component_ref: R) {
        self.hecs
            .insert_one(entity_key.0, component_ref)
            .expect("error inserting Component");
    }

    fn remove_component<R: Replicate<P>>(&mut self, entity_key: &Self::EntityKey) {
        self.hecs
            .remove_one::<Ref<R>>(entity_key.0)
            .expect("error removing Component");
    }
}