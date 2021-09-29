use naia_shared::{PropertyMutate, Ref};

use super::{keys::ComponentKey, mut_handler::MutHandler};

pub struct PropertyMutator {
    key: Option<ComponentKey>,
    mut_handler: Ref<MutHandler>,
}

impl PropertyMutator {
    pub fn new(mut_handler: &Ref<MutHandler>) -> Self {
        PropertyMutator {
            key: None,
            mut_handler: mut_handler.clone(),
        }
    }

    pub fn set_component_key(&mut self, key: ComponentKey) {
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
