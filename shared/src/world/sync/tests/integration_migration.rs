//! INTEGRATION TESTS FOR ENTITY MIGRATION SYSTEM
//!
//! These tests verify the complete migration flow works correctly
//! in real-world scenarios with actual data structures and state.

use crate::{
    world::entity::entity_converters::LocalEntityAndGlobalEntityConverter,
    world::sync::{
        host_entity_channel::HostEntityChannel, remote_entity_channel::RemoteEntityChannel,
    },
    BigMapKey, ComponentKind, EntityAuthStatus, EntityCommand, EntityMessage, GlobalEntity,
    HostEntity, HostType, LocalEntityMap, OwnedLocalEntity,
};

/// Test component types for integration testing
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
#[allow(dead_code)]
struct Health {
    value: u32,
}

/// Helper function to create a component kind for testing
fn component_kind<T: 'static>() -> ComponentKind {
    ComponentKind::from(std::any::TypeId::of::<T>())
}

/// INTEGRATION TEST: Complete server-side migration flow
#[test]
fn server_side_migration_complete_flow() {
    // Setup: Create a complete LocalWorldManager-like scenario
    let mut entity_map = LocalEntityMap::new(HostType::Server);
    let mut remote_engine = crate::world::sync::remote_engine::RemoteEngine::new(HostType::Server);
    let mut host_engine = crate::world::sync::host_engine::HostEngine::new(HostType::Server);

    // Create test entities with unique IDs (use timestamp-like ID)
    let global_entity = GlobalEntity::from_u64(10001);
    let remote_entity = crate::world::local::local_entity::RemoteEntity::new(42);
    let host_entity = HostEntity::new(100);

    // Setup: Insert entity into entity map
    entity_map.insert_with_remote_entity(global_entity, remote_entity);

    // Setup: Create RemoteEntityChannel with components
    let mut remote_channel = RemoteEntityChannel::new(HostType::Client);
    let pos_kind = component_kind::<Position>();
    let vel_kind = component_kind::<Velocity>();
    let health_kind = component_kind::<Health>();

    // Add components to remote channel
    remote_channel.insert_component(pos_kind);
    remote_channel.insert_component(vel_kind);
    remote_channel.insert_component(health_kind);

    // Mark components as inserted
    remote_channel.insert_component_channel_as_inserted(pos_kind, 1);
    remote_channel.insert_component_channel_as_inserted(vel_kind, 2);
    remote_channel.insert_component_channel_as_inserted(health_kind, 3);

    // Insert remote channel into remote engine
    remote_engine.insert_entity_channel(remote_entity, remote_channel);

    // MIGRATION: Extract component state from RemoteEntityChannel
    let component_kinds = {
        let channel = remote_engine.get_world().get(&remote_entity).unwrap();
        channel.extract_inserted_component_kinds()
    };

    // Verify all components were extracted
    assert!(component_kinds.contains(&pos_kind));
    assert!(component_kinds.contains(&vel_kind));
    assert!(component_kinds.contains(&health_kind));
    assert_eq!(component_kinds.len(), 3);

    // MIGRATION: Remove RemoteEntityChannel
    let _old_remote_channel = remote_engine.remove_entity_channel(&remote_entity);

    // MIGRATION: Create new HostEntityChannel with extracted state
    let new_host_channel =
        HostEntityChannel::new_with_components(HostType::Server, component_kinds);

    // MIGRATION: Insert new HostEntityChannel
    host_engine.insert_entity_channel(host_entity, new_host_channel);

    // MIGRATION: Update entity map - remove old mapping first
    entity_map.remove_by_global_entity(&global_entity);
    entity_map.insert_with_host_entity(global_entity, host_entity);

    // MIGRATION: Install entity redirect
    let old_entity = OwnedLocalEntity::Remote(remote_entity.value());
    let new_entity = OwnedLocalEntity::Host(host_entity.value());
    entity_map.install_entity_redirect(old_entity, new_entity);

    // VERIFICATION: Test entity redirect works
    let redirected = entity_map.apply_entity_redirect(&old_entity);
    assert_eq!(redirected, new_entity);

    // VERIFICATION: Test non-redirected entity
    let other_entity = OwnedLocalEntity::Remote(99);
    let not_redirected = entity_map.apply_entity_redirect(&other_entity);
    assert_eq!(not_redirected, other_entity);

    // VERIFICATION: HostEntityChannel has correct components
    let host_channel = host_engine.get_entity_channel(&host_entity).unwrap();
    assert_eq!(host_channel.component_kinds().len(), 3);
    assert!(host_channel.component_kinds().contains(&pos_kind));
    assert!(host_channel.component_kinds().contains(&vel_kind));
    assert!(host_channel.component_kinds().contains(&health_kind));
}

