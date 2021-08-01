use naia_shared::{StateMutator, Ref};

use super::{object_key::object_key::ObjectKey, mut_handler::MutHandler};

pub struct ServerStateMutator {
    key: Option<ObjectKey>,
    mut_handler: Ref<MutHandler>,
}

impl ServerStateMutator {
    pub fn new(mut_handler: &Ref<MutHandler>) -> Self {
        ServerStateMutator {
            key: None,
            mut_handler: mut_handler.clone(),
        }
    }

    pub fn set_object_key(&mut self, key: ObjectKey) {
        self.key = Some(key);
    }
}

impl StateMutator for ServerStateMutator {
    fn mutate(&mut self, property_index: u8) {
        if let Some(key) = self.key {
            self.mut_handler.borrow_mut().mutate(&key, property_index);
        }
    }
}
