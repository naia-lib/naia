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

#[cfg(test)]
mod tests {
    //! `HostAuthHandler` is the server's per-entity authority registry, reached
    //! from 35 call sites across shared/client/server -- and until now it had
    //! no direct tests at all. A mutation sweep found `deregister_entity` and
    //! `set_auth_status` could both be replaced with `()`, and `auth_status`
    //! could return a constant `None`, without a single test noticing.
    //!
    //! The property that makes those mutants visible is that the accessor
    //! handed out by `register_entity` is a *live view* of shared state, not a
    //! snapshot: a write through the handler must be readable through an
    //! accessor obtained before the write.

    use super::*;
    use crate::BigMapKey;

    fn entity(raw: u64) -> GlobalEntity {
        GlobalEntity::from_u64(raw)
    }

    #[test]
    fn a_registered_entity_starts_at_the_default_for_its_host_type() {
        for (host_type, expected) in [
            (HostType::Server, EntityAuthStatus::Available),
            (HostType::Client, EntityAuthStatus::Requested),
        ] {
            let mut handler = HostAuthHandler::new();
            let accessor = handler.register_entity(host_type, &entity(1));

            assert_eq!(accessor.auth_status().status(), expected);
            assert_eq!(
                handler.auth_status(&entity(1)).map(|s| s.status()),
                Some(expected),
                "the handler and the accessor must agree on {host_type:?}",
            );
        }
    }

    #[test]
    fn an_unregistered_entity_has_no_auth_status() {
        let handler = HostAuthHandler::new();

        assert!(handler.auth_status(&entity(1)).is_none());
    }

    /// The `None` return is what distinguishes registered from unregistered,
    /// so a test that only ever asks about unregistered entities would be
    /// satisfied by `auth_status` returning `None` unconditionally. Both sides
    /// are asserted here against the same handler.
    #[test]
    fn auth_status_distinguishes_registered_from_unregistered_entities() {
        let mut handler = HostAuthHandler::new();
        handler.register_entity(HostType::Server, &entity(1));

        assert!(handler.auth_status(&entity(1)).is_some());
        assert!(handler.auth_status(&entity(2)).is_none());
    }

    #[test]
    fn registering_the_same_entity_twice_panics() {
        let mut handler = HostAuthHandler::new();
        handler.register_entity(HostType::Server, &entity(1));

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            handler.register_entity(HostType::Server, &entity(1));
        }));

        assert!(
            result.is_err(),
            "double registration must be rejected: it would hand out a second, \
             unrelated channel and silently orphan every accessor already \
             cloned from the first",
        );
    }

    /// A write through the handler must be visible through an accessor that
    /// was cloned out *before* the write -- that shared-state aliasing is the
    /// whole point of the mutator/accessor pair.
    #[test]
    fn setting_auth_status_is_visible_through_an_existing_accessor() {
        let mut handler = HostAuthHandler::new();
        let accessor = handler.register_entity(HostType::Server, &entity(1));
        assert_eq!(accessor.auth_status().status(), EntityAuthStatus::Available);

        handler.set_auth_status(&entity(1), EntityAuthStatus::Granted);

        assert_eq!(
            accessor.auth_status().status(),
            EntityAuthStatus::Granted,
            "the accessor is a live view, not a snapshot taken at register time",
        );
        assert_eq!(
            handler.auth_status(&entity(1)).map(|s| s.status()),
            Some(EntityAuthStatus::Granted),
        );
    }

    #[test]
    fn every_auth_status_round_trips_through_the_handler() {
        let mut handler = HostAuthHandler::new();
        let accessor = handler.register_entity(HostType::Server, &entity(1));

        for status in [
            EntityAuthStatus::Requested,
            EntityAuthStatus::Granted,
            EntityAuthStatus::Releasing,
            EntityAuthStatus::Denied,
            EntityAuthStatus::Available,
        ] {
            handler.set_auth_status(&entity(1), status);
            assert_eq!(accessor.auth_status().status(), status, "status {status:?}");
        }
    }

    #[test]
    fn setting_auth_status_on_an_unregistered_entity_panics() {
        let handler = HostAuthHandler::new();

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            handler.set_auth_status(&entity(1), EntityAuthStatus::Granted);
        }));

        assert!(result.is_err());
    }

    #[test]
    fn getting_an_accessor_for_an_unregistered_entity_panics() {
        let handler = HostAuthHandler::new();

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            handler.get_accessor(&entity(1));
        }));

        assert!(result.is_err());
    }

    #[test]
    fn get_accessor_returns_a_handle_onto_the_same_channel() {
        let mut handler = HostAuthHandler::new();
        let registered = handler.register_entity(HostType::Server, &entity(1));
        let fetched = handler.get_accessor(&entity(1));

        handler.set_auth_status(&entity(1), EntityAuthStatus::Denied);

        assert_eq!(registered.auth_status().status(), EntityAuthStatus::Denied);
        assert_eq!(fetched.auth_status().status(), EntityAuthStatus::Denied);
    }

    /// Deregistration happens on despawn. If it no-ops, the entity stays
    /// registered forever -- the map leaks, and a later re-register of the same
    /// `GlobalEntity` hits the double-registration panic instead of succeeding.
    #[test]
    fn deregistering_an_entity_removes_it() {
        let mut handler = HostAuthHandler::new();
        handler.register_entity(HostType::Server, &entity(1));

        handler.deregister_entity(&entity(1));

        assert!(
            handler.auth_status(&entity(1)).is_none(),
            "deregister_entity left the channel in the map",
        );
    }

    #[test]
    fn an_entity_can_be_registered_again_after_deregistration() {
        let mut handler = HostAuthHandler::new();
        handler.register_entity(HostType::Server, &entity(1));
        handler.set_auth_status(&entity(1), EntityAuthStatus::Granted);

        handler.deregister_entity(&entity(1));
        let accessor = handler.register_entity(HostType::Server, &entity(1));

        assert_eq!(
            accessor.auth_status().status(),
            EntityAuthStatus::Available,
            "the respawned entity must get a fresh channel, not the despawned \
             entity's leftover Granted authority",
        );
    }

    #[test]
    fn deregistering_one_entity_leaves_the_others_alone() {
        let mut handler = HostAuthHandler::new();
        handler.register_entity(HostType::Server, &entity(1));
        handler.register_entity(HostType::Server, &entity(2));

        handler.deregister_entity(&entity(1));

        assert!(handler.auth_status(&entity(1)).is_none());
        assert!(handler.auth_status(&entity(2)).is_some());
    }

    #[test]
    fn deregistering_an_unregistered_entity_is_a_no_op() {
        let mut handler = HostAuthHandler::new();
        handler.register_entity(HostType::Server, &entity(2));

        handler.deregister_entity(&entity(1));

        assert!(handler.auth_status(&entity(2)).is_some());
    }
}
