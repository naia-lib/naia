#[cfg(feature = "entity_delegation")]
use std::sync::{Arc, RwLock};

use crate::{
    world::delegation::entity_auth_status::{EntityAuthStatus, HostEntityAuthStatus},
    HostType,
};

// EntityAuthChannel
#[derive(Clone)]
pub(crate) struct EntityAuthChannel {
    #[cfg(feature = "entity_delegation")]
    data: std::sync::Arc<std::sync::RwLock<EntityAuthData>>,
}

impl EntityAuthChannel {
    pub(crate) fn new_channel(host_type: HostType) -> (EntityAuthMutator, EntityAuthAccessor) {
        let channel = Self::new_channel_inner(host_type);

        let sender = EntityAuthMutator::new(&channel);
        let receiver = EntityAuthAccessor::new(&channel);

        (sender, receiver)
    }

    #[cfg(feature = "entity_delegation")]
    fn new_channel_inner(host_type: HostType) -> Self {
        Self {
            data: Arc::new(RwLock::new(EntityAuthData::new(host_type))),
        }
    }

    // Without the feature there is no per-entity state to share; the channel
    // is a sizeless token. Callers that need live state fail closed one layer
    // up (apply-layer arms / authority APIs return
    // `AuthorityError::DelegationDisabled`).
    #[cfg(not(feature = "entity_delegation"))]
    fn new_channel_inner(_host_type: HostType) -> Self {
        Self {}
    }

    #[cfg(feature = "entity_delegation")]
    fn auth_status(&self) -> HostEntityAuthStatus {
        let data = self
            .data
            .as_ref()
            .read()
            .expect("Lock on AuthStatus is held by current thread.");
        data.auth_status()
    }

    #[cfg(feature = "entity_delegation")]
    fn set_auth_status(&self, auth_status: EntityAuthStatus) {
        let mut data = self
            .data
            .as_ref()
            .write()
            .expect("Lock on AuthStatus is held by current thread.");
        data.set_auth_status(auth_status);
    }
}

// EntityAuthData
#[cfg(feature = "entity_delegation")]
struct EntityAuthData {
    host_type: HostType,
    status: EntityAuthStatus,
}

#[cfg(feature = "entity_delegation")]
impl EntityAuthData {
    fn new(host_type: HostType) -> Self {
        let status = match host_type {
            HostType::Server => EntityAuthStatus::Available,
            HostType::Client => EntityAuthStatus::Requested,
        };
        Self { host_type, status }
    }

    fn auth_status(&self) -> HostEntityAuthStatus {
        HostEntityAuthStatus::new(self.host_type, self.status)
    }

    fn set_auth_status(&mut self, auth_status: EntityAuthStatus) {
        self.status = auth_status;
    }
}

/// Read-only handle to an entity's shared authority state; cloneable and safe to embed in components.
#[derive(Clone)]
pub struct EntityAuthAccessor {
    #[cfg(feature = "entity_delegation")]
    channel: EntityAuthChannel,
}

impl EntityAuthAccessor {
    #[cfg(feature = "entity_delegation")]
    fn new(channel: &EntityAuthChannel) -> Self {
        Self {
            channel: channel.clone(),
        }
    }

    // Without the feature no accessor ever carries live state (the handler
    // stub answers `None` without consulting it); construction is a no-op.
    #[cfg(not(feature = "entity_delegation"))]
    fn new(_channel: &EntityAuthChannel) -> Self {
        Self {}
    }

    #[cfg(feature = "entity_delegation")]
    pub(crate) fn auth_status(&self) -> HostEntityAuthStatus {
        self.channel.auth_status()
    }

    // Without the feature there is no per-entity state; report the
    // client-side steady state a fresh channel would carry
    // (`HostType::Client` starts `Requested`). This keeps
    // `DelegatedProperty`'s `can_*` predicates total in OFF builds; live
    // delegation paths fail closed before consulting it, and builds without
    // delegation never drive these predicates with delegated entities.
    #[cfg(not(feature = "entity_delegation"))]
    pub(crate) fn auth_status(&self) -> HostEntityAuthStatus {
        HostEntityAuthStatus::new(HostType::Client, EntityAuthStatus::Requested)
    }
}

// EntityAuthMutator
// no Clone necessary
pub(crate) struct EntityAuthMutator {
    #[cfg(feature = "entity_delegation")]
    channel: EntityAuthChannel,
}

impl EntityAuthMutator {
    #[cfg(feature = "entity_delegation")]
    fn new(channel: &EntityAuthChannel) -> Self {
        Self {
            channel: channel.clone(),
        }
    }

    // Without the feature there is no state to mutate; the no-op is
    // unreachable from live paths (they fail closed before reaching here).
    #[cfg(not(feature = "entity_delegation"))]
    fn new(_channel: &EntityAuthChannel) -> Self {
        Self {}
    }

