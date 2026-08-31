//! A real in-memory `WorldMutType` for the unit suites.
//!
//! Several `world` types take a `&mut impl WorldMutType<E>` and genuinely use
//! it -- applying decoded updates, reading the resulting value back, spawning
//! and despawning. A stub whose methods are `unreachable!()` cannot drive those
//! paths, so this is a working world: a map of entity id to component map,
//! keyed on `u64`. It mirrors the demo world in `demos/demo_utils/demo_world`,
//! which is not reachable from this crate.

use std::{any::Any, collections::HashMap};

use naia_serde::{BitReader, BitWriter, SerdeErr};

use crate::{
    world::component::replica_ref::{
        ReplicaDynMutWrapper, ReplicaDynRefWrapper, ReplicaMutTrait, ReplicaMutWrapper,
        ReplicaRefTrait, ReplicaRefWrapper,
    },
    BigMapKey, ComponentFieldUpdate, ComponentKind, ComponentKinds, DiffMask,
    EntityAndGlobalEntityConverter, EntityDoesNotExistError, FakeEntityConverter, GlobalEntity,
    GlobalWorldManagerType, LocalEntityAndGlobalEntityConverter, PendingComponentUpdate, Replicate,
    ReplicatedComponent, WorldMutType, WorldRefType,
};

/// The world entity type these suites use.
pub type TestEntity = u64;

/// An in-memory world holding boxed components per entity.
#[derive(Default)]
pub struct TestWorld {
    entities: HashMap<TestEntity, HashMap<ComponentKind, Box<dyn Replicate>>>,
    next_id: TestEntity,
}

impl TestWorld {
    /// Creates an empty world.
    pub fn new() -> Self {
        Self::default()
    }

    /// Spawns `entity` with no components, so a test can choose the id rather
    /// than take whichever one `spawn_entity` hands out.
    pub fn spawn_at(&mut self, entity: TestEntity) {
        self.entities.insert(entity, HashMap::new());
        self.next_id = self.next_id.max(entity + 1);
    }

    /// Reads a component's value back out, for asserting what an apply did.
    pub fn value_of<R: ReplicatedComponent>(&self, entity: &TestEntity) -> Option<&R> {
        self.entities
            .get(entity)?
            .get(&ComponentKind::of::<R>())?
            .to_any()
            .downcast_ref::<R>()
    }

    fn dyn_mut_of_kind(
        &mut self,
        entity: &TestEntity,
        component_kind: &ComponentKind,
    ) -> Option<ReplicaDynMutWrapper<'_>> {
        let component = self.entities.get_mut(entity)?.get_mut(component_kind)?;
        Some(ReplicaDynMutWrapper::new(component.dyn_mut()))
    }
}

struct Ref<'a, R: Replicate>(&'a R);

impl<'a, R: Replicate> ReplicaRefTrait<R> for Ref<'a, R> {
    fn to_ref(&self) -> &R {
        self.0
    }
}

struct Mut<'a, R: Replicate>(&'a mut R);

impl<'a, R: Replicate> ReplicaRefTrait<R> for Mut<'a, R> {
    fn to_ref(&self) -> &R {
        self.0
    }
}

impl<'a, R: Replicate> ReplicaMutTrait<R> for Mut<'a, R> {
    fn to_mut(&mut self) -> &mut R {
        self.0
    }
}

impl WorldRefType<TestEntity> for TestWorld {
    fn has_entity(&self, entity: &TestEntity) -> bool {
        self.entities.contains_key(entity)
    }

    fn entities(&self) -> Vec<TestEntity> {
        self.entities.keys().copied().collect()
    }

    fn has_component<R: ReplicatedComponent>(&self, entity: &TestEntity) -> bool {
        self.has_component_of_kind(entity, &ComponentKind::of::<R>())
    }

    fn has_component_of_kind(&self, entity: &TestEntity, component_kind: &ComponentKind) -> bool {
        self.entities
            .get(entity)
            .is_some_and(|map| map.contains_key(component_kind))
    }

    fn component<'a, R: ReplicatedComponent>(
        &'a self,
        entity: &TestEntity,
    ) -> Option<ReplicaRefWrapper<'a, R>> {
        let raw = self.value_of::<R>(entity)?;
        Some(ReplicaRefWrapper::new(Ref(raw)))
    }

    fn component_of_kind<'a>(
        &'a self,
        entity: &TestEntity,
        component_kind: &ComponentKind,
    ) -> Option<ReplicaDynRefWrapper<'a>> {
        let component = self.entities.get(entity)?.get(component_kind)?;
        Some(ReplicaDynRefWrapper::new(component.dyn_ref()))
    }
}

