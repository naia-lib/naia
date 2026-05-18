//! `SendStateView<E>` — read-only Sim-side view into naia's registered
//! entity/component state, used to drive per-tick snapshot construction
//! for `SnapshotWorld<E>`.
//!
//! Cyberlith's Sim SubApp holds one as a bevy `Resource` (cloned from
//! [`CoordHandle::send_state_view`]) and queries it each tick to learn:
//! - which entities to mark live in `SnapshotWorld`
//! - which `(entity, component_kind)` pairs to populate from Sim's
//!   gameplay world
//!
//! # Superset semantics
//!
//! Per SPEC_IRIS_2_NAIA.md §1.3 (cyberlith repo), the methods return
//! a safe **superset** of what naia actually reads during
//! `send_all_packets`. Specifically:
//!
//! - [`SendStateView::live_entities`] = every registered global entity
//!   (regardless of per-user scope).
//! - [`SendStateView::required_snapshot_entries`] = every `(entity, kind)`
//!   pair currently registered with the `GlobalWorldManager`.
//!
//! naia's per-user scope + dirty filters still apply inside
//! `send_all_packets`, so over-supplying entries in `SnapshotWorld` is
//! harmless — naia just doesn't read them.
//!
//! Why a superset and not the exact minimum: the exact minimum would
//! require reading per-user `ConnectionVisibilityBitset` + per-user
//! `BaseSendConnection::world_manager` command queues, both of which
//! live behind Send-thread-private `SendState`. Sim's Sim-side thread
//! cannot access those without either a cross-thread lock (forbidden by
//! the C.3 pipeline's no-blocking principle) or a Send→Sim ferry. The
//! superset is computable purely from `Arc<ServerShared>` state, which
//! is already cross-thread-shareable.
//!
//! Future optimization (deferred): trim the superset via a Send→Sim
//! atomic snapshot of dirty bits. Out of scope for Iris 2.

use std::{hash::Hash, sync::Arc};

use naia_shared::{ComponentKind, EntityAndGlobalEntityConverter};

use crate::pipeline_actors::handles::CoordHandle;
use crate::server::ServerShared;

/// Read-only Sim-side view into naia's registered entity/component
/// state. See module docs for the superset-semantics design.
///
/// Cloneable (`Arc`-backed); `Send + Sync`. Safe to hold across ticks
/// as a bevy `Resource` on Sim.
pub struct SendStateView<E: Copy + Eq + Hash + Send + Sync> {
    shared: Arc<ServerShared<E>>,
}

impl<E: Copy + Eq + Hash + Send + Sync> Clone for SendStateView<E> {
    fn clone(&self) -> Self {
        Self {
            shared: Arc::clone(&self.shared),
        }
    }
}

impl<E: Copy + Eq + Hash + Send + Sync> SendStateView<E> {
    /// Constructs a view backed by the supplied `ServerShared` arc.
    /// Public-but-not-typical: cyberlith should obtain a view via
    /// [`CoordHandle::send_state_view`] instead. This constructor is
    /// exposed primarily for tests that build a `ServerShared` directly.
    pub fn from_shared(shared: Arc<ServerShared<E>>) -> Self {
        Self { shared }
    }

    /// Returns every world entity currently registered with naia.
    /// Safe superset for `SnapshotWorld::mark_live`.
    ///
    /// Briefly acquires the `global_world_manager` and
    /// `global_entity_map` read guards; no guards held across the
    /// returned `Vec`.
    pub fn live_entities(&self) -> Vec<E> {
        let gwm = self.shared.global_world_manager.read();
        let gem = self.shared.global_entity_map.read();

        gwm.all_global_entities()
            .filter_map(|ge| gem.global_entity_to_entity(ge).ok())
            .collect()
    }

