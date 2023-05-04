use std::{collections::HashMap, hash::Hash};

use naia_shared::{EntityAuthAccessor, EntityAuthStatus, HostAuthHandler};

use crate::UserKey;

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum AuthOwner {
    None,
    Server,
    Client(UserKey),
}

impl AuthOwner {
    pub fn from_user_key(user_key: &Option<UserKey>) -> Self {
        match user_key {
            Some(user_key) => AuthOwner::Client(*user_key),
            None => AuthOwner::Server,
        }
    }
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

    pub(crate) fn request_authority(&mut self, entity: &E, requester: &AuthOwner) -> bool {
        if let Some(owner) = self.entity_auth_map.get_mut(entity) {
            if *owner == AuthOwner::None {
                *owner = requester.clone();
                if requester == &AuthOwner::Server {
                    self.host_auth_handler
                        .set_auth_status(entity, EntityAuthStatus::Granted);
                } else {
                    self.host_auth_handler
                        .set_auth_status(entity, EntityAuthStatus::Denied);
                }

                return true;
            }
        }
        return false;
    }

    pub(crate) fn release_authority(&mut self, entity: &E, releaser: &AuthOwner) -> bool {
        if let Some(owner) = self.entity_auth_map.get_mut(entity) {
            if owner == releaser {
                *owner = AuthOwner::None;
                self.host_auth_handler
                    .set_auth_status(entity, EntityAuthStatus::Available);
                return true;
            }
        }
        return false;
    }
}
