//! `SnapshotWorld<E>` — `WorldRefType<E>` impl backed by a per-tick snapshot.
//!
//! Pipelined consumers (cyberlith's Send SubApp) build a fresh
//! `SnapshotWorld<E>` each tick by querying
//! [`crate::SendStateView::required_snapshot_entries`] +
//! [`crate::SendStateView::live_entities`] (added in commit 3), reading
//! values from the authoritative gameplay world, and inserting them via
//! [`SnapshotWorld::insert_component`]. The snapshot is then passed to
//! `send_all_packets` and dropped after the call.
//!
//! See `SPEC_IRIS_2_NAIA.md` §1.2 (cyberlith repo) for design rationale
//! and `FINDINGS_IRIS_2_SPIKE.md` §8 for the strawman this is derived from.

use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
    net::SocketAddr,
};

use crate::{
    world::component::{
        component_kinds::ComponentKind,
        replica_ref::{
            ReplicaDynRefTrait, ReplicaDynRefWrapper, ReplicaRefTrait, ReplicaRefWrapper,
        },
        replicate::{Replicate, ReplicatedComponent},
    },
    world::entity::global_entity::GlobalEntity,
    world::update::global_dirty_bitset::FrozenGlobalDirty,
    world::update::global_entity_index::GlobalEntityIndex,
    world::world_type::WorldRefType,
    world::world_writer::UpdateKinds,
};

/// MISSION_TICK_FLOOR Lever 3: per-user send DECISION — the list of entities to
/// send for one user (each with its `GlobalEntityIndex` + per-component
/// `UpdateKinds`). Vec instead of HashMap: the per-user entity set is iterated
/// fully each tick (drain → score → sort → build), so there is no lookup by key;
/// eliminating the HashMap construction + insert allocations saves ~25% of
/// iris_phase3_build at high player counts.
pub type SendUpdateEvents = Vec<(GlobalEntity, GlobalEntityIndex, UpdateKinds)>;

/// MISSION_TICK_FLOOR Lever 3: a self-contained, frozen send job built by
/// [`crate::SnapshotWorld`]'s producer at the FREEZE point (on the gameplay
/// thread, in `SendState::prepare_send_job`) and transmitted later — possibly a
/// tick later, on the send worker — by `SendState::transmit_send_job`.
///
/// It carries everything the lagged transmit needs so it reads ZERO live
/// per-user diff state:
/// - `per_user`: the per-user (shuffled-order) update plan. Each entry's
///   `DiffMask` is the value captured at freeze time (the live mask was cleared
///   right after capture), so serialization is immune to concurrent mutation.
/// - `frozen_dirty`: a plain-`u64` copy of `global_dirty` taken BEFORE the masks
///   were cleared. `transmit`'s Phase 1/2 iterates THIS (not the live, now-
///   decremented bitset) to build `snapshot_map` + clear the per-tick wire cache.
pub struct SendPlan {
    /// Per-user (shuffled-order) update plan; each entry's `DiffMask` is the
    /// value captured at freeze time (the live mask was cleared right after).
    pub per_user: Vec<(SocketAddr, SendUpdateEvents)>,
    /// Plain-`u64` copy of `global_dirty` taken BEFORE the masks were cleared.
    /// `transmit`'s Phase 1/2 iterates this (not the live, decremented bitset).
    pub frozen_dirty: FrozenGlobalDirty,
}

