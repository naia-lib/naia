
use std::collections::HashMap;

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
}

impl WorldType for World {
    fn spawn_entity(&mut self, naia_entity_key: &NaiaEntityKey) {
        let hecs_entity_key = self.hecs.spawn(());

        self.naia_to_hecs_key_map.insert(*naia_entity_key, hecs_entity_key);
        self.hecs_to_naia_key_map.insert(hecs_entity_key, *naia_entity_key);
    }

    fn insert_component<P: ProtocolType, R: ImplRef<P>>(
        &mut self,
        naia_entity_key: &NaiaEntityKey,
        component_ref: R,
    ) {
        let hecs_entity_key = self.naia_to_hecs_key_map.get(naia_entity_key).expect("EntityKey not initialized correctly!");
        self.hecs.insert_one(*hecs_entity_key, component_ref).expect("Entity does not exist?");
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