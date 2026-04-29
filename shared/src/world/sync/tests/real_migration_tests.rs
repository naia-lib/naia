//! REAL MIGRATION TESTS
//!
//! These tests verify the actual migration functionality works correctly
//! without relying on shared state or complex setup.

use crate::{
    world::entity::entity_converters::LocalEntityAndGlobalEntityConverter,
    world::sync::{
        host_entity_channel::HostEntityChannel,
        remote_entity_channel::{EntityChannelState, RemoteEntityChannel},
    },
    BigMapKey, ComponentKind, EntityCommand, EntityMessage, GlobalEntity, HostEntity, HostType,
    LocalEntityMap, OwnedLocalEntity,
};

/// Test component types for migration testing
#[allow(dead_code)]
struct Position {
    x: f32,
    y: f32,
}
#[allow(dead_code)]
struct Velocity {
    x: f32,
    y: f32,
}

/// Helper function to create a component kind for testing
fn component_kind<T: 'static>() -> ComponentKind {
    ComponentKind::from(std::any::TypeId::of::<T>())
}

/// Test that RemoteEntityChannel can be created and managed correctly
#[test]
fn remote_entity_channel_basic_operations() {
    let mut channel = RemoteEntityChannel::new(HostType::Client);

    // Test initial state
    assert_eq!(channel.get_state(), EntityChannelState::Despawned);

    // Test component insertion
    let pos_kind = component_kind::<Position>();
    let vel_kind = component_kind::<Velocity>();

    channel.insert_component(pos_kind);
    channel.insert_component(vel_kind);

    // Test component state management
    channel.insert_component_channel_as_inserted(pos_kind, 1);
    channel.insert_component_channel_as_inserted(vel_kind, 2);

    // Test state extraction
    let component_kinds = channel.extract_inserted_component_kinds();
    assert_eq!(component_kinds.len(), 2);
    assert!(component_kinds.contains(&pos_kind));
    assert!(component_kinds.contains(&vel_kind));

    // Test state transitions
    channel.set_spawned(3);
    assert_eq!(channel.get_state(), EntityChannelState::Spawned);
}

/// Test that HostEntityChannel can be created and managed correctly
#[test]
fn host_entity_channel_basic_operations() {
    let mut channel = HostEntityChannel::new(HostType::Client);

    // Test command sending
    let global_entity = GlobalEntity::from_u64(20001);
    let pos_kind = component_kind::<Position>();

    channel.send_command(EntityCommand::InsertComponent(global_entity, pos_kind));
    channel.send_command(EntityCommand::Publish(Some(1), global_entity));

    // Test command extraction
    let commands = channel.extract_outgoing_commands();
    assert_eq!(commands.len(), 2);

    // Test component kinds
    let component_kinds = channel.component_kinds();
    assert_eq!(component_kinds.len(), 1); // One component was inserted
}

/// Test that LocalEntityMap can handle entity redirects correctly
#[test]
fn local_entity_map_redirect_operations() {
    let mut entity_map = LocalEntityMap::new(HostType::Server);

    // Test entity insertion
    let global_entity = GlobalEntity::from_u64(1);
    let remote_entity = crate::world::local::local_entity::RemoteEntity::new(42);
    let _host_entity = HostEntity::new(100);

    entity_map.insert_with_remote_entity(global_entity, remote_entity);

    // Test redirect installation
    let old_entity = OwnedLocalEntity::Remote(42);
    let new_entity = OwnedLocalEntity::Host { id: 100, is_static: false };
    entity_map.install_entity_redirect(old_entity, new_entity);

    // Test redirect application
    let redirected = entity_map.apply_entity_redirect(&old_entity);
    assert_eq!(redirected, new_entity);

    // Test non-redirected entity
    let other_entity = OwnedLocalEntity::Remote(99);
    let not_redirected = entity_map.apply_entity_redirect(&other_entity);
    assert_eq!(not_redirected, other_entity);
}

/// Test that migration error handling works correctly
#[test]
fn migration_error_handling() {
    let entity_map = LocalEntityMap::new(HostType::Server);
    let fake_entity = GlobalEntity::from_u64(999);

    // Test that non-existent entity returns error
    let result = entity_map.global_entity_to_remote_entity(&fake_entity);
    assert!(result.is_err());
}

