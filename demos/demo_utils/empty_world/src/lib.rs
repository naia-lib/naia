use naia_shared::{
    ComponentFieldUpdate, ComponentKind, ComponentKinds, ComponentUpdate,
    EntityAndGlobalEntityConverter, GlobalWorldManagerType, LocalEntityAndGlobalEntityConverter,
    ReplicaDynMutWrapper, ReplicaDynRefWrapper, ReplicaMutWrapper, ReplicaRefWrapper, Replicate,
    ReplicatedComponent, SerdeErr, WorldMutType, WorldRefType,
};

pub type EmptyEntity = u8;

#[derive(Default)]
pub struct EmptyWorldRef;

#[derive(Default)]
pub struct EmptyWorldMut;

impl WorldRefType<EmptyEntity> for EmptyWorldRef {
    fn has_entity(&self, _world_entity: &EmptyEntity) -> bool {
        unimplemented!()
    }

    fn entities(&self) -> Vec<EmptyEntity> {
        unimplemented!()
    }

    fn has_component<R: ReplicatedComponent>(&self, _world_entity: &EmptyEntity) -> bool {
        unimplemented!()
    }

    fn has_component_of_kind(
        &self,
        _world_entity: &EmptyEntity,
        _component_kind: &ComponentKind,
    ) -> bool {
        unimplemented!()
    }

    fn component<'a, R: ReplicatedComponent>(
        &'a self,
        _world_entity: &EmptyEntity,
    ) -> Option<ReplicaRefWrapper<'a, R>> {
        unimplemented!()
    }

    fn component_of_kind<'a>(
        &'a self,
        _world_entity: &EmptyEntity,
        _component_kind: &ComponentKind,
    ) -> Option<ReplicaDynRefWrapper<'a>> {
        unimplemented!()
    }
}

impl WorldRefType<EmptyEntity> for EmptyWorldMut {
    fn has_entity(&self, _world_entity: &EmptyEntity) -> bool {
        unimplemented!()
    }

    fn entities(&self) -> Vec<EmptyEntity> {
        unimplemented!()
    }

    fn has_component<R: ReplicatedComponent>(&self, _world_entity: &EmptyEntity) -> bool {
        unimplemented!()
    }

    fn has_component_of_kind(
        &self,
        _world_entity: &EmptyEntity,
        _component_kind: &ComponentKind,
    ) -> bool {
        unimplemented!()
    }

    fn component<'a, R: ReplicatedComponent>(
        &'a self,
        _world_entity: &EmptyEntity,
    ) -> Option<ReplicaRefWrapper<'a, R>> {
        unimplemented!()
    }

    fn component_of_kind<'a>(
        &'a self,
        _world_entity: &EmptyEntity,
        _component_kind: &ComponentKind,
    ) -> Option<ReplicaDynRefWrapper<'a>> {
        unimplemented!()
    }
}

impl WorldMutType<EmptyEntity> for EmptyWorldMut {
    fn spawn_entity(&mut self) -> EmptyEntity {
        unimplemented!()
    }

    fn local_duplicate_entity(&mut self, _world_entity: &EmptyEntity) -> EmptyEntity {
        unimplemented!()
    }

    fn local_duplicate_components(
        &mut self,
        _mutable_entity: &EmptyEntity,
        _immutable_entity: &EmptyEntity,
    ) {
        unimplemented!()
    }

    fn despawn_entity(&mut self, _world_entity: &EmptyEntity) {
        unimplemented!()
    }

    fn component_kinds(&mut self, _world_entity: &EmptyEntity) -> Vec<ComponentKind> {
        unimplemented!()
    }

