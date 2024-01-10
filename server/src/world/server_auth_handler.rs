use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
};

use naia_shared::{EntityAuthAccessor, EntityAuthStatus, HostAuthHandler, HostType};

use crate::UserKey;

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum AuthOwner {
    None,
    Server,
    Client(UserKey),
}

impl AuthOwner {
    pub fn from_user_key(user_key: Option<&UserKey>) -> Self {
        match user_key {
            Some(user_key) => AuthOwner::Client(*user_key),
            None => AuthOwner::Server,
        }
    }
}

pub struct ServerAuthHandler<E: Copy + Eq + Hash + Send + Sync> {
    host_auth_handler: HostAuthHandler<E>,
    entity_auth_map: HashMap<E, AuthOwner>,
    user_to_entity_map: HashMap<UserKey, HashSet<E>>,
}

impl<E: Copy + Eq + Hash + Send + Sync> ServerAuthHandler<E> {
    pub fn new() -> Self {
        Self {
            host_auth_handler: HostAuthHandler::new(),
            entity_auth_map: HashMap::new(),
            user_to_entity_map: HashMap::new(),
        }
    }

    pub fn get_accessor(&self, entity: &E) -> EntityAuthAccessor {
        return self.host_auth_handler.get_accessor(entity);
    }

    pub fn register_entity(&mut self, entity: &E) {
        self.host_auth_handler
            .register_entity(HostType::Server, entity);
        self.entity_auth_map.insert(*entity, AuthOwner::None);
    }

    pub fn deregister_entity(&mut self, entity: &E) {
        self.host_auth_handler.deregister_entity(entity);
        self.entity_auth_map.remove(&entity);
    }

    pub(crate) fn authority_status(&self, entity: &E) -> Option<EntityAuthStatus> {
        self.host_auth_handler
            .auth_status(entity)
            .map(|host_status| host_status.status())
    }

    pub(crate) fn client_request_authority(&mut self, entity: &E, requester: &AuthOwner) -> bool {
        let Some(owner) = self.entity_auth_map.get_mut(entity) else {
            panic!("Entity not registered with ServerAuthHandler");
        };
        if *owner == AuthOwner::None {
            match requester {
                AuthOwner::Server => {
                    *owner = AuthOwner::Server;
                    // If the Server is requesting Authority, grant the Server local Authority
                    self.host_auth_handler
                        .set_auth_status(entity, EntityAuthStatus::Granted);
                }
                AuthOwner::Client(user_key) => {
                    *owner = AuthOwner::Client(*user_key);
                    self.user_to_entity_map
                        .entry(*user_key)
                        .or_insert(HashSet::new())
                        .insert(*entity);
                    // If a Client is requesting Authority, restrict the Server's local Authority
                    self.host_auth_handler
                        .set_auth_status(entity, EntityAuthStatus::Denied);
                }
                AuthOwner::None => {}
            }

            return true;
        } else {
            return false;
        }
    }

    pub(crate) fn client_release_authority(&mut self, entity: &E, releaser: &AuthOwner) -> bool {
        let Some(owner) = self.entity_auth_map.get_mut(entity) else {
            panic!("Entity not registered with ServerAuthHandler");
        };

        if owner == releaser {
            let previous_owner = *owner;
            *owner = AuthOwner::None;
            self.release_all_authority(entity, previous_owner);

            return true;
        } else {
            return false;
        }
    }

    // returns whether or not any change needed to be made
    pub(crate) fn server_take_authority(&mut self, entity: &E) -> bool {
        let Some(owner) = self.entity_auth_map.get_mut(entity) else {
            panic!("Entity not registered with ServerAuthHandler");
        };

        let previous_owner = *owner;
        *owner = AuthOwner::None;
        let response = self.release_all_authority(entity, previous_owner);

        response
    }

    fn release_all_authority(&mut self, entity: &E, owner: AuthOwner) -> bool {
        if owner == AuthOwner::None {
            // no change was made
            return false;
        }

        if let AuthOwner::Client(user_key) = owner {
            let mut remove_user = false;
            if let Some(entities) = self.user_to_entity_map.get_mut(&user_key) {
                entities.remove(entity);
                remove_user = true;
            }
            if remove_user {
                self.user_to_entity_map.remove(&user_key);
            }
        }

        self.host_auth_handler
            .set_auth_status(entity, EntityAuthStatus::Available);

        return true;
    }

    pub(crate) fn user_all_owned_entities(&self, user_key: &UserKey) -> Option<&HashSet<E>> {
        if let Some(entities) = self.user_to_entity_map.get(user_key) {
            return Some(entities);
        }
        return None;
    }
}