/// [`WorldRefType<E>`] implementation backed by a per-tick ephemeral
/// snapshot of component values.
///
/// Pipelined consumers populate one each tick from values authoritatively
/// owned by their gameplay world and pass it to `send_all_packets`.
///
/// # Completeness contract
///
/// The caller MUST:
/// - [`insert_component`](Self::insert_component) every `(entity, kind)`
///   pair yielded by [`crate::SendStateView::required_snapshot_entries`]
///   (added in commit 3), AND
/// - [`mark_live`](Self::mark_live) every entity yielded by
///   [`crate::SendStateView::live_entities`].
///
/// Missing entries cause `send_all_packets` to panic on
/// `world.component_of_kind(...).expect(...)` (matching today's behavior
/// when a bevy world is missing an expected component).
///
/// # Wire-byte identity
///
/// Wire output is byte-identical to passing a [`WorldRefType<E>`] impl
/// over a bevy world holding the same component values. This is pinned
/// by the integration test
/// `server/tests/snapshot_world_wire_identity.rs` (added in commit 5).
///
/// # Storage type
///
/// Stores `Box<dyn Replicate>`. The newer [`crate::CachedReplicate`]
/// trait (commit 1) is the narrow contract that downstream pipelined
/// consumers see — the blanket `impl<R: Replicate> CachedReplicate for R`
/// means every value cyberlith ships is automatically convertible.
/// Internally we keep `Box<dyn Replicate>` because the
/// [`ReplicaDynRefWrapper`] / [`ReplicaRefWrapper`] surface that
/// `WorldRefType<E>::component*` returns is keyed on `Replicate`, and
/// holding the broader type avoids an extra trait-object layer in the
/// hot read path.
pub struct SnapshotWorld<E: Copy + Eq + Hash> {
    components: HashMap<(E, ComponentKind), Box<dyn Replicate>>,
    live_entities: HashSet<E>,
    /// MISSION_TICK_FLOOR Lever 3: optional frozen `global_dirty` rider. When
    /// present, this snapshot is a self-contained send *job* — the active send
    /// worker iterates this frozen "what-to-send" set (via
    /// `send_all_packets_frozen`) instead of the live, concurrently-mutated
    /// `global_dirty`. `None` for the deterministic oracle (which sends
    /// synchronously against the live bitset) and for all non-send uses.
    frozen_dirty: Option<FrozenGlobalDirty>,
    /// MISSION_TICK_FLOOR Lever 3: the prepared per-user send plan (frozen
    /// `DiffMask`s + frozen dirty domain). Attached on the gameplay thread by
    /// `SendState::prepare_send_job` at the freeze point; consumed by the send
    /// worker via `transmit_send_job`. Supersedes `frozen_dirty` as the
    /// self-contained job descriptor — when present, the transmit reads zero
    /// live per-user diff state. `None` for the deterministic oracle (which
    /// prepares + transmits synchronously) and all non-send uses.
    send_plan: Option<SendPlan>,
}

impl<E: Copy + Eq + Hash> Default for SnapshotWorld<E> {
    fn default() -> Self {
        Self::new()
    }
}

impl<E: Copy + Eq + Hash> SnapshotWorld<E> {
    /// Creates an empty snapshot.
    pub fn new() -> Self {
        Self {
            components: HashMap::new(),
            live_entities: HashSet::new(),
            frozen_dirty: None,
            send_plan: None,
        }
    }

    /// MISSION_TICK_FLOOR Lever 3: attach the prepared per-user send plan,
    /// turning this snapshot into a self-contained, frozen send job. Called by
    /// `SendState::prepare_send_job` on the active path.
    pub fn attach_send_plan(&mut self, plan: SendPlan) {
        self.send_plan = Some(plan);
    }

    /// MISSION_TICK_FLOOR Lever 3: take the prepared send plan out of the job.
    /// The send worker calls this, then dispatches to `transmit_send_job` when
    /// `Some`.
    pub fn take_send_plan(&mut self) -> Option<SendPlan> {
        self.send_plan.take()
    }

    /// MISSION_TICK_FLOOR Lever 3: attach the frozen `global_dirty` rider,
    /// turning this snapshot into a self-contained send job (see the
    /// `frozen_dirty` field). Called by the snapshot builder on the active
    /// path only.
    pub fn attach_frozen_dirty(&mut self, frozen: FrozenGlobalDirty) {
        self.frozen_dirty = Some(frozen);
    }

    /// MISSION_TICK_FLOOR Lever 3: take the frozen `global_dirty` rider out of
    /// the job (leaving `None`). The send worker calls this, then dispatches to
    /// `send_all_packets_frozen` when `Some` / `send_all_packets` when `None`.
    pub fn take_frozen_dirty(&mut self) -> Option<FrozenGlobalDirty> {
        self.frozen_dirty.take()
    }

