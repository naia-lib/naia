use std::collections::HashMap;

use crate::{
    world::delegation::{
        auth_channel::{EntityAuthAccessor, EntityAuthChannel, EntityAuthMutator},
        entity_auth_status::{EntityAuthStatus, HostEntityAuthStatus},
    },
    GlobalEntity, HostType,
};

/// Server-side registry of per-entity authority channels, tracking which entities are delegated and their current status.
pub struct HostAuthHandler {
    auth_channels: HashMap<GlobalEntity, (EntityAuthMutator, EntityAuthAccessor)>,
}

impl Default for HostAuthHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl HostAuthHandler {
    /// Creates an empty `HostAuthHandler`.
    pub fn new() -> Self {
        Self {
            auth_channels: HashMap::new(),
        }
    }

    /// Registers `entity` with this handler, creating an authority channel for it and returning the accessor.
    pub fn register_entity(
        &mut self,
        host_type: HostType,
        entity: &GlobalEntity,
    ) -> EntityAuthAccessor {
        if self.auth_channels.contains_key(entity) {
            panic!("Entity cannot register with Server more than once!");
        }

        let (mutator, accessor) = EntityAuthChannel::new_channel(host_type);

        self.auth_channels
            .insert(*entity, (mutator, accessor.clone()));

        accessor
    }

    /// Removes `entity`'s authority channel. Called on entity despawn.
    pub fn deregister_entity(&mut self, entity: &GlobalEntity) {
        self.auth_channels.remove(entity);
    }

    /// Returns a cloned `EntityAuthAccessor` for `entity`. Panics if not registered.
    pub fn get_accessor(&self, entity: &GlobalEntity) -> EntityAuthAccessor {
        let (_, receiver) = self
            .auth_channels
            .get(entity)
            .expect("Entity must be registered with Server before it can receive messages!");

        receiver.clone()
    }

    /// Returns the current authority status for `entity`, or `None` if not registered.
    pub fn auth_status(&self, entity: &GlobalEntity) -> Option<HostEntityAuthStatus> {
        if let Some((_, receiver)) = self.auth_channels.get(entity) {
            return Some(receiver.auth_status());
        }

        None
    }

    /// Updates the authority status for `entity`. Panics if not registered.
    pub fn set_auth_status(&self, entity: &GlobalEntity, auth_status: EntityAuthStatus) {
        let (sender, _) = self
            .auth_channels
            .get(entity)
            .expect("Entity must be registered with Server before it can be mutated!");

        sender.set_auth_status(auth_status);
    }
}