    fn component_mut<'a, R: ReplicatedComponent>(
        &'a mut self,
        _world_entity: &EmptyEntity,
    ) -> Option<ReplicaMutWrapper<'a, R>> {
        unimplemented!()
    }

    fn component_mut_of_kind<'a>(
        &'a mut self,
        _world_entity: &EmptyEntity,
        _component_kind: &ComponentKind,
    ) -> Option<ReplicaDynMutWrapper<'a>> {
        unimplemented!()
    }

    fn component_apply_update(
        &mut self,
        _converter: &dyn LocalEntityAndGlobalEntityConverter,
        _world_entity: &EmptyEntity,
        _component_kind: &ComponentKind,
        _update: ComponentUpdate,
    ) -> Result<(), SerdeErr> {
        unimplemented!()
    }

    fn component_apply_field_update(
        &mut self,
        _converter: &dyn LocalEntityAndGlobalEntityConverter,
        _world_entity: &EmptyEntity,
        _component_kind: &ComponentKind,
        _update: ComponentFieldUpdate,
    ) -> Result<(), SerdeErr> {
        unimplemented!()
    }

    fn mirror_entities(&mut self, _mutable_entity: &EmptyEntity, _immutable_entity: &EmptyEntity) {
        unimplemented!()
    }

    fn mirror_components(
        &mut self,
        _mutable_entity: &EmptyEntity,
        _immutable_entity: &EmptyEntity,
        _component_kind: &ComponentKind,
    ) {
        unimplemented!()
    }

    fn insert_component<R: ReplicatedComponent>(
        &mut self,
        _world_entity: &EmptyEntity,
        _component_ref: R,
    ) {
        unimplemented!()
    }

    fn insert_boxed_component(
        &mut self,
        _world_entity: &EmptyEntity,
        _boxed_component: Box<dyn Replicate>,
    ) {
        unimplemented!()
    }

    fn remove_component<R: ReplicatedComponent>(
        &mut self,
        _world_entity: &EmptyEntity,
    ) -> Option<R> {
        unimplemented!()
    }

    fn remove_component_of_kind(
        &mut self,
        _world_entity: &EmptyEntity,
        _component_kind: &ComponentKind,
    ) -> Option<Box<dyn Replicate>> {
        unimplemented!()
    }

    fn entity_publish(
        &mut self,
        _component_kinds: &ComponentKinds,
        _converter: &dyn EntityAndGlobalEntityConverter<EmptyEntity>,
        _global_world_manager: &dyn GlobalWorldManagerType,
        _world_entity: &EmptyEntity,
    ) {
        unimplemented!()
    }

    fn component_publish(
        &mut self,
        _component_kinds: &ComponentKinds,
        _converter: &dyn EntityAndGlobalEntityConverter<EmptyEntity>,
        _global_world_manager: &dyn GlobalWorldManagerType,
        _world_entity: &EmptyEntity,
        _component_kind: &ComponentKind,
    ) {
        unimplemented!()
    }

    fn entity_unpublish(&mut self, _world_entity: &EmptyEntity) {
        unimplemented!()
    }

    fn component_unpublish(
        &mut self,
        _world_entity: &EmptyEntity,
        _component_kind: &ComponentKind,
    ) {
        unimplemented!()
    }

    fn entity_enable_delegation(
        &mut self,
        _component_kinds: &ComponentKinds,
        _converter: &dyn EntityAndGlobalEntityConverter<EmptyEntity>,
        _global_world_manager: &dyn GlobalWorldManagerType,
        _world_entity: &EmptyEntity,
    ) {
        unimplemented!()
    }

    fn component_enable_delegation(
        &mut self,
        _component_kinds: &ComponentKinds,
        _converter: &dyn EntityAndGlobalEntityConverter<EmptyEntity>,
        _global_world_manager: &dyn GlobalWorldManagerType,
        _world_entity: &EmptyEntity,
        _component_kind: &ComponentKind,
    ) {
        unimplemented!()
    }

    fn entity_disable_delegation(&mut self, _world_entity: &EmptyEntity) {
        unimplemented!()
    }

    fn component_disable_delegation(
        &mut self,
        _world_entity: &EmptyEntity,
        _component_kind: &ComponentKind,
    ) {
        unimplemented!()
    }
}