    /// Marks `entity` as live (visible to `has_entity` / `entities`)
    /// without inserting any component. Used by the Send-stage builder
    /// for coord-stage scope-evaluation edge cases where naia consults
    /// `has_entity` for an entity that currently has zero replicated
    /// components in scope.
    ///
    /// `insert_component` already marks an entity live as a side effect,
    /// so the explicit `mark_live` API is needed only for the
    /// zero-component-currently-in-snapshot case.
    pub fn mark_live(&mut self, entity: E) {
        self.live_entities.insert(entity);
    }

    /// Removes the entity and cascade-removes all of its components from
    /// the snapshot. Symmetric inverse of [`Self::mark_live`] +
    /// [`Self::insert_component`].
    pub fn mark_despawned(&mut self, entity: E) {
        self.live_entities.remove(&entity);
        self.components.retain(|(e, _), _| *e != entity);
    }

    /// Inserts a component value into the snapshot, also marking the
    /// entity live as a side effect (to maintain the
    /// "entity-with-a-component-is-live" invariant required by
    /// `has_entity`).
    pub fn insert_component(
        &mut self,
        entity: E,
        kind: ComponentKind,
        value: Box<dyn Replicate>,
    ) {
        self.live_entities.insert(entity);
        self.components.insert((entity, kind), value);
    }

    /// Removes a single component from the snapshot. The entity stays
    /// live (it may still have other components or be live-with-no-
    /// components via `mark_live`).
    pub fn remove_component(&mut self, entity: E, kind: &ComponentKind) {
        self.components.remove(&(entity, *kind));
    }

    /// Number of `(entity, kind)` entries currently held. Exposed for
    /// tests and instrumentation.
    pub fn component_count(&self) -> usize {
        self.components.len()
    }

    /// Number of live entities currently held. Exposed for tests and
    /// instrumentation.
    pub fn live_entity_count(&self) -> usize {
        self.live_entities.len()
    }
}

// ============================================================================
// WorldRefType<E> impl
// ============================================================================

/// `ReplicaRefTrait` adapter holding a `&R` borrowed from a downcast of
/// the boxed `dyn Replicate` stored in `SnapshotWorld::components`.
struct BoxReplicaRef<'a, R: Replicate> {
    inner: &'a R,
}
impl<'a, R: Replicate> ReplicaRefTrait<R> for BoxReplicaRef<'a, R> {
    fn to_ref(&self) -> &R {
        self.inner
    }
}

/// `ReplicaDynRefTrait` adapter holding a `&dyn Replicate` borrowed from
/// the boxed `dyn Replicate` stored in `SnapshotWorld::components`.
struct BoxReplicaDynRef<'a> {
    inner: &'a dyn Replicate,
}
impl<'a> ReplicaDynRefTrait for BoxReplicaDynRef<'a> {
    fn to_dyn_ref(&self) -> &dyn Replicate {
        self.inner
    }
}

impl<E: Copy + Eq + Hash + Send + Sync> WorldRefType<E> for SnapshotWorld<E> {
    fn has_entity(&self, world_entity: &E) -> bool {
        self.live_entities.contains(world_entity)
    }

    fn entities(&self) -> Vec<E> {
        self.live_entities.iter().copied().collect()
    }

    fn has_component<R: ReplicatedComponent>(&self, world_entity: &E) -> bool {
        self.components
            .contains_key(&(*world_entity, ComponentKind::of::<R>()))
    }

    fn has_component_of_kind(&self, world_entity: &E, component_kind: &ComponentKind) -> bool {
        self.components
            .contains_key(&(*world_entity, *component_kind))
    }