    #[cfg(feature = "entity_delegation")]
    pub(crate) fn set_auth_status(&self, auth_status: EntityAuthStatus) {
        self.channel.set_auth_status(auth_status);
    }

    // Without the feature there is no state to mutate. The no-op keeps
    // `enable_delegation` machinery total in OFF builds; live delegation
    // entry points fail closed before reaching here (`AuthorityError::
    // DelegationDisabled`), and builds without delegation never drive these
    // paths with delegated entities.
    #[cfg(not(feature = "entity_delegation"))]
    #[allow(dead_code)]
    pub(crate) fn set_auth_status(&self, _auth_status: EntityAuthStatus) {}
}

#[cfg(all(test, feature = "entity_delegation"))]
mod tests {
    //! The shared-state plumbing behind `HostAuthHandler`: one
    //! `EntityAuthChannel` per entity, handed out as a write-only
    //! `EntityAuthMutator` and any number of cloneable read-only
    //! `EntityAuthAccessor`s.
    //!
    //! Two things here are easy to get wrong invisibly. The first is aliasing:
    //! if `EntityAuthAccessor::new` ever took a snapshot instead of cloning the
    //! `Arc`, reads would silently freeze at the value from registration time.
    //! The second is the `host_type` carried through into
    //! `HostEntityAuthStatus` -- it is never read back directly, only through
    //! predicates like `can_mutate`, so it is asserted through one of them.

    use super::*;

    #[test]
    fn a_new_channel_starts_at_the_default_for_its_host_type() {
        for (host_type, expected) in [
            (HostType::Server, EntityAuthStatus::Available),
            (HostType::Client, EntityAuthStatus::Requested),
        ] {
            let (_mutator, accessor) = EntityAuthChannel::new_channel(host_type);

            assert_eq!(
                accessor.auth_status().status(),
                expected,
                "wrong initial status for {host_type:?}",
            );
        }
    }

    /// The server owns entities by default and may always mutate them; a client
    /// at `Available` may not. That asymmetry is the only observable proof that
    /// the channel's `host_type` survives into the status it reports.
    #[test]
    fn the_channels_host_type_reaches_the_reported_status() {
        let (_m, server) = EntityAuthChannel::new_channel(HostType::Server);
        assert!(server.auth_status().can_mutate());

        let (mutator, client) = EntityAuthChannel::new_channel(HostType::Client);
        mutator.set_auth_status(EntityAuthStatus::Available);
        assert!(
            !client.auth_status().can_mutate(),
            "a client with no authority must not be allowed to mutate; the \
             status came back tagged as a server",
        );
    }

    #[test]
    fn a_write_through_the_mutator_is_seen_by_the_accessor() {
        let (mutator, accessor) = EntityAuthChannel::new_channel(HostType::Server);

        for status in [
            EntityAuthStatus::Requested,
            EntityAuthStatus::Granted,
            EntityAuthStatus::Releasing,
            EntityAuthStatus::Denied,
            EntityAuthStatus::Available,
        ] {
            mutator.set_auth_status(status);
            assert_eq!(accessor.auth_status().status(), status, "status {status:?}");
        }
    }

    #[test]
    fn cloned_accessors_share_one_channel() {
        let (mutator, accessor) = EntityAuthChannel::new_channel(HostType::Server);
        let clone = accessor.clone();

        mutator.set_auth_status(EntityAuthStatus::Granted);

        assert_eq!(accessor.auth_status().status(), EntityAuthStatus::Granted);
        assert_eq!(
            clone.auth_status().status(),
            EntityAuthStatus::Granted,
            "a clone taken before the write must observe it too",
        );
    }

    #[test]
    fn separate_channels_do_not_share_state() {
        let (mutator_a, accessor_a) = EntityAuthChannel::new_channel(HostType::Server);
        let (_mutator_b, accessor_b) = EntityAuthChannel::new_channel(HostType::Server);

        mutator_a.set_auth_status(EntityAuthStatus::Granted);

        assert_eq!(accessor_a.auth_status().status(), EntityAuthStatus::Granted);
        assert_eq!(
            accessor_b.auth_status().status(),
            EntityAuthStatus::Available,
            "each entity gets its own channel",
        );
    }

    /// Reads take a read lock and writes take a write lock; a read held across
    /// a write would deadlock the caller rather than fail a test, so this pins
    /// that reads release before the next write is taken.
    #[test]
    fn interleaved_reads_and_writes_do_not_deadlock() {
        let (mutator, accessor) = EntityAuthChannel::new_channel(HostType::Server);

        for _ in 0..3 {
            let _ = accessor.auth_status();
            mutator.set_auth_status(EntityAuthStatus::Granted);
            let _ = accessor.auth_status();
            mutator.set_auth_status(EntityAuthStatus::Available);
        }

        assert_eq!(accessor.auth_status().status(), EntityAuthStatus::Available);
    }
}
