use std::{
    hash::Hash,
    sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard},
};

use crate::world::local::local_entity::{HostEntity, OwnedLocalEntity, RemoteEntity};
use crate::world::update::global_dirty_bitset::GlobalDirtyBitset;
use crate::world::update::mut_channel::MutChannelType;
use crate::{
    bigmap::BigMapKey,
    world::{
        delegation::{auth_channel::EntityAuthAccessor, entity_auth_status::HostEntityAuthStatus},
        entity::{error::EntityDoesNotExistError, global_entity::GlobalEntity},
    },
    ComponentKind, ComponentKinds, GlobalDiffHandler, HostEntityGenerator, InScopeEntities,
    LocalEntityMap, PropertyMutator,
};

/// Global world state queries needed during message and component serialization.
pub trait GlobalWorldManagerType: InScopeEntities<GlobalEntity> {
    /// Returns the list of component kinds currently attached to `entity`, or `None` if the entity is not known.
    fn component_kinds(&self, entity: &GlobalEntity) -> Option<Vec<ComponentKind>>;
    /// Whether or not a given user can receive a Message/Component with an EntityProperty relating to the given Entity
    fn entity_can_relate_to_user(&self, global_entity: &GlobalEntity, user_key: &u64) -> bool;
    /// Creates a new `MutChannelType` of `diff_mask_length` bytes for a component's mutation tracking.
    fn new_mut_channel(&self, diff_mask_length: u8) -> Arc<RwLock<dyn MutChannelType>>;
    /// Returns a handle to the global diff handler used to fan out property mutations.
    fn diff_handler(&self) -> Arc<RwLock<GlobalDiffHandler>>;
    /// Registers a component for mutation tracking, returning a [`PropertyMutator`] wired to the global diff handler.
    fn register_component(
        &self,
        component_kinds: &ComponentKinds,
        global_entity: &GlobalEntity,
        component_kind: &ComponentKind,
        diff_mask_length: u8,
    ) -> PropertyMutator;
    /// Returns an [`EntityAuthAccessor`] for reading the delegation authority state of `global_entity`.
    fn get_entity_auth_accessor(&self, global_entity: &GlobalEntity) -> EntityAuthAccessor;
    /// Returns `true` if `global_entity` requires a `PropertyMutator` to notify authority changes during delegation.
    fn entity_needs_mutator_for_delegation(&self, global_entity: &GlobalEntity) -> bool;
    /// Returns `true` if `global_entity` is actively being replicated.
    fn entity_is_replicating(&self, global_entity: &GlobalEntity) -> bool;
    /// Returns `true` if `global_entity` was spawned as a static entity.
    fn entity_is_static(&self, global_entity: &GlobalEntity) -> bool;
    /// Authority status for `global_entity`, or `None` when the entity has no
    /// delegation authority state to consult.
    ///
    /// Unlike [`Self::get_entity_auth_accessor`], this never panics on an
    /// unregistered entity, so the send path can ask about any entity it is
    /// about to serialize. The default returns `None` ("no constraint").
    fn entity_auth_status(&self, _global_entity: &GlobalEntity) -> Option<HostEntityAuthStatus> {
        None
    }
    /// Returns the global dirty bitset for mutation tracking, or `None` on the client side.
    fn global_dirty_bitset(&self) -> Option<Arc<GlobalDirtyBitset>> {
        None
    }
}

/// Bidirectional conversion between a world-type entity `E` and a `GlobalEntity`.
pub trait EntityAndGlobalEntityConverter<E: Copy + Eq + Hash + Sync + Send> {
    /// Resolves `global_entity` to the corresponding world-local entity `E`, or returns an error if not found.
    fn global_entity_to_entity(
        &self,
        global_entity: &GlobalEntity,
    ) -> Result<E, EntityDoesNotExistError>;
    /// Resolves a world-local `entity` to its stable [`GlobalEntity`] identifier, or returns an error if not found.
    fn entity_to_global_entity(&self, entity: &E) -> Result<GlobalEntity, EntityDoesNotExistError>;
}