impl WorldMutType<TestEntity> for TestWorld {
    fn spawn_entity(&mut self) -> TestEntity {
        let entity = self.next_id;
        self.spawn_at(entity);
        entity
    }

    fn local_duplicate_entity(&mut self, entity: &TestEntity) -> TestEntity {
        let new_entity = self.spawn_entity();
        self.local_duplicate_components(&new_entity, entity);
        new_entity
    }

    fn local_duplicate_components(&mut self, new_entity: &TestEntity, old_entity: &TestEntity) {
        for component_kind in WorldMutType::<TestEntity>::component_kinds(self, old_entity) {
            let copied = self
                .component_of_kind(old_entity, &component_kind)
                .map(|component| component.copy_to_box());
            if let Some(boxed) = copied {
                self.insert_boxed_component(new_entity, boxed);
            }
        }
    }

    fn despawn_entity(&mut self, entity: &TestEntity) {
        self.entities.remove(entity);
    }

    fn component_kinds(&mut self, entity: &TestEntity) -> Vec<ComponentKind> {
        self.entities
            .get(entity)
            .map(|map| map.keys().copied().collect())
            .unwrap_or_default()
    }

    fn component_mut<'a, R: ReplicatedComponent>(
        &'a mut self,
        entity: &TestEntity,
    ) -> Option<ReplicaMutWrapper<'a, R>> {
        let raw = self
            .entities
            .get_mut(entity)?
            .get_mut(&ComponentKind::of::<R>())?
            .to_any_mut()
            .downcast_mut::<R>()?;
        Some(ReplicaMutWrapper::new(Mut(raw)))
    }

    fn component_mut_of_kind<'a>(
        &'a mut self,
        entity: &TestEntity,
        component_kind: &ComponentKind,
    ) -> Option<ReplicaDynMutWrapper<'a>> {
        self.dyn_mut_of_kind(entity, component_kind)
    }

    fn component_apply_update(
        &mut self,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
        entity: &TestEntity,
        component_kind: &ComponentKind,
        update: PendingComponentUpdate,
    ) -> Result<(), SerdeErr> {
        if let Some(mut component) = self.dyn_mut_of_kind(entity, component_kind) {
            component.read_apply_update(converter, update)?;
        }
        Ok(())
    }

    fn component_apply_field_update(
        &mut self,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
        entity: &TestEntity,
        component_kind: &ComponentKind,
        update: ComponentFieldUpdate,
    ) -> Result<(), SerdeErr> {
        if let Some(mut component) = self.dyn_mut_of_kind(entity, component_kind) {
            component.read_apply_field_update(converter, update)?;
        }
        Ok(())
    }

    fn mirror_entities(&mut self, new_entity: &TestEntity, old_entity: &TestEntity) {
        for component_kind in WorldMutType::<TestEntity>::component_kinds(self, old_entity) {
            self.mirror_components(new_entity, old_entity, &component_kind);
        }
    }

    fn mirror_components(
        &mut self,
        mutable_entity: &TestEntity,
        immutable_entity: &TestEntity,
        component_kind: &ComponentKind,
    ) {
        let copied = self
            .entities
            .get(immutable_entity)
            .and_then(|map| map.get(component_kind))
            .map(|component| component.copy_to_box());
        let Some(source) = copied else {
            return;
        };
        if let Some(target) = self
            .entities
            .get_mut(mutable_entity)
            .and_then(|map| map.get_mut(component_kind))
        {
            target.mirror(source.as_ref());
        }
    }

    fn insert_component<R: ReplicatedComponent>(&mut self, entity: &TestEntity, component: R) {
        self.insert_boxed_component(entity, Box::new(component));
    }

    fn insert_boxed_component(&mut self, entity: &TestEntity, boxed_component: Box<dyn Replicate>) {
        let Some(map) = self.entities.get_mut(entity) else {
            return;
        };
        let component_kind = boxed_component.kind();
        if map.contains_key(&component_kind) {
            panic!("Entity already has a Component of that type!");
        }
        map.insert(component_kind, boxed_component);
    }

    fn remove_component<R: ReplicatedComponent>(&mut self, entity: &TestEntity) -> Option<R> {
        let boxed = self.remove_component_of_kind(entity, &ComponentKind::of::<R>())?;
        Box::<dyn Any + 'static>::downcast::<R>(boxed.to_boxed_any())
            .ok()
            .map(|boxed| *boxed)
    }

    fn remove_component_of_kind(
        &mut self,
        entity: &TestEntity,
        component_kind: &ComponentKind,
    ) -> Option<Box<dyn Replicate>> {
        self.entities.get_mut(entity)?.remove(component_kind)
    }

    fn entity_publish(
        &mut self,
        component_kinds: &ComponentKinds,
        converter: &dyn EntityAndGlobalEntityConverter<TestEntity>,
        global_world_manager: &dyn GlobalWorldManagerType,
        entity: &TestEntity,
    ) {
        for component_kind in WorldMutType::<TestEntity>::component_kinds(self, entity) {
            self.component_publish(
                component_kinds,
                converter,
                global_world_manager,
                entity,
                &component_kind,
            );
        }
    }

    fn component_publish(
        &mut self,
        component_kinds: &ComponentKinds,
        converter: &dyn EntityAndGlobalEntityConverter<TestEntity>,
        global_world_manager: &dyn GlobalWorldManagerType,
        entity: &TestEntity,
        component_kind: &ComponentKind,
    ) {
        let Ok(global_entity) = converter.entity_to_global_entity(entity) else {
            return;
        };
        let Some(component) = self
            .entities
            .get_mut(entity)
            .and_then(|map| map.get_mut(component_kind))
        else {
            return;
        };
        let diff_mask_size = component.diff_mask_size();
        let mutator = global_world_manager.register_component(
            component_kinds,
            &global_entity,
            component_kind,
            diff_mask_size,
        );
        component.publish(&mutator);
    }

    fn entity_unpublish(&mut self, entity: &TestEntity) {
        for component_kind in WorldMutType::<TestEntity>::component_kinds(self, entity) {
            self.component_unpublish(entity, &component_kind);
        }
    }

    fn component_unpublish(&mut self, entity: &TestEntity, component_kind: &ComponentKind) {
        if let Some(component) = self
            .entities
            .get_mut(entity)
            .and_then(|map| map.get_mut(component_kind))
        {
            component.unpublish();
        }
    }

    fn entity_enable_delegation(
        &mut self,
        component_kinds: &ComponentKinds,
        converter: &dyn EntityAndGlobalEntityConverter<TestEntity>,
        global_world_manager: &dyn GlobalWorldManagerType,
        entity: &TestEntity,
    ) {
        for component_kind in WorldMutType::<TestEntity>::component_kinds(self, entity) {
            self.component_enable_delegation(
                component_kinds,
                converter,
                global_world_manager,
                entity,
                &component_kind,
            );
        }
    }

    fn component_enable_delegation(
        &mut self,
        component_kinds: &ComponentKinds,
        converter: &dyn EntityAndGlobalEntityConverter<TestEntity>,
        global_world_manager: &dyn GlobalWorldManagerType,
        entity: &TestEntity,
        component_kind: &ComponentKind,
    ) {
        let Ok(global_entity) = converter.entity_to_global_entity(entity) else {
            return;
        };
        let accessor = global_world_manager.get_entity_auth_accessor(&global_entity);
        let needs_mutator =
            global_world_manager.entity_needs_mutator_for_delegation(&global_entity);
        let Some(mut component) = self.dyn_mut_of_kind(entity, component_kind) else {
            return;
        };
        let mutator_opt = if needs_mutator {
            let diff_mask_size = component.diff_mask_size();
            Some(global_world_manager.register_component(
                component_kinds,
                &global_entity,
                component_kind,
                diff_mask_size,
            ))
        } else {
            None
        };
        component.enable_delegation(&accessor, mutator_opt.as_ref());
    }

    fn entity_disable_delegation(&mut self, entity: &TestEntity) {
        for component_kind in WorldMutType::<TestEntity>::component_kinds(self, entity) {
            self.component_disable_delegation(entity, &component_kind);
        }
    }

    fn component_disable_delegation(
        &mut self,
        entity: &TestEntity,
        component_kind: &ComponentKind,
    ) {
        if let Some(mut component) = self.dyn_mut_of_kind(entity, component_kind) {
            component.disable_delegation();
        }
    }
}

