use std::{ops::Deref, any::TypeId};

use hecs::{Entity, World as HecsWorld};

use naia_server::{ImplRef, ProtocolType, Ref, Replicate, WorldType, KeyType};

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
        self.hecs.despawn(entity_key.0).expect("error despawning Entity");
    }

    fn has_component<R: Replicate<P>>(&self, entity_key: &Self::EntityKey) -> bool {
        let result = self.hecs.get::<Ref<R>>(entity_key.0);
        return result.is_ok();
    }

    fn has_component_dynamic(&self, entity_key: &Self::EntityKey, type_id: &TypeId) -> bool {
        unimplemented!()
    }

    fn get_component<R: Replicate<P>>(&self, entity_key: &Self::EntityKey) -> Option<Ref<R>> {
        return self
            .hecs
            .get::<Ref<R>>(entity_key.0)
            .map_or(None, |v| Some(v.deref().clone()));
    }

    fn get_component_dynamic(&self, entity_key: &Self::EntityKey, type_id: &TypeId) -> Option<P> {
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

//fn hecs_remove_component<R: 'static + Replicate<Protocol>>(
//        &mut self,
//        hecs_entity_key: &HecsEntityKey,
//    ) {
//        self.world.hecs
//            .remove_one::<Ref<R>>(*hecs_entity_key)
//            .expect("error removing component");
//    }
//fn spawn_entity(&mut self, naia_entity_key: &NaiaEntityKey) {
//        let hecs_entity_key = self.hecs.spawn(());
//
////        self.naia_to_hecs_key_map
////            .insert(*naia_entity_key, hecs_entity_key);
////        self.hecs_to_naia_key_map
////            .insert(hecs_entity_key, *naia_entity_key);
//    }
//
//    fn despawn_entity(&mut self, naia_entity_key: &NaiaEntityKey) {
////        let hecs_entity_key = self
////            .naia_to_hecs_key_map
////            .remove(naia_entity_key)
////            .expect("EntityKey not initialized correctly!");
////
////        self.hecs_to_naia_key_map
////            .remove(&hecs_entity_key)
////            .expect("Entity Keys not initialized correctly!");
//
//        self.hecs
//            .despawn(hecs_entity_key)
//            .expect("Hecs entity not initialized correctly!");
//    }
//
//    fn has_component<P: ProtocolType, R: Replicate<P>>(
//        &self,
//        naia_entity_key: &NaiaEntityKey,
//    ) -> bool {
////        let hecs_entity_key = self
////            .naia_to_hecs_key_map
////            .get(naia_entity_key)
////            .expect("EntityKey not initialized correctly!");
//
//        let result = self.hecs.get::<Ref<R>>(*hecs_entity_key);
//
//        return result.is_ok();
//    }
//
//    fn get_component<P: ProtocolType, R: Replicate<P>>(
//        &self,
//        naia_entity_key: &NaiaEntityKey,
//    ) -> Option<Ref<R>> {
////        let hecs_entity_key = self
////            .naia_to_hecs_key_map
////            .get(naia_entity_key)
////            .expect("EntityKey not initialized correctly!");
//
//        return self
//            .hecs
//            .get::<Ref<R>>(*hecs_entity_key)
//            .map_or(None, |v| Some(v.deref().clone()));
//    }
//
//    fn insert_component<P: ProtocolType, R: ImplRef<P>>(
//        &mut self,
//        naia_entity_key: &NaiaEntityKey,
//        component_ref: R,
//    ) {
////        let hecs_entity_key = self
////            .naia_to_hecs_key_map
////            .get(naia_entity_key)
////            .expect("EntityKey not initialized correctly!");
//
//        self.hecs
//            .insert_one(*hecs_entity_key, component_ref)
//            .expect("Entity does not exist?");
//    }
//
//    fn remove_component<P: ProtocolType, R: Replicate<P>>(
//        &mut self,
//        naia_entity_key: &NaiaEntityKey,
//    ) {
////        let hecs_entity_key = self
////            .naia_to_hecs_key_map
////            .get(naia_entity_key)
////            .expect("EntityKey not initialized correctly!");
//
//        self.hecs
//            .remove_one::<Ref<R>>(*hecs_entity_key)
//            .expect("error removing component");
//    }