/// Conversions between the connection-local host/remote entity representations and the global entity space.
pub trait LocalEntityAndGlobalEntityConverter {
    /// Returns the [`HostEntity`] for `global_entity` if one is registered, or an error otherwise.
    fn global_entity_to_host_entity(
        &self,
        global_entity: &GlobalEntity,
    ) -> Result<HostEntity, EntityDoesNotExistError>;
    /// Returns the [`RemoteEntity`] for `global_entity` if one is registered, or an error otherwise.
    fn global_entity_to_remote_entity(
        &self,
        global_entity: &GlobalEntity,
    ) -> Result<RemoteEntity, EntityDoesNotExistError>;
    /// Returns the [`OwnedLocalEntity`] (host or remote) for `global_entity`, or an error if not found.
    fn global_entity_to_owned_entity(
        &self,
        global_entity: &GlobalEntity,
    ) -> Result<OwnedLocalEntity, EntityDoesNotExistError>;
    /// Returns the [`GlobalEntity`] for a dynamic `host_entity`, or an error if not found.
    fn host_entity_to_global_entity(
        &self,
        host_entity: &HostEntity,
    ) -> Result<GlobalEntity, EntityDoesNotExistError>;
    /// Returns the [`GlobalEntity`] for a static `host_entity`, or an error if not found.
    fn static_host_entity_to_global_entity(
        &self,
        host_entity: &HostEntity,
    ) -> Result<GlobalEntity, EntityDoesNotExistError>;
    /// Returns the [`GlobalEntity`] for `remote_entity`, or an error if not found.
    fn remote_entity_to_global_entity(
        &self,
        remote_entity: &RemoteEntity,
    ) -> Result<GlobalEntity, EntityDoesNotExistError>;
    /// Returns the [`GlobalEntity`] for `owned_entity`, dispatching to the appropriate host or remote lookup.
    fn owned_entity_to_global_entity(
        &self,
        owned_entity: &OwnedLocalEntity,
    ) -> Result<GlobalEntity, EntityDoesNotExistError> {
        match owned_entity {
            OwnedLocalEntity::Host {
                id,
                is_static: true,
            } => self.static_host_entity_to_global_entity(&HostEntity::new(*id)),
            OwnedLocalEntity::Host {
                id,
                is_static: false,
            } => self.host_entity_to_global_entity(&HostEntity::new(*id)),
            OwnedLocalEntity::Remote { id, is_static } => {
                let remote = if *is_static {
                    RemoteEntity::new_static(*id)
                } else {
                    RemoteEntity::new(*id)
                };
                self.remote_entity_to_global_entity(&remote)
            }
        }
    }
    /// Returns the current redirect target for `entity`, or `entity` unchanged if no redirect is installed.
    fn apply_entity_redirect(&self, entity: &OwnedLocalEntity) -> OwnedLocalEntity;
}

/// No-op converter that always succeeds with entity ID 0; useful in test contexts where real mapping is not needed.
pub struct FakeEntityConverter;

impl LocalEntityAndGlobalEntityConverter for FakeEntityConverter {
    fn global_entity_to_host_entity(
        &self,
        _: &GlobalEntity,
    ) -> Result<HostEntity, EntityDoesNotExistError> {
        Ok(HostEntity::new(0))
    }

    fn global_entity_to_remote_entity(
        &self,
        _: &GlobalEntity,
    ) -> Result<RemoteEntity, EntityDoesNotExistError> {
        Ok(RemoteEntity::new(0))
    }

    fn global_entity_to_owned_entity(
        &self,
        _global_entity: &GlobalEntity,
    ) -> Result<OwnedLocalEntity, EntityDoesNotExistError> {
        Ok(OwnedLocalEntity::Host {
            id: 0,
            is_static: false,
        })
    }

    fn host_entity_to_global_entity(
        &self,
        _: &HostEntity,
    ) -> Result<GlobalEntity, EntityDoesNotExistError> {
        Ok(GlobalEntity::from_u64(0))
    }

