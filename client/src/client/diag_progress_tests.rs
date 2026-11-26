use naia_shared::{
    ComponentFieldUpdate, ComponentKinds, EntityAndGlobalEntityConverter, GlobalWorldManagerType,
    LocalEntityAndGlobalEntityConverter, PendingComponentUpdate, ReplicaDynMutWrapper,
    ReplicaDynRefWrapper, ReplicaMutWrapper, ReplicaRefWrapper, SerdeErr,
};

use super::*;

/// Minimal world: the idle/disconnected `process_all_packets` path this
/// module exercises returns before touching the world at all, so every
/// `WorldMutType` method it doesn't reach is `unimplemented!()`.
struct StubWorld;

impl WorldRefType<u64> for StubWorld {
    fn has_entity(&self, _world_entity: &u64) -> bool {
        false
    }
    fn entities(&self) -> Vec<u64> {
        Vec::new()
    }
    fn has_component<R: ReplicatedComponent>(&self, _world_entity: &u64) -> bool {
        false
    }
    fn has_component_of_kind(&self, _world_entity: &u64, _component_kind: ComponentKind) -> bool {
        false
    }
    fn component<'a, R: ReplicatedComponent>(
        &'a self,
        _entity: &u64,
    ) -> Option<ReplicaRefWrapper<'a, R>> {
        None
    }
    fn component_of_kind<'a>(
        &'a self,
        _entity: &u64,
        _component_kind: ComponentKind,
    ) -> Option<ReplicaDynRefWrapper<'a>> {
        None
    }
}