    fn component<'a, R: ReplicatedComponent>(
        &'a self,
        entity: &E,
    ) -> Option<ReplicaRefWrapper<'a, R>> {
        let boxed = self.components.get(&(*entity, ComponentKind::of::<R>()))?;
        let any_ref: &dyn std::any::Any = boxed.as_ref().to_any();
        let downcast: &R = any_ref.downcast_ref::<R>()?;
        Some(ReplicaRefWrapper::new(BoxReplicaRef { inner: downcast }))
    }

    fn component_of_kind<'a>(
        &'a self,
        entity: &E,
        component_kind: &ComponentKind,
    ) -> Option<ReplicaDynRefWrapper<'a>> {
        let boxed = self.components.get(&(*entity, *component_kind))?;
        Some(ReplicaDynRefWrapper::new(BoxReplicaDynRef {
            inner: boxed.as_ref(),
        }))
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    use std::any::Any;
    use std::collections::HashSet;

    use naia_serde::{BitReader, BitWrite, SerdeErr};

    use crate::named::Named;
    use crate::world::component::component_kinds::{ComponentKind, ComponentKinds};
    use crate::world::component::property_mutate::PropertyMutator;
    use crate::world::component::replica_ref::{ReplicaDynMut, ReplicaDynRef};
    use crate::world::component::replicate::{Replicate, ReplicateBuilder, SplitUpdateResult};
    use crate::world::delegation::auth_channel::EntityAuthAccessor;
    use crate::world::entity::entity_converters::{
        LocalEntityAndGlobalEntityConverter, LocalEntityAndGlobalEntityConverterMut,
    };
    use crate::world::update::component_update::{ComponentFieldUpdate, PendingComponentUpdate};
    use crate::world::update::diff_mask::DiffMask;
    use crate::RemoteEntity;

    // ----- Minimal Replicate stubs (no derive macros needed in this crate) -----

    struct TestPosition {
        x: i32,
        y: i32,
    }
    struct TestPositionBuilder;
    struct TestVelocity {
        dx: i32,
    }
    struct TestVelocityBuilder;

    // bevy_support requires `Component` for ReplicatedComponent.
    // We provide a `cfg(feature = "bevy_support")` Component impl
    // mirroring the derive-macro output.
    #[cfg(feature = "bevy_support")]
    impl bevy_ecs::component::Component for TestPosition {
        const STORAGE_TYPE: bevy_ecs::component::StorageType =
            bevy_ecs::component::StorageType::Table;
        type Mutability = bevy_ecs::component::Mutable;
    }
    #[cfg(feature = "bevy_support")]
    impl bevy_ecs::component::Component for TestVelocity {
        const STORAGE_TYPE: bevy_ecs::component::StorageType =
            bevy_ecs::component::StorageType::Table;
        type Mutability = bevy_ecs::component::Mutable;
    }

    impl Named for TestPosition {
        fn protocol_name() -> &'static str { "TestPosition" }
        fn name(&self) -> String { "TestPosition".to_string() }
    }
    impl Named for TestPositionBuilder {
        fn protocol_name() -> &'static str { "TestPosition" }
        fn name(&self) -> String { "TestPosition".to_string() }
    }
    impl Named for TestVelocity {
        fn protocol_name() -> &'static str { "TestVelocity" }
        fn name(&self) -> String { "TestVelocity".to_string() }
    }
    impl Named for TestVelocityBuilder {
        fn protocol_name() -> &'static str { "TestVelocity" }
        fn name(&self) -> String { "TestVelocity".to_string() }
    }

    macro_rules! impl_test_builder {
        ($builder:ident, $val:ident) => {
            impl ReplicateBuilder for $builder {
                fn read(
                    &self,
                    _reader: &mut BitReader,
                    _converter: &dyn LocalEntityAndGlobalEntityConverter,
                ) -> Result<Box<dyn Replicate>, SerdeErr> {
                    unreachable!("test builder does not exercise reads")
                }
                fn read_create_update(
                    &self,
                    _reader: &mut BitReader,
                ) -> Result<PendingComponentUpdate, SerdeErr> {
                    unreachable!()
                }
                fn split_update(
                    &self,
                    _converter: &dyn LocalEntityAndGlobalEntityConverter,
                    _update: PendingComponentUpdate,
                ) -> SplitUpdateResult {
                    unreachable!()
                }
                fn box_clone(&self) -> Box<dyn ReplicateBuilder> {
                    Box::new($builder)
                }
            }
            impl Replicate for $val {
                fn kind(&self) -> ComponentKind { ComponentKind::of::<$val>() }
                fn to_any(&self) -> &dyn Any { self }
                fn to_any_mut(&mut self) -> &mut dyn Any { self }
                fn to_boxed_any(self: Box<Self>) -> Box<dyn Any> { self }
                fn copy_to_box(&self) -> Box<dyn Replicate> {
                    // Field-by-field copy avoids requiring Clone bound on the type itself.
                    Box::new(unsafe { std::ptr::read(self as *const $val) })
                }
                fn create_builder() -> Box<dyn ReplicateBuilder>
                where Self: Sized
                { Box::new($builder) }
                fn diff_mask_size(&self) -> u8 { 0 }
                fn dyn_ref(&self) -> ReplicaDynRef<'_> { ReplicaDynRef::new(self) }
                fn dyn_mut(&mut self) -> ReplicaDynMut<'_> { ReplicaDynMut::new(self) }
                fn mirror(&mut self, _other: &dyn Replicate) {}
                fn mirror_single_field(&mut self, _idx: u8, _other: &dyn Replicate) {}
                fn set_mutator(&mut self, _m: &PropertyMutator) {}
                fn write(
                    &self,
                    _ck: &ComponentKinds,
                    _w: &mut dyn BitWrite,
                    _c: &mut dyn LocalEntityAndGlobalEntityConverterMut,
                ) {}
                fn write_update(
                    &self,
                    _dm: &DiffMask,
                    _w: &mut dyn BitWrite,
                    _c: &mut dyn LocalEntityAndGlobalEntityConverterMut,
                ) {}
                fn read_apply_update(
                    &mut self,
                    _c: &dyn LocalEntityAndGlobalEntityConverter,
                    _u: PendingComponentUpdate,
                ) -> Result<(), SerdeErr> { Ok(()) }
                fn read_apply_field_update(
                    &mut self,
                    _c: &dyn LocalEntityAndGlobalEntityConverter,
                    _u: ComponentFieldUpdate,
                ) -> Result<(), SerdeErr> { Ok(()) }
                fn relations_waiting(&self) -> Option<HashSet<RemoteEntity>> { None }
                fn relations_complete(&mut self, _c: &dyn LocalEntityAndGlobalEntityConverter) {}
                fn publish(&mut self, _m: &PropertyMutator) {}
                fn unpublish(&mut self) {}
                fn enable_delegation(
                    &mut self,
                    _a: &EntityAuthAccessor,
                    _m: Option<&PropertyMutator>,
                ) {}
                fn disable_delegation(&mut self) {}
                fn localize(&mut self) {}
            }
        };
    }

    impl_test_builder!(TestPositionBuilder, TestPosition);
    impl_test_builder!(TestVelocityBuilder, TestVelocity);

    // Use `u64` as the test entity type — Copy + Eq + Hash + Send + Sync.
    type E = u64;

    #[test]
    fn new_snapshot_is_empty() {
        let snap: SnapshotWorld<E> = SnapshotWorld::new();
        assert_eq!(snap.component_count(), 0);
        assert_eq!(snap.live_entity_count(), 0);
        assert!(snap.entities().is_empty());
    }

    #[test]
    fn insert_component_marks_entity_live() {
        let mut snap: SnapshotWorld<E> = SnapshotWorld::new();
        let e: E = 7;
        snap.insert_component(e, ComponentKind::of::<TestPosition>(),
                              Box::new(TestPosition { x: 1, y: 2 }));
        assert!(snap.has_entity(&e));
        assert_eq!(snap.live_entity_count(), 1);
        assert_eq!(snap.component_count(), 1);
    }

    #[test]
    fn has_component_and_has_component_of_kind() {
        let mut snap: SnapshotWorld<E> = SnapshotWorld::new();
        let e: E = 42;
        snap.insert_component(e, ComponentKind::of::<TestPosition>(),
                              Box::new(TestPosition { x: 10, y: 20 }));

        assert!(snap.has_component::<TestPosition>(&e));
        assert!(!snap.has_component::<TestVelocity>(&e));
        assert!(snap.has_component_of_kind(&e, &ComponentKind::of::<TestPosition>()));
        assert!(!snap.has_component_of_kind(&e, &ComponentKind::of::<TestVelocity>()));

        let absent: E = 99;
        assert!(!snap.has_component::<TestPosition>(&absent));
        assert!(!snap.has_component_of_kind(&absent, &ComponentKind::of::<TestPosition>()));
    }

    #[test]
    fn component_returns_downcast_typed_ref() {
        let mut snap: SnapshotWorld<E> = SnapshotWorld::new();
        let e: E = 1;
        snap.insert_component(e, ComponentKind::of::<TestPosition>(),
                              Box::new(TestPosition { x: 5, y: 9 }));

        let r = snap.component::<TestPosition>(&e).expect("component present");
        assert_eq!(r.x, 5);
        assert_eq!(r.y, 9);

        let v = snap.component::<TestVelocity>(&e);
        assert!(v.is_none(), "wrong-typed lookup must return None");
    }

    #[test]
    fn component_of_kind_returns_dyn_ref() {
        let mut snap: SnapshotWorld<E> = SnapshotWorld::new();
        let e: E = 2;
        snap.insert_component(e, ComponentKind::of::<TestVelocity>(),
                              Box::new(TestVelocity { dx: 100 }));

        let dyn_ref = snap
            .component_of_kind(&e, &ComponentKind::of::<TestVelocity>())
            .expect("present");
        // Verify we can downcast back via Replicate::to_any.
        let any = dyn_ref.to_any();
        let v = any.downcast_ref::<TestVelocity>().expect("downcast");
        assert_eq!(v.dx, 100);
    }

    #[test]
    fn mark_live_without_component() {
        let mut snap: SnapshotWorld<E> = SnapshotWorld::new();
        let e: E = 50;
        snap.mark_live(e);

        assert!(snap.has_entity(&e));
        assert_eq!(snap.live_entity_count(), 1);
        assert_eq!(snap.component_count(), 0);
        assert!(!snap.has_component::<TestPosition>(&e));
    }

    #[test]
    fn mark_despawned_cascades_component_removal() {
        let mut snap: SnapshotWorld<E> = SnapshotWorld::new();
        let e: E = 3;
        snap.insert_component(e, ComponentKind::of::<TestPosition>(),
                              Box::new(TestPosition { x: 1, y: 1 }));
        snap.insert_component(e, ComponentKind::of::<TestVelocity>(),
                              Box::new(TestVelocity { dx: 9 }));
        let other: E = 4;
        snap.insert_component(other, ComponentKind::of::<TestPosition>(),
                              Box::new(TestPosition { x: 7, y: 7 }));

        assert_eq!(snap.component_count(), 3);
        assert_eq!(snap.live_entity_count(), 2);

        snap.mark_despawned(e);

        assert!(!snap.has_entity(&e));
        assert!(!snap.has_component::<TestPosition>(&e));
        assert!(!snap.has_component::<TestVelocity>(&e));
        // Other entity untouched.
        assert!(snap.has_entity(&other));
        assert!(snap.has_component::<TestPosition>(&other));
        assert_eq!(snap.live_entity_count(), 1);
        assert_eq!(snap.component_count(), 1);
    }

    #[test]
    fn remove_component_leaves_entity_live() {
        let mut snap: SnapshotWorld<E> = SnapshotWorld::new();
        let e: E = 11;
        snap.insert_component(e, ComponentKind::of::<TestPosition>(),
                              Box::new(TestPosition { x: 1, y: 1 }));
        snap.insert_component(e, ComponentKind::of::<TestVelocity>(),
                              Box::new(TestVelocity { dx: 2 }));

        snap.remove_component(e, &ComponentKind::of::<TestPosition>());

        assert!(snap.has_entity(&e), "entity stays live");
        assert!(!snap.has_component::<TestPosition>(&e));
        assert!(snap.has_component::<TestVelocity>(&e));
        assert_eq!(snap.component_count(), 1);
    }

    #[test]
    fn entities_returns_all_live() {
        let mut snap: SnapshotWorld<E> = SnapshotWorld::new();
        snap.mark_live(1);
        snap.mark_live(2);
        snap.insert_component(3, ComponentKind::of::<TestPosition>(),
                              Box::new(TestPosition { x: 0, y: 0 }));

        let mut es = snap.entities();
        es.sort();
        assert_eq!(es, vec![1, 2, 3]);
    }

    // Verify SnapshotWorld<E> is Send + Sync (required by
    // send_all_packets<W: WorldRefType<E> + Sync>).
    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn snapshot_world_is_send_sync() {
        assert_send_sync::<SnapshotWorld<u64>>();
    }
}