    fn static_host_entity_to_global_entity(
        &self,
        _: &HostEntity,
    ) -> Result<GlobalEntity, EntityDoesNotExistError> {
        Ok(GlobalEntity::from_u64(0))
    }

    fn remote_entity_to_global_entity(
        &self,
        _: &RemoteEntity,
    ) -> Result<GlobalEntity, EntityDoesNotExistError> {
        Ok(GlobalEntity::from_u64(0))
    }

    fn apply_entity_redirect(&self, entity: &OwnedLocalEntity) -> OwnedLocalEntity {
        *entity // No redirects in fake converter
    }
}

impl LocalEntityAndGlobalEntityConverterMut for FakeEntityConverter {
    fn get_or_reserve_entity(
        &mut self,
        _global_entity: &GlobalEntity,
    ) -> Result<OwnedLocalEntity, EntityDoesNotExistError> {
        Ok(OwnedLocalEntity::Host {
            id: 0,
            is_static: false,
        })
    }
}

/// Mutable extension of `LocalEntityAndGlobalEntityConverter` that can allocate new host-side entity slots.
pub trait LocalEntityAndGlobalEntityConverterMut: LocalEntityAndGlobalEntityConverter {
    /// Looks up the local entity for `global_entity`, reserving a new host slot if none exists yet.
    fn get_or_reserve_entity(
        &mut self,
        global_entity: &GlobalEntity,
    ) -> Result<OwnedLocalEntity, EntityDoesNotExistError>;
}

/// Stateful converter used when writing messages: looks up or reserves host-side entity slots on demand.
pub struct EntityConverterMut<'a, 'b> {
    global_world_manager: &'a dyn GlobalWorldManagerType,
    local_entity_map: &'b mut LocalEntityMap,
    host_entity_generator: &'b mut HostEntityGenerator,
}

impl<'a, 'b> EntityConverterMut<'a, 'b> {
    /// Creates an `EntityConverterMut` backed by the given world manager, entity map, and generator.
    pub fn new(
        global_world_manager: &'a dyn GlobalWorldManagerType,
        local_entity_map: &'b mut LocalEntityMap,
        host_entity_generator: &'b mut HostEntityGenerator,
    ) -> Self {
        Self {
            global_world_manager,
            local_entity_map,
            host_entity_generator,
        }
    }
}

impl<'a, 'b> LocalEntityAndGlobalEntityConverter for EntityConverterMut<'a, 'b> {
    fn global_entity_to_host_entity(
        &self,
        global_entity: &GlobalEntity,
    ) -> Result<HostEntity, EntityDoesNotExistError> {
        self.local_entity_map
            .entity_converter()
            .global_entity_to_host_entity(global_entity)
    }

    fn global_entity_to_remote_entity(
        &self,
        global_entity: &GlobalEntity,
    ) -> Result<RemoteEntity, EntityDoesNotExistError> {
        self.local_entity_map
            .entity_converter()
            .global_entity_to_remote_entity(global_entity)
    }

    fn global_entity_to_owned_entity(
        &self,
        global_entity: &GlobalEntity,
    ) -> Result<OwnedLocalEntity, EntityDoesNotExistError> {
        self.local_entity_map
            .entity_converter()
            .global_entity_to_owned_entity(global_entity)
    }

    fn host_entity_to_global_entity(
        &self,
        host_entity: &HostEntity,
    ) -> Result<GlobalEntity, EntityDoesNotExistError> {
        self.local_entity_map
            .entity_converter()
            .host_entity_to_global_entity(host_entity)
    }

    fn static_host_entity_to_global_entity(
        &self,
        host_entity: &HostEntity,
    ) -> Result<GlobalEntity, EntityDoesNotExistError> {
        self.local_entity_map
            .entity_converter()
            .static_host_entity_to_global_entity(host_entity)
    }

    fn remote_entity_to_global_entity(
        &self,
        remote_entity: &RemoteEntity,
    ) -> Result<GlobalEntity, EntityDoesNotExistError> {
        self.local_entity_map
            .entity_converter()
            .remote_entity_to_global_entity(remote_entity)
    }