    /// Returns every `(entity, ComponentKind)` pair currently registered
    /// with naia's `GlobalWorldManager`. Safe superset for the set of
    /// pairs cyberlith must populate into `SnapshotWorld` before the
    /// next `send_all_packets`.
    ///
    /// Briefly acquires the `global_world_manager` and
    /// `global_entity_map` read guards; no guards held across the
    /// returned `Vec`.
    pub fn required_snapshot_entries(&self) -> Vec<(E, ComponentKind)> {
        let gwm = self.shared.global_world_manager.read();
        let gem = self.shared.global_entity_map.read();

        let mut out = Vec::new();
        for ge in gwm.all_global_entities() {
            let Ok(world_entity) = gem.global_entity_to_entity(ge) else {
                // Reserved-but-not-yet-spawned global entities have no
                // world-entity counterpart; skip them (cyberlith can't
                // look them up anyway).
                continue;
            };
            if let Some(kinds) = gwm.component_kinds(ge) {
                for kind in kinds {
                    out.push((world_entity, kind));
                }
            }
        }
        out
    }
}

impl<E: Copy + Eq + Hash + Send + Sync> CoordHandle<E> {
    /// Construct a [`SendStateView<E>`] backed by the same
    /// `Arc<ServerShared>` as this CoordHandle. Cyberlith calls this
    /// once at init and hands the view to Sim as a Resource.
    pub fn send_state_view(&self) -> SendStateView<E> {
        SendStateView::from_shared(Arc::clone(&self.shared))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use naia_shared::{
        ComponentKind, GlobalEntity, GlobalEntitySpawner, Protocol,
    };

    use crate::EntityOwner;
    use crate::ServerConfig;
    use crate::pipeline_actors::spawn_server_handles;

    /// Compile-time check: SendStateView<E> is Send + Sync + Clone.
    fn assert_traits<T: Send + Sync + Clone>() {}

    #[test]
    fn send_state_view_is_send_sync_clone() {
        assert_traits::<SendStateView<u64>>();
    }

    /// Build a fresh CoordHandle<u64> backed by an empty protocol,
    /// extract its SendStateView, and assert both methods return empty
    /// collections on a brand-new (no entities registered) server.
    #[test]
    fn empty_server_yields_empty_view() {
        let mut proto = Protocol::builder();
        proto.lock();
        let protocol = proto.build();

        let (coord, _recv, _send) =
            spawn_server_handles::<u64, _>(ServerConfig::default(), protocol);

        let view = coord.send_state_view();
        assert!(view.live_entities().is_empty(), "no entities → empty live");
        assert!(
            view.required_snapshot_entries().is_empty(),
            "no entities → empty required",
        );
    }

    /// Registers a world entity + two component records directly into
    /// `ServerShared::global_world_manager` and asserts the view picks
    /// them up. Uses two arbitrary `ComponentKind`s — the view's
    /// correctness does not depend on the kinds being registered in the
    /// protocol's `ComponentKinds` registry (`required_snapshot_entries`
    /// reads from `global_world_manager.component_kinds(global_entity)`
    /// which is populated by `insert_component_record`).
    #[test]
    fn registered_entity_appears_in_both_views() {
        let mut proto = Protocol::builder();
        proto.lock();
        let protocol = proto.build();

        let (coord, _recv, _send) =
            spawn_server_handles::<u64, _>(ServerConfig::default(), protocol);

        // Register a world entity (id = 42) + global entity + two
        // arbitrary component kinds. Bypass the WorldServer spawn API
        // entirely — go straight at the global maps.
        let world_entity: u64 = 42;
        let global_entity: GlobalEntity = {
            let mut gem = coord.shared.global_entity_map.write();
            gem.spawn(world_entity, None)
        };
        {
            let mut gwm = coord.shared.global_world_manager.write();
            gwm.insert_entity_record(&global_entity, EntityOwner::Server);
            // Two distinct kinds drawn from std types — kinds are
            // TypeId-derived, so any two distinct types serve.
            gwm.insert_component_record(&global_entity, &ComponentKind::of::<TestKindA>());
            gwm.insert_component_record(&global_entity, &ComponentKind::of::<TestKindB>());
        }

        let view = coord.send_state_view();
        let live = view.live_entities();
        assert_eq!(live, vec![world_entity], "live should contain just the registered entity");

        let mut entries = view.required_snapshot_entries();
        entries.sort_by_key(|(_, k)| format!("{k:?}"));
        let mut expected = vec![
            (world_entity, ComponentKind::of::<TestKindA>()),
            (world_entity, ComponentKind::of::<TestKindB>()),
        ];
        expected.sort_by_key(|(_, k)| format!("{k:?}"));
        assert_eq!(entries, expected);
    }

    #[test]
    fn deregistered_entity_drops_from_view() {
        let mut proto = Protocol::builder();
        proto.lock();
        let protocol = proto.build();

        let (coord, _recv, _send) =
            spawn_server_handles::<u64, _>(ServerConfig::default(), protocol);

        let world_entity: u64 = 7;
        let global_entity: GlobalEntity = {
            let mut gem = coord.shared.global_entity_map.write();
            gem.spawn(world_entity, None)
        };
        {
            let mut gwm = coord.shared.global_world_manager.write();
            gwm.insert_entity_record(&global_entity, EntityOwner::Server);
            gwm.insert_component_record(&global_entity, &ComponentKind::of::<TestKindA>());
        }

        // Sanity: present before removal.
        assert_eq!(coord.send_state_view().live_entities(), vec![world_entity]);

        // Deregister the component record + entity record + map entry.
        {
            let mut gwm = coord.shared.global_world_manager.write();
            gwm.remove_component_record(&global_entity, &ComponentKind::of::<TestKindA>());
            gwm.remove_entity_diff_handlers(&global_entity);
            gwm.remove_entity_record(&global_entity);
        }
        {
            let mut gem = coord.shared.global_entity_map.write();
            gem.despawn_by_world(&world_entity);
        }

        let view = coord.send_state_view();
        assert!(view.live_entities().is_empty(), "live empty after removal");
        assert!(
            view.required_snapshot_entries().is_empty(),
            "required empty after removal",
        );
    }

    /// Multiple entities + multiple components yields the expected
    /// cross-product of `(entity, kind)` pairs.
    #[test]
    fn multiple_entities_yield_cross_product() {
        let mut proto = Protocol::builder();
        proto.lock();
        let protocol = proto.build();

        let (coord, _recv, _send) =
            spawn_server_handles::<u64, _>(ServerConfig::default(), protocol);

        let e1: u64 = 1;
        let e2: u64 = 2;
        let g1 = coord.shared.global_entity_map.write().spawn(e1, None);
        let g2 = coord.shared.global_entity_map.write().spawn(e2, None);
        {
            let mut gwm = coord.shared.global_world_manager.write();
            gwm.insert_entity_record(&g1, EntityOwner::Server);
            gwm.insert_entity_record(&g2, EntityOwner::Server);
            gwm.insert_component_record(&g1, &ComponentKind::of::<TestKindA>());
            gwm.insert_component_record(&g1, &ComponentKind::of::<TestKindB>());
            gwm.insert_component_record(&g2, &ComponentKind::of::<TestKindA>());
        }

        let view = coord.send_state_view();
        let mut live = view.live_entities();
        live.sort();
        assert_eq!(live, vec![e1, e2]);

        let entries = view.required_snapshot_entries();
        assert_eq!(entries.len(), 3, "2 components on e1 + 1 on e2");
    }

    // ---- Test-only Replicate stubs ----------
    //
    // `ComponentKind::of::<T>` requires `T: Replicate`. We need two
    // distinct TypeIds — actual Replicate behavior is not exercised
    // here; only the type identity is consulted by
    // `GlobalWorldManager::insert_component_record` and the
    // `component_kinds` enumeration the view reads.
    //
    // (Mirrors the local-stub pattern used inside
    // `shared/src/world/snapshot_world.rs`'s test module.)

    use std::any::Any;
    use std::collections::HashSet;

    use naia_shared::{
        BitReader, BitWrite, ComponentFieldUpdate, ComponentKinds, DiffMask,
        LocalEntityAndGlobalEntityConverter, LocalEntityAndGlobalEntityConverterMut, Named,
        PendingComponentUpdate, PropertyMutator, ReplicaDynMut, ReplicaDynRef, RemoteEntity,
        Replicate, ReplicateBuilder, SerdeErr,
    };
    use naia_shared::EntityAuthAccessor;

    struct TestKindA;
    struct TestKindB;
    struct TestBuilderA;
    struct TestBuilderB;

    macro_rules! impl_test_kind {
        ($val:ident, $builder:ident, $name:literal) => {
            impl Named for $val {
                fn protocol_name() -> &'static str { $name }
                fn name(&self) -> String { $name.to_string() }
            }
            impl Named for $builder {
                fn protocol_name() -> &'static str { $name }
                fn name(&self) -> String { $name.to_string() }
            }
            impl ReplicateBuilder for $builder {
                fn read(
                    &self,
                    _r: &mut BitReader,
                    _c: &dyn LocalEntityAndGlobalEntityConverter,
                ) -> Result<Box<dyn Replicate>, SerdeErr> { unreachable!() }
                fn read_create_update(&self, _r: &mut BitReader) -> Result<PendingComponentUpdate, SerdeErr> { unreachable!() }
                fn split_update(
                    &self,
                    _c: &dyn LocalEntityAndGlobalEntityConverter,
                    _u: PendingComponentUpdate,
                ) -> Result<(Option<Vec<(RemoteEntity, ComponentFieldUpdate)>>, Option<PendingComponentUpdate>), SerdeErr> {
                    unreachable!()
                }
                fn box_clone(&self) -> Box<dyn ReplicateBuilder> { Box::new($builder) }
            }
            impl Replicate for $val {
                fn kind(&self) -> ComponentKind { ComponentKind::of::<$val>() }
                fn to_any(&self) -> &dyn Any { self }
                fn to_any_mut(&mut self) -> &mut dyn Any { self }
                fn to_boxed_any(self: Box<Self>) -> Box<dyn Any> { self }
                fn copy_to_box(&self) -> Box<dyn Replicate> { Box::new($val) }
                fn create_builder() -> Box<dyn ReplicateBuilder> where Self: Sized { Box::new($builder) }
                fn diff_mask_size(&self) -> u8 { 0 }
                fn dyn_ref(&self) -> ReplicaDynRef<'_> { ReplicaDynRef::new(self) }
                fn dyn_mut(&mut self) -> ReplicaDynMut<'_> { ReplicaDynMut::new(self) }
                fn mirror(&mut self, _o: &dyn Replicate) {}
                fn mirror_single_field(&mut self, _i: u8, _o: &dyn Replicate) {}
                fn set_mutator(&mut self, _m: &PropertyMutator) {}
                fn write(&self, _ck: &ComponentKinds, _w: &mut dyn BitWrite, _c: &mut dyn LocalEntityAndGlobalEntityConverterMut) {}
                fn write_update(&self, _dm: &DiffMask, _w: &mut dyn BitWrite, _c: &mut dyn LocalEntityAndGlobalEntityConverterMut) {}
                fn read_apply_update(&mut self, _c: &dyn LocalEntityAndGlobalEntityConverter, _u: PendingComponentUpdate) -> Result<(), SerdeErr> { Ok(()) }
                fn read_apply_field_update(&mut self, _c: &dyn LocalEntityAndGlobalEntityConverter, _u: ComponentFieldUpdate) -> Result<(), SerdeErr> { Ok(()) }
                fn relations_waiting(&self) -> Option<HashSet<RemoteEntity>> { None }
                fn relations_complete(&mut self, _c: &dyn LocalEntityAndGlobalEntityConverter) {}
                fn publish(&mut self, _m: &PropertyMutator) {}
                fn unpublish(&mut self) {}
                fn enable_delegation(&mut self, _a: &EntityAuthAccessor, _m: Option<&PropertyMutator>) {}
                fn disable_delegation(&mut self) {}
                fn localize(&mut self) {}
            }
            #[cfg(feature = "bevy_support")]
            impl naia_shared::bevy_ecs::component::Component for $val {
                const STORAGE_TYPE: naia_shared::bevy_ecs::component::StorageType =
                    naia_shared::bevy_ecs::component::StorageType::Table;
                type Mutability = naia_shared::bevy_ecs::component::Mutable;
            }
        };
    }

    impl_test_kind!(TestKindA, TestBuilderA, "TestKindA");
    impl_test_kind!(TestKindB, TestBuilderB, "TestKindB");
}
