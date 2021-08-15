use naia_shared::{Ref, SharedReplicateMutator};

use super::{keys::replicate_key::ReplicateKey, mut_handler::MutHandler};

pub struct ReplicateMutator {
    key: Option<ReplicateKey>,
    mut_handler: Ref<MutHandler>,
}

impl ReplicateMutator {
    pub fn new(mut_handler: &Ref<MutHandler>) -> Self {
        ReplicateMutator {
            key: None,
            mut_handler: mut_handler.clone(),
        }
    }

    pub fn set_object_key(&mut self, key: ReplicateKey) {
        self.key = Some(key);
    }
}

impl SharedReplicateMutator for ReplicateMutator {
    fn mutate(&mut self, property_index: u8) {
        if let Some(key) = self.key {
            self.mut_handler.borrow_mut().mutate(&key, property_index);
        }
    }
}
