// Regression test for Bug #11: Client-created delegated entity channel not in Delegated state
//
// **Bug Description:**
// When a client creates a delegated entity and calls `.enable_delegation()`, the local
// `HostEntityChannel`'s `AuthChannel` needs to transition through the proper states:
// Unpublished → Published → Delegated
//
// **Why This Wasn't Caught:**
// Previous tests didn't simulate the complete delegation flow, specifically:
// 1. Entities must be Published before they can be Delegated
// 2. Both Publish and EnableDelegation commands must have SubCommandIds assigned
// 3. The channel state must be correct to receive MigrateResponse from the server
//
// **The Fix:**
// Modified `LocalWorldManager::send_enable_delegation()` to send BOTH:
// 1. Publish command (Unpublished → Published)
// 2. EnableDelegation command (Published → Delegated)

use naia_shared::{
    BigMapKey, EntityAuthChannelState, EntityCommand, EntityMessageType, GlobalEntity, HostEntity,
    HostEntityChannel, HostType, RemoteEntity,
};

/// Test correct state transitions: Unpublished → Published → Delegated
#[test]
fn bug_11_delegation_requires_publish_first() {
    // Create a HostEntityChannel (starts in Unpublished state)
    let mut channel = HostEntityChannel::new(HostType::Client);
    let global_entity = GlobalEntity::from_u64(1);

    // Verify it starts Unpublished
    assert_eq!(
        channel.auth_channel_state(),
        EntityAuthChannelState::Unpublished
    );

    // Send Publish command
    let publish_cmd = EntityCommand::Publish(Some(1), global_entity);
    channel.send_command(publish_cmd);

    // Verify it's now Published
    assert_eq!(
        channel.auth_channel_state(),
        EntityAuthChannelState::Published
    );

    // Send EnableDelegation command
    let enable_delegation_cmd = EntityCommand::EnableDelegation(Some(2), global_entity);
    channel.send_command(enable_delegation_cmd);

    // Verify it's now Delegated
    assert_eq!(
        channel.auth_channel_state(),
        EntityAuthChannelState::Delegated
    );
    assert!(channel.is_delegated());
}

/// Test that MigrateResponse can be received after proper delegation setup
#[test]
fn bug_11_migrate_response_validates_after_publish_and_delegation() {
    let mut channel = HostEntityChannel::new(HostType::Client);
    let global_entity = GlobalEntity::from_u64(1);

    // Proper setup: Publish then EnableDelegation
    channel.send_command(EntityCommand::Publish(Some(1), global_entity));
    channel.send_command(EntityCommand::EnableDelegation(Some(2), global_entity));

    // Now send MigrateResponse - should NOT panic
    let old_remote_entity = RemoteEntity::new(0);
    let new_host_entity = HostEntity::new(100);
    let migrate_response =
        EntityCommand::MigrateResponse(Some(3), global_entity, old_remote_entity, new_host_entity);

    channel.send_command(migrate_response);

    // If we got here without panicking, the test passes
}

/// Test that EnableDelegation panics if entity is not Published first
#[test]
#[should_panic(expected = "Cannot enable delegation on Entity")]
fn bug_11_enable_delegation_requires_publish() {
    let mut channel = HostEntityChannel::new(HostType::Client);
    let global_entity = GlobalEntity::from_u64(1);

    // Try to enable delegation WITHOUT publishing first
    let enable_delegation_cmd = EntityCommand::EnableDelegation(Some(1), global_entity);
    channel.send_command(enable_delegation_cmd);

    // Should panic
}

/// Test that Publish and EnableDelegation commands are sent correctly
#[test]
fn bug_11_both_commands_are_sent() {
    let mut channel = HostEntityChannel::new(HostType::Client);
    let global_entity = GlobalEntity::from_u64(1);

    // Send commands with SubCommandIds (simulating real usage)
    channel.send_command(EntityCommand::Publish(Some(1), global_entity));
    channel.send_command(EntityCommand::EnableDelegation(Some(2), global_entity));

    // Extract commands
    let commands = channel.extract_outgoing_commands();

    // Verify both commands were queued
    assert_eq!(
        commands.len(),
        2,
        "Should have 2 commands: Publish and EnableDelegation"
    );

    // Verify command types
    assert_eq!(commands[0].get_type(), EntityMessageType::Publish);
    assert_eq!(commands[1].get_type(), EntityMessageType::EnableDelegation);
}

/// Test that enabling delegation on an already-Published entity doesn't try to Publish again
/// This was Bug #11d - server-owned entities were already Published when coming into scope
#[test]
fn bug_11d_enable_delegation_on_already_published_entity() {
    let mut channel = HostEntityChannel::new(HostType::Client);
    let global_entity = GlobalEntity::from_u64(1);

    // First, publish the entity (simulating server-owned entity coming into scope)
    channel.send_command(EntityCommand::Publish(Some(1), global_entity));

    // Verify it's Published
    assert_eq!(
        channel.auth_channel_state(),
        EntityAuthChannelState::Published
    );

    // Extract the Publish command
    let first_commands = channel.extract_outgoing_commands();
    assert_eq!(first_commands.len(), 1);

    // Now enable delegation on the ALREADY published entity
    // This should NOT send Publish again, only EnableDelegation
    channel.send_command(EntityCommand::EnableDelegation(Some(2), global_entity));

    // Verify it's now Delegated
    assert_eq!(
        channel.auth_channel_state(),
        EntityAuthChannelState::Delegated
    );

    // Extract commands - should only have EnableDelegation
    let second_commands = channel.extract_outgoing_commands();
    assert_eq!(second_commands.len(), 1);
    assert_eq!(
        second_commands[0].get_type(),
        EntityMessageType::EnableDelegation
    );
}
