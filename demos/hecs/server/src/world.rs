
use std::{collections::HashMap, ops::Deref};

use hecs::{Entity as HecsEntityKey, World as HecsWorld};

use naia_server::{WorldType, EntityKey as NaiaEntityKey, ProtocolType, ImplRef, Ref, Replicate};

pub struct World {
    pub hecs: HecsWorld,
    naia_to_hecs_key_map: HashMap<NaiaEntityKey, HecsEntityKey>,
    hecs_to_naia_key_map: HashMap<HecsEntityKey, NaiaEntityKey>,
}

impl World {
    pub fn new() -> Self {
        World {
            hecs: HecsWorld::new(),
            naia_to_hecs_key_map: HashMap::new(),
            hecs_to_naia_key_map: HashMap::new(),
        }
    }

    pub fn naia_to_hecs_key(&self, key: &NaiaEntityKey) -> HecsEntityKey {
        return *self.naia_to_hecs_key_map.get(key).expect("nonexistant key!");
    }

    pub fn hecs_to_naia_key(&self, key: &HecsEntityKey) -> NaiaEntityKey {
        return *self.hecs_to_naia_key_map.get(key).expect("nonexistant key!");
    }
}

impl WorldType for World {
    fn spawn_entity(&mut self, naia_entity_key: &NaiaEntityKey) {
        let hecs_entity_key = self.hecs.spawn(());

        self.naia_to_hecs_key_map.insert(*naia_entity_key, hecs_entity_key);
        self.hecs_to_naia_key_map.insert(hecs_entity_key, *naia_entity_key);
    }

    fn despawn_entity(&mut self, naia_entity_key: &NaiaEntityKey) {
        let hecs_entity_key = self
            .naia_to_hecs_key_map
            .remove(naia_entity_key)
            .expect("EntityKey not initialized correctly!");

        self.hecs_to_naia_key_map.remove(&hecs_entity_key)
            .expect("Entity Keys not initialized correctly!");

        self.hecs.despawn(hecs_entity_key)
            .expect("Hecs entity not initialized correctly!");
    }

    fn has_component<P: ProtocolType, R: Replicate<P>>(&self, naia_entity_key: &NaiaEntityKey) -> bool {
        let hecs_entity_key = self
            .naia_to_hecs_key_map
            .get(naia_entity_key)
            .expect("EntityKey not initialized correctly!");

        let result = self.hecs.get::<Ref<R>>(*hecs_entity_key);

        return result.is_ok();
    }

    fn component<P: ProtocolType, R: Replicate<P>>(&self, naia_entity_key: &NaiaEntityKey) -> Option<Ref<R>> {
        let hecs_entity_key = self
            .naia_to_hecs_key_map
            .get(naia_entity_key)
            .expect("EntityKey not initialized correctly!");

        return self.hecs.get::<Ref<R>>(*hecs_entity_key).map_or(None, |v| Some(v.deref().clone()));
    }

    fn insert_component<P: ProtocolType, R: ImplRef<P>>(
        &mut self,
        naia_entity_key: &NaiaEntityKey,
        component_ref: R,
    ) {
        let hecs_entity_key = self
            .naia_to_hecs_key_map
            .get(naia_entity_key)
            .expect("EntityKey not initialized correctly!");

        self.hecs
            .insert_one(*hecs_entity_key, component_ref)
            .expect("Entity does not exist?");
    }

    fn remove_component<P: ProtocolType, R: Replicate<P>>(
        &mut self,
        naia_entity_key: &NaiaEntityKey,
    ) {
        let hecs_entity_key = self
            .naia_to_hecs_key_map
            .get(naia_entity_key)
            .expect("EntityKey not initialized correctly!");

        self.hecs
            .remove_one::<Ref<R>>(*hecs_entity_key)
            .expect("error removing component");
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