use naia_shared::{
    BigMapKey, ComponentFieldUpdate, ComponentKinds, PendingComponentUpdate, ReplicaDynMutWrapper,
    ReplicaDynRefWrapper, ReplicaMutWrapper, ReplicaRefWrapper, SerdeErr,
};

use super::*;

/// Minimal world: dropped events return before touching the world, so
/// every `WorldMutType` method is `unimplemented!()` — reaching one
/// would itself prove the event was not dropped.
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

fn fresh_client() -> Client<u64> {
    Client::<u64>::new(ClientConfig::default(), Protocol::builder().build())
}

#[test]
fn rogue_publish_of_a_public_entity_is_dropped() {
    // common-02: remote input must not panic. A server-originated
    // Publish for an already-Public entity contradicts the direction,
    // so it is dropped and the config is untouched.
    let mut client = fresh_client();
    let global_entity = GlobalEntity::from_u64(1);
    client
        .global_world_manager
        .remote_spawn_entity(&global_entity);
    client.publish_entity(&global_entity, false);
    assert_eq!(
        client
            .global_world_manager
            .entity_replication_config(&global_entity),
        Some(Publicity::Public),
        "a dropped publish must not alter the replication config",
    );
}

#[test]
fn rogue_unpublish_of_a_private_entity_is_dropped() {
    // common-02: remote input must not panic. A server-originated
    // Unpublish for an already-Private entity contradicts the
    // direction, so it is dropped and the config is untouched.
    let mut client = fresh_client();
    let global_entity = GlobalEntity::from_u64(1);
    client
        .global_world_manager
        .remote_spawn_entity(&global_entity);
    client.global_world_manager.entity_unpublish(&global_entity);
    client.unpublish_entity(&global_entity, false);
    assert_eq!(
        client
            .global_world_manager
            .entity_replication_config(&global_entity),
        Some(Publicity::Private),
        "a dropped unpublish must not alter the replication config",
    );
}

#[test]
fn rogue_delegation_response_is_dropped() {
    // common-02: remote input must not panic. Only the client sends
    // EnableDelegationResponse; a server echo of it is dropped before
    // it can touch the world (every world method above would abort).
    let mut client = fresh_client();
    let global_entity = GlobalEntity::from_u64(1);
    client.process_entity_events(
        &mut StubWorld,
        vec![(None, EntityEvent::EnableDelegationResponse(global_entity))],
    );
}

#[test]
fn rogue_request_authority_is_dropped() {
    // common-02: remote input must not panic. Only the client sends
    // RequestAuthority; a server echo of it is dropped before it can
    // touch the world.
    let mut client = fresh_client();
    let global_entity = GlobalEntity::from_u64(1);
    client.process_entity_events(
        &mut StubWorld,
        vec![(None, EntityEvent::RequestAuthority(global_entity))],
    );
}

#[test]
fn rogue_release_authority_is_dropped() {
    // common-02: remote input must not panic. Only the client sends
    // ReleaseAuthority; a server echo of it is dropped before it can
    // touch the world.
    let mut client = fresh_client();
    let global_entity = GlobalEntity::from_u64(1);
    client.process_entity_events(
        &mut StubWorld,
        vec![(None, EntityEvent::ReleaseAuthority(global_entity))],
    );
}