/// Maps a [`GlobalEntity`] to a [`TestEntity`] and back by raw id, so a suite
/// can use the same number on both sides and read assertions at a glance.
pub struct IdentityConverter;

impl EntityAndGlobalEntityConverter<TestEntity> for IdentityConverter {
    fn global_entity_to_entity(
        &self,
        global_entity: &GlobalEntity,
    ) -> Result<TestEntity, EntityDoesNotExistError> {
        Ok(global_entity.to_u64())
    }

    fn entity_to_global_entity(
        &self,
        entity: &TestEntity,
    ) -> Result<GlobalEntity, EntityDoesNotExistError> {
        Ok(GlobalEntity::from_u64(*entity))
    }
}

/// Serializes `component`'s full state as a wire update and reads it back, so a
/// test can feed a genuinely decoded [`PendingComponentUpdate`] into the
/// incoming buffers rather than a hand-built stand-in.
pub fn full_update<R: ReplicatedComponent>(
    component_kinds: &ComponentKinds,
    component: &R,
) -> PendingComponentUpdate {
    let mut writer = BitWriter::new();
    ComponentKind::of::<R>().ser(component_kinds, &mut writer);

    let mut diff_mask = DiffMask::new(component.diff_mask_size());
    for index in 0..(diff_mask.byte_number() * 8) {
        diff_mask.set_bit(index, true);
    }
    component.write_update(&diff_mask, &mut writer, &mut FakeEntityConverter);

    let bytes = writer.to_bytes();
    let mut reader = BitReader::new(&bytes);
    component_kinds
        .read_create_update(&mut reader)
        .expect("a freshly written update should read back")
}