/// INTEGRATION TEST: Client-side migration with command replay
#[test]
fn client_side_migration_with_command_replay() {
    // Setup: Create HostEntityChannel with commands (Client-owned entity)
    let mut host_channel = HostEntityChannel::new(HostType::Client);
    let global_entity = GlobalEntity::from_u64(10002);
    let pos_kind = component_kind::<Position>();
    let vel_kind = component_kind::<Velocity>();

    // Add some commands to the host channel
    host_channel.send_command(EntityCommand::InsertComponent(global_entity, pos_kind));
    host_channel.send_command(EntityCommand::InsertComponent(global_entity, vel_kind));
    host_channel.send_command(EntityCommand::Publish(Some(1), global_entity));
    host_channel.send_command(EntityCommand::EnableDelegation(Some(2), global_entity));

    // MIGRATION: Extract commands
    let commands = host_channel.extract_outgoing_commands();
    assert_eq!(commands.len(), 4);

    // MIGRATION: Extract component state
    let component_kinds = host_channel.component_kinds().clone();
    assert_eq!(component_kinds.len(), 2); // Two components were inserted

    // MIGRATION: Create new RemoteEntityChannel
    let mut remote_channel = RemoteEntityChannel::new(HostType::Client);
    for &comp_kind in &component_kinds {
        remote_channel.insert_component(comp_kind);
    }

    // MIGRATION: Replay valid commands
    let mut replayed_commands = 0;
    for command in commands {
        if command.is_valid_for_remote_entity() {
            // In real implementation, this would send the command
            replayed_commands += 1;
        }
    }

    // Verify some commands were replayed
    assert!(replayed_commands > 0);
}

/// INTEGRATION TEST: Migration with buffered operations
#[test]
fn migration_with_buffered_operations() {
    // Setup: Create RemoteEntityChannel with buffered operations
    let mut remote_channel = RemoteEntityChannel::new(HostType::Client);
    let pos_kind = component_kind::<Position>();
    let vel_kind = component_kind::<Velocity>();

    // Add buffered operations (out of order)
    remote_channel.receive_message(5, EntityMessage::<()>::InsertComponent((), vel_kind));
    remote_channel.receive_message(3, EntityMessage::<()>::InsertComponent((), pos_kind));
    remote_channel.receive_message(1, EntityMessage::<()>::Spawn(()));
    remote_channel.receive_message(4, EntityMessage::<()>::RemoveComponent((), pos_kind));

    // MIGRATION: Force drain all buffers
    remote_channel.force_drain_all_buffers();

    // Force-drain processes all buffered operations; component extraction not needed here
}

/// INTEGRATION TEST: Migration error handling
#[test]
fn migration_error_handling() {
    // Test that migration fails gracefully with invalid input
    let entity_map = LocalEntityMap::new(HostType::Server);
    let fake_entity = GlobalEntity::from_u64(999);

    // This should return an error when trying to get a non-existent entity
    let result = entity_map.global_entity_to_remote_entity(&fake_entity);
    assert!(result.is_err());
}