    fn apply_entity_redirect(&self, entity: &OwnedLocalEntity) -> OwnedLocalEntity {
        self.local_entity_map
            .entity_converter()
            .apply_entity_redirect(entity)
    }
}

impl<'a, 'b> LocalEntityAndGlobalEntityConverterMut for EntityConverterMut<'a, 'b> {
    fn get_or_reserve_entity(
        &mut self,
        global_entity: &GlobalEntity,
    ) -> Result<OwnedLocalEntity, EntityDoesNotExistError> {
        if !self
            .global_world_manager
            .entity_can_relate_to_user(global_entity, self.host_entity_generator.get_user_key())
        {
            return Err(EntityDoesNotExistError);
        }
        let result = self
            .local_entity_map
            .global_entity_to_owned_entity(global_entity);
        if result.is_ok() {
            // info!("get_or_reserve_entity(). `global_entity`: {:?} --> `owned_entity`: {:?}", global_entity, result);
            return result;
        }

        let host_entity = self
            .host_entity_generator
            .host_reserve_entity(self.local_entity_map, global_entity);

        // warn!("get_or_reserve_entity() `global_entity` {:?} is not owned by user, attempting to reserve. `host_entity`: {:?}", global_entity, host_entity);

        Ok(host_entity.copy_to_owned())
    }
}

// ── L3 send-state seam: guard-owning converters over the shared entity map ──
//
// `LocalWorldManager` holds `entity_map: Arc<RwLock<LocalEntityMap>>` (decision
// B). A `&dyn` converter borrowed straight out of the guard cannot escape the
// guard's scope, so these two wrappers OWN the guard and implement the converter
// traits by delegating through it. `LocalEntityMap` itself implements
// `LocalEntityAndGlobalEntityConverter`, so the read delegations are one-liners.

/// Read-only converter that owns a read guard on the shared entity map.
pub struct EntityMapReadConverter<'a> {
    guard: RwLockReadGuard<'a, LocalEntityMap>,
}

impl<'a> EntityMapReadConverter<'a> {
    /// Wrap a read guard on the shared entity map as a read-only converter.
    pub fn new(guard: RwLockReadGuard<'a, LocalEntityMap>) -> Self {
        Self { guard }
    }
}

impl LocalEntityAndGlobalEntityConverter for EntityMapReadConverter<'_> {
    fn global_entity_to_host_entity(
        &self,
        global_entity: &GlobalEntity,
    ) -> Result<HostEntity, EntityDoesNotExistError> {
        self.guard.global_entity_to_host_entity(global_entity)
    }
    fn global_entity_to_remote_entity(
        &self,
        global_entity: &GlobalEntity,
    ) -> Result<RemoteEntity, EntityDoesNotExistError> {
        self.guard.global_entity_to_remote_entity(global_entity)
    }
    fn global_entity_to_owned_entity(
        &self,
        global_entity: &GlobalEntity,
    ) -> Result<OwnedLocalEntity, EntityDoesNotExistError> {
        self.guard.global_entity_to_owned_entity(global_entity)
    }
    fn host_entity_to_global_entity(
        &self,
        host_entity: &HostEntity,
    ) -> Result<GlobalEntity, EntityDoesNotExistError> {
        self.guard.host_entity_to_global_entity(host_entity)
    }
    fn static_host_entity_to_global_entity(
        &self,
        host_entity: &HostEntity,
    ) -> Result<GlobalEntity, EntityDoesNotExistError> {
        self.guard.static_host_entity_to_global_entity(host_entity)
    }
    fn remote_entity_to_global_entity(
        &self,
        remote_entity: &RemoteEntity,
    ) -> Result<GlobalEntity, EntityDoesNotExistError> {
        self.guard.remote_entity_to_global_entity(remote_entity)
    }
    fn apply_entity_redirect(&self, entity: &OwnedLocalEntity) -> OwnedLocalEntity {
        self.guard.apply_entity_redirect(entity)
    }
}

