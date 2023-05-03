use std::{collections::HashMap, hash::Hash};

use naia_shared::{EntityAuthAccessor, HostAuthHandler};

use crate::UserKey;

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
enum AuthOwner {
    None,
    Server,
    Client(UserKey),
}

pub struct ServerAuthHandler<E: Copy + Eq + Hash + Send + Sync> {
    host_auth_handler: HostAuthHandler<E>,
    entity_auth_map: HashMap<E, AuthOwner>,
}

impl<E: Copy + Eq + Hash + Send + Sync> ServerAuthHandler<E> {
    pub fn new() -> Self {
        Self {
            host_auth_handler: HostAuthHandler::new(),
            entity_auth_map: HashMap::new(),
        }
    }

    pub fn get_accessor(&self, entity: &E) -> EntityAuthAccessor {
        return self.host_auth_handler.get_accessor(entity);
    }

    pub fn register_entity(&mut self, entity: &E) {
        self.host_auth_handler.register_entity(entity);
        self.entity_auth_map.insert(*entity, AuthOwner::None);
    }

    pub fn deregister_entity(&mut self, entity: &E) {
        self.host_auth_handler.deregister_entity(entity);
        self.entity_auth_map.remove(&entity);
    }
}
