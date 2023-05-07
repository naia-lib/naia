use std::{collections::HashMap, hash::Hash};
use crate::HostType;

use crate::world::delegation::{
    auth_channel::{EntityAuthAccessor, EntityAuthChannel, EntityAuthMutator},
    entity_auth_status::EntityAuthStatus,
};
use crate::world::delegation::entity_auth_status::HostEntityAuthStatus;

pub struct HostAuthHandler<E: Copy + Eq + Hash> {
    auth_channels: HashMap<E, (EntityAuthMutator, EntityAuthAccessor)>,
}

impl<E: Copy + Eq + Hash> HostAuthHandler<E> {
    pub fn new() -> Self {
        Self {
            auth_channels: HashMap::new(),
        }
    }

    pub fn register_entity(&mut self, host_type: HostType, entity: &E) -> EntityAuthAccessor {
        if self.auth_channels.contains_key(&entity) {
            panic!("Entity cannot register with Server more than once!");
        }

        let (mutator, accessor) = EntityAuthChannel::new_channel(host_type);

        self.auth_channels
            .insert(*entity, (mutator, accessor.clone()));

        accessor
    }

    pub fn deregister_entity(&mut self, entity: &E) {
        self.auth_channels.remove(&entity);
    }

    pub fn get_accessor(&self, entity: &E) -> EntityAuthAccessor {
        let (_, receiver) = self
            .auth_channels
            .get(&entity)
            .expect("Entity must be registered with Server before it can receive messages!");

        receiver.clone()
    }

    pub fn auth_status(&self, entity: &E) -> Option<HostEntityAuthStatus> {
        if let Some((_, receiver)) = self.auth_channels.get(&entity) {
            return Some(receiver.auth_status());
        }

        return None;
    }

    pub fn set_auth_status(&self, entity: &E, auth_status: EntityAuthStatus) {
        let (sender, _) = self
            .auth_channels
            .get(&entity)
            .expect("Entity must be registered with Server before it can be mutated!");

        sender.set_auth_status(auth_status);
    }
}