/// Mutable converter that owns a write guard on the shared entity map plus a
/// `&mut` borrow of the host entity generator (the only `&mut` need in transmit
/// is `get_or_reserve_entity`, which reserves a host id during message
/// serialization). Mirrors [`EntityConverterMut`] but holds the guard.
pub struct EntityMapConverterMut<'a, 'b> {
    global_world_manager: &'a dyn GlobalWorldManagerType,
    guard: RwLockWriteGuard<'b, LocalEntityMap>,
    host_entity_generator: &'b mut HostEntityGenerator,
}

impl<'a, 'b> EntityMapConverterMut<'a, 'b> {
    /// Wrap a write guard on the shared entity map (plus the host entity
    /// generator) as a mutable converter.
    pub fn new(
        global_world_manager: &'a dyn GlobalWorldManagerType,
        guard: RwLockWriteGuard<'b, LocalEntityMap>,
        host_entity_generator: &'b mut HostEntityGenerator,
    ) -> Self {
        Self {
            global_world_manager,
            guard,
            host_entity_generator,
        }
    }
}

impl LocalEntityAndGlobalEntityConverter for EntityMapConverterMut<'_, '_> {
    fn global_entity_to_host_entity(
        &self,
        global_entity: &GlobalEntity,
    ) -> Result<HostEntity, EntityDoesNotExistError> {
        self.guard.global_entity_to_host_entity(global_entity)
    }
    fn global_entity_to_remote_entity(
        &self,
        global_entity: &GlobalEntity,
    ) -> Result<RemoteEntity, EntityDoesNotExistError> {
        self.guard.global_entity_to_remote_entity(global_entity)
    }
    fn global_entity_to_owned_entity(
        &self,
        global_entity: &GlobalEntity,
    ) -> Result<OwnedLocalEntity, EntityDoesNotExistError> {
        self.guard.global_entity_to_owned_entity(global_entity)
    }
    fn host_entity_to_global_entity(
        &self,
        host_entity: &HostEntity,
    ) -> Result<GlobalEntity, EntityDoesNotExistError> {
        self.guard.host_entity_to_global_entity(host_entity)
    }
    fn static_host_entity_to_global_entity(
        &self,
        host_entity: &HostEntity,
    ) -> Result<GlobalEntity, EntityDoesNotExistError> {
        self.guard.static_host_entity_to_global_entity(host_entity)
    }
    fn remote_entity_to_global_entity(
        &self,
        remote_entity: &RemoteEntity,
    ) -> Result<GlobalEntity, EntityDoesNotExistError> {
        self.guard.remote_entity_to_global_entity(remote_entity)
    }
    fn apply_entity_redirect(&self, entity: &OwnedLocalEntity) -> OwnedLocalEntity {
        self.guard.apply_entity_redirect(entity)
    }
}

impl LocalEntityAndGlobalEntityConverterMut for EntityMapConverterMut<'_, '_> {
    fn get_or_reserve_entity(
        &mut self,
        global_entity: &GlobalEntity,
    ) -> Result<OwnedLocalEntity, EntityDoesNotExistError> {
        if !self
            .global_world_manager
            .entity_can_relate_to_user(global_entity, self.host_entity_generator.get_user_key())
        {
            return Err(EntityDoesNotExistError);
        }
        let result = self.guard.global_entity_to_owned_entity(global_entity);
        if result.is_ok() {
            return result;
        }

        let host_entity = self
            .host_entity_generator
            .host_reserve_entity(&mut self.guard, global_entity);

        Ok(host_entity.copy_to_owned())
    }
}

#[cfg(test)]
mod entity_converter_tests {
    use super::*;
    use crate::{world::test_support::TestGwm, ComponentKinds, HostType};

    fn global(id: u64) -> GlobalEntity {
        GlobalEntity::from_u64(id)
    }

    // -- the address-form dispatch -----------------------------------------

