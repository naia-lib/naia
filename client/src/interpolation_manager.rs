use std::collections::HashMap;

use naia_shared::{EntityType, Instant, LocalEntityKey};

#[derive(Debug)]
pub struct InterpolationManager<U: EntityType> {
    entity_store: HashMap<LocalEntityKey, U>,
    pawn_store: HashMap<LocalEntityKey, U>,
}

impl<U: EntityType> InterpolationManager<U> {
    pub fn new() -> Self {
        InterpolationManager {
            entity_store: HashMap::new(),
            pawn_store: HashMap::new(),
        }
    }

    pub fn create_interpolation(&mut self, key: &LocalEntityKey) {
        unimplemented!()
    }

    pub fn delete_interpolation(&mut self, key: &LocalEntityKey) {
        unimplemented!()
    }

    pub fn get_interpolation(&self, key: &LocalEntityKey, now: &Instant) -> Option<&U> {
        unimplemented!()
    }

    pub fn create_pawn_interpolation(&mut self, key: &LocalEntityKey) {
        unimplemented!()
    }

    pub fn delete_pawn_interpolation(&mut self, key: &LocalEntityKey) {
        unimplemented!()
    }

    pub fn get_pawn_interpolation(&self, key: &LocalEntityKey, now: &Instant) -> Option<&U> {
        unimplemented!()
    }
}