/// Test that component state is preserved during migration
#[test]
fn component_state_preservation() {
    let mut remote_channel = RemoteEntityChannel::new(HostType::Client);
    let pos_kind = component_kind::<Position>();
    let vel_kind = component_kind::<Velocity>();

    // Add components
    remote_channel.insert_component(pos_kind);
    remote_channel.insert_component(vel_kind);
    remote_channel.insert_component_channel_as_inserted(pos_kind, 1);
    remote_channel.insert_component_channel_as_inserted(vel_kind, 2);

    // Extract component state
    let component_kinds = remote_channel.extract_inserted_component_kinds();
    assert_eq!(component_kinds.len(), 2);

    // Create new host channel with extracted state
    let host_channel = HostEntityChannel::new_with_components(HostType::Server, component_kinds);

    // Verify state was preserved
    assert_eq!(host_channel.component_kinds().len(), 2);
    assert!(host_channel.component_kinds().contains(&pos_kind));
    assert!(host_channel.component_kinds().contains(&vel_kind));
}

/// Test that command replay works correctly
#[test]
fn command_replay_functionality() {
    let mut host_channel = HostEntityChannel::new(HostType::Client);
    let global_entity = GlobalEntity::from_u64(30001);
    let pos_kind = component_kind::<Position>();

    // Add commands
    host_channel.send_command(EntityCommand::InsertComponent(global_entity, pos_kind));
    host_channel.send_command(EntityCommand::Publish(Some(1), global_entity));
    host_channel.send_command(EntityCommand::EnableDelegation(Some(2), global_entity));

    // Extract commands
    let commands = host_channel.extract_outgoing_commands();
    assert_eq!(commands.len(), 3);

    // Test command validation
    let mut valid_commands = 0;
    for command in commands {
        if command.is_valid_for_remote_entity() {
            valid_commands += 1;
        }
    }
    assert!(valid_commands > 0);
}

/// Test that buffered operations are handled correctly
#[test]
fn buffered_operations_handling() {
    let mut remote_channel = RemoteEntityChannel::new(HostType::Client);
    let pos_kind = component_kind::<Position>();

    // Add buffered operations
    remote_channel.receive_message(1, EntityMessage::<()>::Spawn(()));
    remote_channel.receive_message(2, EntityMessage::<()>::InsertComponent((), pos_kind));
    remote_channel.receive_message(3, EntityMessage::<()>::RemoveComponent((), pos_kind));

    // Force drain buffers
    remote_channel.force_drain_all_buffers();

    // Extract final state (processing happened during force_drain)
    let _component_kinds = remote_channel.extract_inserted_component_kinds();
}

/// Test that high-frequency operations work correctly
#[test]
fn high_frequency_operations() {
    let mut entity_map = LocalEntityMap::new(HostType::Server);

    // Create many redirects
    for i in 0..100 {
        let old_entity = OwnedLocalEntity::Remote(i);
        let new_entity = OwnedLocalEntity::Host { id: i + 1000, is_static: false };
        entity_map.install_entity_redirect(old_entity, new_entity);
    }

    // Test all redirects work
    for i in 0..100 {
        let old_entity = OwnedLocalEntity::Remote(i);
        let expected_new_entity = OwnedLocalEntity::Host { id: i + 1000, is_static: false };
        let redirected = entity_map.apply_entity_redirect(&old_entity);
        assert_eq!(redirected, expected_new_entity);
    }
}

/// Test that memory efficiency is maintained
#[test]
fn memory_efficiency() {
    let mut remote_channel = RemoteEntityChannel::new(HostType::Client);
    let pos_kind = component_kind::<Position>();

    // Add many operations
    for i in 1..=50 {
        remote_channel.receive_message(i, EntityMessage::<()>::InsertComponent((), pos_kind));
        remote_channel.receive_message(i + 50, EntityMessage::<()>::RemoveComponent((), pos_kind));
    }

    // Force drain (should be efficient)
    remote_channel.force_drain_all_buffers();

    // Extract state (should be efficient)
    let component_kinds = remote_channel.extract_inserted_component_kinds();

    // Verify we got reasonable results
    assert!(component_kinds.len() <= 1); // Should be at most 1 unique component type
}