    /// Answers each of the four lookups with a different `GlobalEntity`, so a
    /// test can tell which one `owned_entity_to_global_entity` reached for.
    /// The four wire forms are the same width, so a variant routed to the
    /// wrong lookup resolves to a real — and wrong — entity rather than
    /// failing.
    struct Signposts;

    const BY_HOST: u64 = 10;
    const BY_STATIC_HOST: u64 = 20;
    const BY_REMOTE: u64 = 30;
    const BY_STATIC_REMOTE: u64 = 40;

    impl LocalEntityAndGlobalEntityConverter for Signposts {
        fn global_entity_to_host_entity(
            &self,
            _: &GlobalEntity,
        ) -> Result<HostEntity, EntityDoesNotExistError> {
            unreachable!("not part of the dispatch under test")
        }
        fn global_entity_to_remote_entity(
            &self,
            _: &GlobalEntity,
        ) -> Result<RemoteEntity, EntityDoesNotExistError> {
            unreachable!("not part of the dispatch under test")
        }
        fn global_entity_to_owned_entity(
            &self,
            _: &GlobalEntity,
        ) -> Result<OwnedLocalEntity, EntityDoesNotExistError> {
            unreachable!("not part of the dispatch under test")
        }
        fn host_entity_to_global_entity(
            &self,
            host_entity: &HostEntity,
        ) -> Result<GlobalEntity, EntityDoesNotExistError> {
            assert_eq!(host_entity.value(), 7, "the id must survive the dispatch");
            Ok(global(BY_HOST))
        }
        fn static_host_entity_to_global_entity(
            &self,
            host_entity: &HostEntity,
        ) -> Result<GlobalEntity, EntityDoesNotExistError> {
            assert_eq!(host_entity.value(), 7, "the id must survive the dispatch");
            Ok(global(BY_STATIC_HOST))
        }
        fn remote_entity_to_global_entity(
            &self,
            remote_entity: &RemoteEntity,
        ) -> Result<GlobalEntity, EntityDoesNotExistError> {
            assert_eq!(remote_entity.value(), 7, "the id must survive the dispatch");
            Ok(global(if remote_entity.is_static() {
                BY_STATIC_REMOTE
            } else {
                BY_REMOTE
            }))
        }
        fn apply_entity_redirect(&self, entity: &OwnedLocalEntity) -> OwnedLocalEntity {
            *entity
        }
    }

    #[test]
    fn each_address_form_resolves_through_its_own_lookup() {
        let cases = [
            (
                OwnedLocalEntity::Host {
                    id: 7,
                    is_static: false,
                },
                BY_HOST,
            ),
            (
                OwnedLocalEntity::Host {
                    id: 7,
                    is_static: true,
                },
                BY_STATIC_HOST,
            ),
            (
                OwnedLocalEntity::Remote {
                    id: 7,
                    is_static: false,
                },
                BY_REMOTE,
            ),
            (
                OwnedLocalEntity::Remote {
                    id: 7,
                    is_static: true,
                },
                BY_STATIC_REMOTE,
            ),
        ];

        for (owned, expected) in cases {
            assert_eq!(
                Signposts.owned_entity_to_global_entity(&owned),
                Ok(global(expected)),
                "{owned:?} must not be resolved by another form's lookup",
            );
        }
    }

    // -- the stand-in converter --------------------------------------------

    #[test]
    fn the_fake_converter_answers_everything_with_entity_zero() {
        let entity = global(123);
        assert_eq!(
            FakeEntityConverter.global_entity_to_host_entity(&entity),
            Ok(HostEntity::new(0)),
        );
        assert_eq!(
            FakeEntityConverter.global_entity_to_remote_entity(&entity),
            Ok(RemoteEntity::new(0)),
        );
        assert_eq!(
            FakeEntityConverter.global_entity_to_owned_entity(&entity),
            Ok(OwnedLocalEntity::Host {
                id: 0,
                is_static: false,
            }),
        );
        assert_eq!(
            FakeEntityConverter.get_or_reserve_entity(&entity),
            Ok(OwnedLocalEntity::Host {
                id: 0,
                is_static: false,
            }),
            "reserving must agree with looking up, or a written entity would \
             not read back as itself",
        );
        assert_eq!(
            FakeEntityConverter.host_entity_to_global_entity(&HostEntity::new(9)),
            Ok(global(0)),
        );
        assert_eq!(
            FakeEntityConverter.static_host_entity_to_global_entity(&HostEntity::new(9)),
            Ok(global(0)),
        );
        assert_eq!(
            FakeEntityConverter.remote_entity_to_global_entity(&RemoteEntity::new(9)),
            Ok(global(0)),
        );
    }