impl WorldMutType<u64> for StubWorld {
    fn spawn_entity(&mut self) -> u64 {
        panic!("stub world: not reachable in this test")
    }
    fn local_duplicate_entity(&mut self, _world_entity: &u64) -> u64 {
        panic!("stub world: not reachable in this test")
    }
    fn local_duplicate_components(&mut self, _mutable_entity: &u64, _immutable_entity: &u64) {
        panic!("stub world: not reachable in this test")
    }
    fn despawn_entity(&mut self, _world_entity: &u64) {
        panic!("stub world: not reachable in this test")
    }
    fn component_kinds(&mut self, _world_entity: &u64) -> Vec<ComponentKind> {
        panic!("stub world: not reachable in this test")
    }
    fn component_mut<'a, R: ReplicatedComponent>(
        &'a mut self,
        _world_entity: &u64,
    ) -> Option<ReplicaMutWrapper<'a, R>> {
        panic!("stub world: not reachable in this test")
    }
    fn component_mut_of_kind<'a>(
        &'a mut self,
        _world_entity: &u64,
        _component_kind: ComponentKind,
    ) -> Option<ReplicaDynMutWrapper<'a>> {
        panic!("stub world: not reachable in this test")
    }
    fn component_apply_update(
        &mut self,
        _converter: &dyn LocalEntityAndGlobalEntityConverter,
        _world_entity: &u64,
        _component_kind: ComponentKind,
        _update: PendingComponentUpdate,
    ) -> Result<(), SerdeErr> {
        panic!("stub world: not reachable in this test")
    }
    fn component_apply_field_update(
        &mut self,
        _converter: &dyn LocalEntityAndGlobalEntityConverter,
        _world_entity: &u64,
        _component_kind: ComponentKind,
        _update: ComponentFieldUpdate,
    ) -> Result<(), SerdeErr> {
        panic!("stub world: not reachable in this test")
    }
    fn mirror_entities(&mut self, _mutable_entity: &u64, _immutable_entity: &u64) {
        panic!("stub world: not reachable in this test")
    }
    fn mirror_components(
        &mut self,
        _mutable_entity: &u64,
        _immutable_entity: &u64,
        _component_kind: ComponentKind,
    ) {
        panic!("stub world: not reachable in this test")
    }
    fn insert_component<R: ReplicatedComponent>(&mut self, _world_entity: &u64, _component_ref: R) {
        panic!("stub world: not reachable in this test")
    }
    fn insert_boxed_component(
        &mut self,
        _world_entity: &u64,
        _boxed_component: Box<dyn Replicate>,
    ) {
        panic!("stub world: not reachable in this test")
    }
    fn remove_component<R: ReplicatedComponent>(&mut self, _world_entity: &u64) -> Option<R> {
        panic!("stub world: not reachable in this test")
    }
    fn remove_component_of_kind(
        &mut self,
        _world_entity: &u64,
        _component_kind: ComponentKind,
    ) -> Option<Box<dyn Replicate>> {
        panic!("stub world: not reachable in this test")
    }
    fn entity_publish(
        &mut self,
        _component_kinds: &ComponentKinds,
        _converter: &dyn EntityAndGlobalEntityConverter<u64>,
        _global_world_manager: &dyn GlobalWorldManagerType,
        _world_entity: &u64,
    ) {
        panic!("stub world: not reachable in this test")
    }
    fn component_publish(
        &mut self,
        _component_kinds: &ComponentKinds,
        _converter: &dyn EntityAndGlobalEntityConverter<u64>,
        _global_world_manager: &dyn GlobalWorldManagerType,
        _world_entity: &u64,
        _component_kind: ComponentKind,
    ) {
        panic!("stub world: not reachable in this test")
    }
    fn entity_unpublish(&mut self, _world_entity: &u64) {
        panic!("stub world: not reachable in this test")
    }
    fn component_unpublish(&mut self, _world_entity: &u64, _component_kind: ComponentKind) {
        panic!("stub world: not reachable in this test")
    }
    fn entity_enable_delegation(
        &mut self,
        _component_kinds: &ComponentKinds,
        _converter: &dyn EntityAndGlobalEntityConverter<u64>,
        _global_world_manager: &dyn GlobalWorldManagerType,
        _world_entity: &u64,
    ) {
        panic!("stub world: not reachable in this test")
    }
    fn component_enable_delegation(
        &mut self,
        _component_kinds: &ComponentKinds,
        _converter: &dyn EntityAndGlobalEntityConverter<u64>,
        _global_world_manager: &dyn GlobalWorldManagerType,
        _world_entity: &u64,
        _component_kind: ComponentKind,
    ) {
        panic!("stub world: not reachable in this test")
    }
    fn entity_disable_delegation(&mut self, _world_entity: &u64) {
        panic!("stub world: not reachable in this test")
    }
    fn component_disable_delegation(
        &mut self,
        _world_entity: &u64,
        _component_kind: ComponentKind,
    ) {
        panic!("stub world: not reachable in this test")
    }
}

#[test]
fn poll_and_process_calls_count_every_invocation_without_consuming() {
    let mut client = Client::<u64>::new(ClientConfig::default(), Protocol::builder().build());

    assert_eq!(client.diag_progress().poll_calls, 0);
    assert_eq!(client.diag_progress().process_calls, 0);

    client.receive_all_packets();
    client.receive_all_packets();
    client.process_all_packets(StubWorld, &Instant::now());

    let snapshot = client.diag_progress();
    assert_eq!(
        snapshot.poll_calls, 2,
        "receive_all_packets must count every call, including no-op polls",
    );
    assert_eq!(
        snapshot.process_calls, 1,
        "process_all_packets must count every call, including the disconnected no-op path",
    );

    // Reading twice must not reset anything: non-consuming.
    let snapshot_again = client.diag_progress();
    assert_eq!(
        snapshot_again.poll_calls, 2,
        "diag_progress must be non-consuming"
    );
    assert_eq!(
        snapshot_again.process_calls, 1,
        "diag_progress must be non-consuming"
    );

    client.receive_all_packets();
    assert_eq!(
        client.diag_progress().poll_calls,
        3,
        "counts must keep accumulating across calls",
    );
}