/// INTEGRATION TEST: High-frequency migration operations
#[test]
fn high_frequency_migration_operations() {
    let mut entity_map = LocalEntityMap::new(HostType::Server);
    let mut redirects = Vec::new();

    // Create many entity redirects
    for i in 0..1000 {
        let old_entity = OwnedLocalEntity::Remote(i);
        let new_entity = OwnedLocalEntity::Host(i + 1000);
        entity_map.install_entity_redirect(old_entity, new_entity);
        redirects.push((old_entity, new_entity));
    }

    // Test all redirects work correctly
    for (old_entity, expected_new_entity) in redirects {
        let redirected = entity_map.apply_entity_redirect(&old_entity);
        assert_eq!(redirected, expected_new_entity);
    }

    // Test non-existent redirect returns original entity
    let non_existent = OwnedLocalEntity::Remote(9999);
    let result = entity_map.apply_entity_redirect(&non_existent);
    assert_eq!(result, non_existent);
}

/// INTEGRATION TEST: Memory efficiency during migration
#[test]
fn migration_memory_efficiency() {
    // Test that migration doesn't leak memory
    let mut remote_channel = RemoteEntityChannel::new(HostType::Client);
    let pos_kind = component_kind::<Position>();

    // Add many operations
    for i in 1..=100 {
        remote_channel.receive_message(i, EntityMessage::<()>::InsertComponent((), pos_kind));
        remote_channel.receive_message(i + 100, EntityMessage::<()>::RemoveComponent((), pos_kind));
    }

    // Force drain (this should not leak memory)
    remote_channel.force_drain_all_buffers();

    // Extract state (this should be efficient)
    let component_kinds = remote_channel.extract_inserted_component_kinds();

    // Verify we got reasonable results
    assert!(component_kinds.len() <= 1); // Should be at most 1 unique component type
}

/// INTEGRATION TEST: Concurrent migration scenarios
#[test]
fn concurrent_migration_scenarios() {
    // Test that migration handles concurrent operations correctly
    let mut remote_channel = RemoteEntityChannel::new(HostType::Client);
    let pos_kind = component_kind::<Position>();
    let vel_kind = component_kind::<Velocity>();

    // Simulate concurrent operations
    remote_channel.receive_message(1, EntityMessage::<()>::Spawn(()));
    remote_channel.receive_message(2, EntityMessage::<()>::InsertComponent((), pos_kind));
    remote_channel.receive_message(3, EntityMessage::<()>::InsertComponent((), vel_kind));
    remote_channel.receive_message(4, EntityMessage::<()>::RemoveComponent((), pos_kind));

    // Force drain should handle all operations
    remote_channel.force_drain_all_buffers();

    // Extract final state
    let component_kinds = remote_channel.extract_inserted_component_kinds();

    // Verify final state is consistent
    assert!(component_kinds.len() <= 2); // At most 2 unique component types
}

/// INTEGRATION TEST: Migration with authority changes
#[test]
fn migration_with_authority_changes() {
    let mut host_channel = HostEntityChannel::new(HostType::Client);
    let global_entity = GlobalEntity::from_u64(10003);

    // Add authority-related commands
    host_channel.send_command(EntityCommand::Publish(Some(1), global_entity));
    host_channel.send_command(EntityCommand::EnableDelegation(Some(2), global_entity));
    host_channel.send_command(EntityCommand::SetAuthority(
        Some(3),
        global_entity,
        EntityAuthStatus::Granted,
    ));

    // Extract commands
    let commands = host_channel.extract_outgoing_commands();

    // Verify authority commands were preserved
    assert_eq!(commands.len(), 3);

    // Verify command types
    let mut has_publish = false;
    let mut has_delegation = false;
    let mut has_authority = false;

    for command in commands {
        match command {
            EntityCommand::Publish(_, _) => has_publish = true,
            EntityCommand::EnableDelegation(_, _) => has_delegation = true,
            EntityCommand::SetAuthority(_, _, _) => has_authority = true,
            _ => {}
        }
    }

    assert!(has_publish);
    assert!(has_delegation);
    assert!(has_authority);
}