    #[test]
    fn the_fake_converter_redirects_nothing() {
        let entity = OwnedLocalEntity::Remote {
            id: 3,
            is_static: true,
        };
        assert_eq!(FakeEntityConverter.apply_entity_redirect(&entity), entity);
    }

    // -- the map-backed converters -----------------------------------------

    /// A map holding one entity of each kind, plus a redirect.
    fn a_populated_map() -> LocalEntityMap {
        let mut map = LocalEntityMap::new(HostType::Server);
        map.insert_with_host_entity(global(1), HostEntity::new(11));
        map.insert_with_static_host_entity(global(2), HostEntity::new(22));
        map.insert_with_remote_entity(global(3), RemoteEntity::new(33));
        map.install_entity_redirect(
            OwnedLocalEntity::Remote {
                id: 33,
                is_static: false,
            },
            OwnedLocalEntity::Host {
                id: 11,
                is_static: false,
            },
        );
        map
    }

    /// Every answer the populated map gives, so a wrapper can be held to it.
    fn assert_answers_like_the_map(converter: &dyn LocalEntityAndGlobalEntityConverter) {
        assert_eq!(
            converter.global_entity_to_host_entity(&global(1)),
            Ok(HostEntity::new(11)),
        );
        assert_eq!(
            converter.global_entity_to_remote_entity(&global(3)),
            Ok(RemoteEntity::new(33)),
        );
        assert_eq!(
            converter.global_entity_to_owned_entity(&global(1)),
            Ok(OwnedLocalEntity::Host {
                id: 11,
                is_static: false,
            }),
        );
        assert_eq!(
            converter.host_entity_to_global_entity(&HostEntity::new(11)),
            Ok(global(1)),
        );
        assert_eq!(
            converter.static_host_entity_to_global_entity(&HostEntity::new(22)),
            Ok(global(2)),
        );
        assert_eq!(
            converter.remote_entity_to_global_entity(&RemoteEntity::new(33)),
            Ok(global(3)),
        );
        assert_eq!(
            converter.apply_entity_redirect(&OwnedLocalEntity::Remote {
                id: 33,
                is_static: false,
            }),
            OwnedLocalEntity::Host {
                id: 11,
                is_static: false,
            },
            "a migrated entity must be looked up under its new address",
        );
        assert_eq!(
            converter.global_entity_to_host_entity(&global(99)),
            Err(EntityDoesNotExistError),
        );
    }

    /// The three wrappers exist only because a `&dyn` borrowed out of a guard
    /// cannot outlive it. None of them may change an answer on the way past.
    #[test]
    fn every_wrapper_reports_exactly_what_the_map_holds() {
        let kinds = ComponentKinds::new();
        let gwm = TestGwm::new(&kinds);

        let mut map = a_populated_map();
        let mut generator = HostEntityGenerator::new(1);
        assert_answers_like_the_map(&EntityConverterMut::new(&gwm, &mut map, &mut generator));

        let shared = Arc::new(RwLock::new(a_populated_map()));
        assert_answers_like_the_map(&EntityMapReadConverter::new(shared.read().unwrap()));

        let mut generator = HostEntityGenerator::new(1);
        assert_answers_like_the_map(&EntityMapConverterMut::new(
            &gwm,
            shared.write().unwrap(),
            &mut generator,
        ));
    }

    // -- reserving on the way out ------------------------------------------

