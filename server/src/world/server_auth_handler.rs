use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
};

use naia_shared::{
    AuthorityError, EntityAuthAccessor, EntityAuthStatus, GlobalEntity, HostAuthHandler, HostType,
};

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

pub struct ServerAuthHandler {
    host_auth_handler: HostAuthHandler,
    entity_auth_map: HashMap<GlobalEntity, AuthOwner>,
    user_to_entity_map: HashMap<UserKey, HashSet<GlobalEntity>>,
}

impl ServerAuthHandler {
    pub fn new() -> Self {
        Self {
            host_auth_handler: HostAuthHandler::new(),
            entity_auth_map: HashMap::new(),
            user_to_entity_map: HashMap::new(),
        }
    }

    pub fn get_accessor(&self, entity: &GlobalEntity) -> EntityAuthAccessor {
        return self.host_auth_handler.get_accessor(entity);
    }

    pub fn register_entity(&mut self, entity: &GlobalEntity) {
        self.host_auth_handler
            .register_entity(HostType::Server, entity);
        self.entity_auth_map.insert(*entity, AuthOwner::None);
    }

    pub fn deregister_entity(&mut self, entity: &GlobalEntity) {
        self.host_auth_handler.deregister_entity(entity);
        self.entity_auth_map.remove(&entity);
    }

    pub(crate) fn authority_status(&self, entity: &GlobalEntity) -> Option<EntityAuthStatus> {
        self.host_auth_handler
            .auth_status(entity)
            .map(|host_status| host_status.status())
    }

    pub(crate) fn client_request_authority(
        &mut self,
        entity: &GlobalEntity,
        requester: &AuthOwner,
    ) -> Result<(), AuthorityError> {
        let Some(owner) = self.entity_auth_map.get_mut(entity) else {
            return Err(AuthorityError::NotDelegated);
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

            return Ok(());
        } else {
            return Err(AuthorityError::NotAvailable);
        }
    }

    pub(crate) fn client_release_authority(
        &mut self,
        entity: &GlobalEntity,
        releaser: &AuthOwner,
    ) -> Result<(), AuthorityError> {
        let Some(owner) = self.entity_auth_map.get_mut(entity) else {
            return Err(AuthorityError::NotDelegated);
        };

        if owner == releaser {
            let previous_owner = *owner;
            *owner = AuthOwner::None;
            self.release_all_authority(entity, previous_owner);

            return Ok(());
        } else {
            return Err(AuthorityError::NotHolder);
        }
    }

    pub(crate) fn server_take_authority(&mut self, entity: &GlobalEntity) -> Result<AuthOwner, AuthorityError> {
        let Some(owner) = self.entity_auth_map.get_mut(entity) else {
            return Err(AuthorityError::NotDelegated);
        };

        let previous_owner = *owner;
        *owner = AuthOwner::None;
        self.release_all_authority(entity, previous_owner);

        Ok(previous_owner)
    }

    fn release_all_authority(&mut self, entity: &GlobalEntity, owner: AuthOwner) -> bool {
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

    pub(crate) fn user_all_owned_entities(
        &self,
        user_key: &UserKey,
    ) -> Option<&HashSet<GlobalEntity>> {
        if let Some(entities) = self.user_to_entity_map.get(user_key) {
            return Some(entities);
        }
        return None;
    }

    /// Check if a user is the authority holder for a specific entity
    pub(crate) fn user_is_authority_holder(
        &self,
        user_key: &UserKey,
        entity: &GlobalEntity,
    ) -> bool {
        self.entity_auth_map
            .get(entity)
            .map(|owner| *owner == AuthOwner::Client(*user_key))
            .unwrap_or(false)
    }
}
