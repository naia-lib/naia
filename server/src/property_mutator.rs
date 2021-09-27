use naia_shared::{PropertyMutate, Ref};

use super::{
    keys::{ComponentKey, KeyType},
    mut_handler::MutHandler,
};

pub struct PropertyMutator<K: KeyType> {
    key: Option<ComponentKey<K>>,
    mut_handler: Ref<MutHandler<K>>,
}

impl<K: KeyType> PropertyMutator<K> {
    pub fn new(mut_handler: &Ref<MutHandler<K>>) -> Self {
        PropertyMutator {
            key: None,
            mut_handler: mut_handler.clone(),
        }
    }

    pub fn set_component_key(&mut self, key: ComponentKey<K>) {
        self.key = Some(key);
    }
}

impl<K: KeyType> PropertyMutate for PropertyMutator<K> {
    fn mutate(&mut self, property_index: u8) {
        if let Some(key) = self.key {
            self.mut_handler.borrow_mut().mutate(&key, property_index);
        }
    }
}
