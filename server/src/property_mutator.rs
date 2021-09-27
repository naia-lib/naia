use naia_shared::{PropertyMutate, Ref, ProtocolType};

use super::{keys::{ComponentKey, KeyType}, mut_handler::MutHandler, world_type::WorldType};

pub struct PropertyMutator<P: ProtocolType, W: WorldType<P>> {
    key: Option<ComponentKey<P, W>>,
    mut_handler: Ref<MutHandler<P, W>>,
}

impl<P: ProtocolType, W: WorldType<P>> PropertyMutator<P, W> {
    pub fn new(mut_handler: &Ref<MutHandler<P, W>>) -> Self {
        PropertyMutator {
            key: None,
            mut_handler: mut_handler.clone(),
        }
    }

    pub fn set_component_key(&mut self, key: ComponentKey<P, W>) {
        self.key = Some(key);
    }
}

impl<P: ProtocolType, W: WorldType<P>> PropertyMutate for PropertyMutator<P, W> {
    fn mutate(&mut self, property_index: u8) {
        if let Some(key) = self.key {
            self.mut_handler.borrow_mut().mutate(&key, property_index);
        }
    }
}