    #[test]
    fn an_entity_the_user_may_not_see_is_refused_rather_than_reserved() {
        let kinds = ComponentKinds::new();
        let gwm = TestGwm::new(&kinds);
        gwm.deny_relation(&global(5));

        let mut map = LocalEntityMap::new(HostType::Server);
        let mut generator = HostEntityGenerator::new(1);
        let mut converter = EntityConverterMut::new(&gwm, &mut map, &mut generator);

        assert_eq!(
            converter.get_or_reserve_entity(&global(5)),
            Err(EntityDoesNotExistError),
        );
        assert!(
            !map.contains_global_entity(&global(5)),
            "a refused entity must not be left holding a reservation",
        );
    }

    #[test]
    fn an_entity_already_in_the_map_is_returned_rather_than_reserved_again() {
        let kinds = ComponentKinds::new();
        let gwm = TestGwm::new(&kinds);

        let mut map = a_populated_map();
        let mut generator = HostEntityGenerator::new(1);

        let owned = {
            let mut converter = EntityConverterMut::new(&gwm, &mut map, &mut generator);
            converter
                .get_or_reserve_entity(&global(3))
                .expect("the entity is mapped")
        };
        assert_eq!(
            owned,
            OwnedLocalEntity::Remote {
                id: 33,
                is_static: false,
            },
            "an entity this peer received keeps its remote address; reserving \
             a host address for it would rename someone else's entity",
        );
    }

    #[test]
    fn an_unmapped_entity_is_reserved_a_host_address_and_keeps_it() {
        let kinds = ComponentKinds::new();
        let gwm = TestGwm::new(&kinds);

        let mut map = LocalEntityMap::new(HostType::Server);
        let mut generator = HostEntityGenerator::new(1);

        let first = {
            let mut converter = EntityConverterMut::new(&gwm, &mut map, &mut generator);
            converter
                .get_or_reserve_entity(&global(5))
                .expect("an unmapped entity is reserved, not refused")
        };
        let OwnedLocalEntity::Host { id, is_static } = first else {
            panic!("a reservation must be a host address, got {first:?}");
        };
        assert!(!is_static, "a reserved address is not a static one");
        assert_eq!(
            map.global_entity_from_host(&HostEntity::new(id)),
            Some(&global(5)),
            "the reservation must be recorded in the map, not just returned",
        );

        let second = {
            let mut converter = EntityConverterMut::new(&gwm, &mut map, &mut generator);
            converter
                .get_or_reserve_entity(&global(5))
                .expect("the entity is mapped now")
        };
        assert_eq!(
            second, first,
            "reserving twice must not rename the entity mid-flight",
        );
    }

    /// `EntityMapConverterMut` carries its own copy of the reserve logic
    /// rather than delegating, so the gate has to be pinned on both.
    #[test]
    fn the_guard_owning_converter_reserves_under_the_same_rules() {
        let kinds = ComponentKinds::new();
        let gwm = TestGwm::new(&kinds);
        gwm.deny_relation(&global(5));

        let shared = Arc::new(RwLock::new(LocalEntityMap::new(HostType::Server)));
        let mut generator = HostEntityGenerator::new(1);

        {
            let mut converter =
                EntityMapConverterMut::new(&gwm, shared.write().unwrap(), &mut generator);
            assert_eq!(
                converter.get_or_reserve_entity(&global(5)),
                Err(EntityDoesNotExistError),
            );
        }
        assert!(
            !shared.read().unwrap().contains_global_entity(&global(5)),
            "a refused entity must not be left holding a reservation",
        );

        let reserved = {
            let mut converter =
                EntityMapConverterMut::new(&gwm, shared.write().unwrap(), &mut generator);
            converter
                .get_or_reserve_entity(&global(6))
                .expect("an entity the user may see is reserved, not refused")
        };
        let OwnedLocalEntity::Host { id, .. } = reserved else {
            panic!("a reservation must be a host address, got {reserved:?}");
        };
        assert_eq!(
            shared
                .read()
                .unwrap()
                .global_entity_from_host(&HostEntity::new(id)),
            Some(&global(6)),
            "the reservation must be recorded in the shared map",
        );
    }
}