/// Serializes `component` and reads it back through `component_kinds`, which is
/// the only way to obtain a REMOTE-owned component. A locally constructed one
/// is host-owned and panics on `read`, so a world meant to receive updates must
/// be seeded through this rather than with `insert_component` directly.
pub fn remote_component<R: ReplicatedComponent>(
    component_kinds: &ComponentKinds,
    component: &R,
) -> Box<dyn Replicate> {
    let mut writer = BitWriter::new();
    component.write(component_kinds, &mut writer, &mut FakeEntityConverter);
    let bytes = writer.to_bytes();
    let mut reader = BitReader::new(&bytes);
    component_kinds
        .read(&mut reader, &FakeEntityConverter)
        .expect("a freshly written component should read back")
}

/// A [`GlobalEntitySpawner`] with identity semantics: the `GlobalEntity`'s raw
/// u64 IS the `TestEntity`. That matches [`IdentityConverter`], so a test can
/// mint `GlobalEntity`s directly and still have the remote world manager
/// resolve them without a prior `spawn` call.
pub struct TestSpawner {
    reserved: HashMap<crate::RemoteEntity, GlobalEntity>,
}

impl TestSpawner {
    pub fn new() -> Self {
        Self {
            reserved: HashMap::new(),
        }
    }
}

impl Default for TestSpawner {
    fn default() -> Self {
        Self::new()
    }
}

impl EntityAndGlobalEntityConverter<TestEntity> for TestSpawner {
    fn global_entity_to_entity(
        &self,
        global_entity: &GlobalEntity,
    ) -> Result<TestEntity, EntityDoesNotExistError> {
        Ok(global_entity.to_u64())
    }

    fn entity_to_global_entity(
        &self,
        entity: &TestEntity,
    ) -> Result<GlobalEntity, EntityDoesNotExistError> {
        Ok(GlobalEntity::from_u64(*entity))
    }
}

impl crate::GlobalEntitySpawner<TestEntity> for TestSpawner {
    fn spawn(
        &mut self,
        world_entity: TestEntity,
        remote_entity_opt: Option<crate::RemoteEntity>,
    ) -> GlobalEntity {
        if let Some(remote_entity) = remote_entity_opt {
            self.reserved.remove(&remote_entity);
        }
        GlobalEntity::from_u64(world_entity)
    }

    fn reserve_global_entity(&mut self, remote_entity: crate::RemoteEntity) -> GlobalEntity {
        let global_entity = GlobalEntity::from_u64(remote_entity.value() as u64);
        self.reserved.insert(remote_entity, global_entity);
        global_entity
    }

    fn despawn_by_global(&mut self, _global_entity: &GlobalEntity) {}

    fn despawn_by_world(&mut self, _world_entity: &TestEntity) {}

    fn to_converter(&self) -> &dyn EntityAndGlobalEntityConverter<TestEntity> {
        self
    }
}
