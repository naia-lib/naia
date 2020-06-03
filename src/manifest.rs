
use std::any::{TypeId};
use std::collections::HashMap;

use crate::{NetBase};

pub struct Manifest<T: ManifestType> {
    gaia_id_count: u16,
    gaia_id_map: HashMap<u16, T>,
    type_id_map: HashMap<TypeId, u16>,
}

impl<T: ManifestType> Manifest<T> {
    pub fn new() -> Self {
        Manifest {
            gaia_id_count: 111,
            gaia_id_map: HashMap::new(),
            type_id_map: HashMap::new()
        }
    }

    pub fn register<S: NetBase<T>>(&mut self, some_type: S) {
        let new_gaia_id = self.gaia_id_count;
        self.type_id_map.insert(TypeId::of::<S>(), new_gaia_id);
        self.gaia_id_map.insert(new_gaia_id, some_type.to_type());
        self.gaia_id_count += 1;
    }

    pub fn get_gaia_id<S: NetBase<T>>(&self, _net_base: &S) -> u16 {
        let gaia_id = self.type_id_map.get(&TypeId::of::<S>())
            .expect("hey I should get a TypeId here...");
        return *gaia_id;
    }

    pub fn create_entity(&self, gaia_id: u16) -> Option<T> {
        let entity_entry = self.gaia_id_map.get(&gaia_id);
        match entity_entry {
            Some(entity_type) => {
                return (*entity_type).optional_clone();
            }
            None => {}
        }

        return None;
    }

    pub fn process(&mut self) {

    }
}

pub trait ManifestType {
    fn optional_clone(&self) -> Option<Self> where Self: Sized;
    fn is_event(&self) -> bool;
    fn use_bytes(&mut self, bytes: &[u8]);
}