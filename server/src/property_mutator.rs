use naia_shared::{PropertyMutate, Ref};

use super::{keys::replica_key::ReplicaKey, mut_handler::MutHandler};

pub struct PropertyMutator {
    key: Option<ReplicaKey>,
    mut_handler: Ref<MutHandler>,
}

impl PropertyMutator {
    pub fn new(mut_handler: &Ref<MutHandler>) -> Self {
        PropertyMutator {
            key: None,
            mut_handler: mut_handler.clone(),
        }
    }

    pub fn set_object_key(&mut self, key: ReplicaKey) {
        self.key = Some(key);
    }
}

impl PropertyMutate for PropertyMutator {
    fn mutate(&mut self, property_index: u8) {
        if let Some(key) = self.key {
            self.mut_handler.borrow_mut().mutate(&key, property_index);
        }
    }
}
