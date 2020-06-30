use std::{cell::RefCell, rc::Rc};

use naia_shared::EntityMutator;

use super::{entity_key::entity_key::EntityKey, mut_handler::MutHandler};

pub struct ServerEntityMutator {
    key: Option<EntityKey>,
    mut_handler: Rc<RefCell<MutHandler>>,
}

impl ServerEntityMutator {
    pub fn new(mut_handler: &Rc<RefCell<MutHandler>>) -> Self {
        ServerEntityMutator {
            key: None,
            mut_handler: mut_handler.clone(),
        }
    }

    pub fn set_entity_key(&mut self, key: EntityKey) {
        self.key = Some(key);
    }
}

impl EntityMutator for ServerEntityMutator {
    fn mutate(&mut self, property_index: u8) {
        if let Some(key) = self.key {
            self.mut_handler
                .as_ref()
                .borrow_mut()
                .mutate(&key, property_index);
        }
    }
}
