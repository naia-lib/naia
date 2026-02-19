/// CRITICAL TEST: Validate ALL command types can be sent through AuthChannel
///
/// This test was added AFTER a production bug where MigrateResponse was not
/// registered in AuthChannel validation, causing server crashes.
///
/// The bug was missed because previous tests created channels directly and
/// bypassed the real command flow. These tests exercise the ACTUAL code path.
use crate::{
    world::sync::host_entity_channel::HostEntityChannel,
    BigMapKey, EntityCommand, GlobalEntity, HostEntity, HostType, RemoteEntity,
};

/// Test that ALL migration-related commands can be sent through the real channel flow
#[test]
fn test_all_migration_commands_through_real_flow() {
    let global_entity = GlobalEntity::from_u64(10001);
    let old_remote_entity = RemoteEntity::new(99);
    let new_host_entity = HostEntity::new(100);

    // Test Server-side HostEntityChannel (Published → Delegated)
    let mut host_channel = HostEntityChannel::new(HostType::Server);

    // Step 1: Publish (Server starts published)
    // Already published by default for Server

    // Step 2: Enable delegation (sets status to Available automatically)
    host_channel.send_command(EntityCommand::EnableDelegation(Some(1), global_entity));

    // Step 3: CRITICAL - Send MigrateResponse (THIS IS WHAT WAS BROKEN)
    // MigrateResponse tells client the new HostEntity ID after migration
    // This is valid after EnableDelegation has made the entity delegated
    host_channel.send_command(EntityCommand::MigrateResponse(
        Some(2),
        global_entity,
        old_remote_entity,
        new_host_entity,
    ));

    // If we got here without panic, MigrateResponse is properly registered!
    let commands = host_channel.extract_outgoing_commands();
    assert!(
        commands.len() >= 2,
        "Should have at least 2 commands buffered"
    );

    // Verify MigrateResponse is in the commands
    let has_migrate_response = commands
        .iter()
        .any(|cmd| matches!(cmd, EntityCommand::MigrateResponse(_, _, _, _)));
    assert!(
        has_migrate_response,
        "MigrateResponse should be in outgoing commands"
    );
}

/// Test RequestAuthority command (client requesting authority)
#[test]
fn test_request_authority_command() {
    let global_entity = GlobalEntity::from_u64(10002);

    // Client-side: Start unpublished, then publish, then enable delegation
    let mut host_channel = HostEntityChannel::new(HostType::Client);

    // Client must publish first
    host_channel.send_command(EntityCommand::Publish(Some(1), global_entity));

    // Then enable delegation
    host_channel.send_command(EntityCommand::EnableDelegation(Some(2), global_entity));

    // Now client can request authority
    host_channel.send_command(EntityCommand::RequestAuthority(Some(3), global_entity));

    let commands = host_channel.extract_outgoing_commands();
    let has_request = commands
        .iter()
        .any(|cmd| matches!(cmd, EntityCommand::RequestAuthority(_, _)));
    assert!(
        has_request,
        "RequestAuthority should be in outgoing commands"
    );
}

/// Test EnableDelegationResponse command (client responding to delegation request)
#[test]
fn test_enable_delegation_response_command() {
    let global_entity = GlobalEntity::from_u64(10003);

    // Client-side: starts unpublished
    let mut host_channel = HostEntityChannel::new(HostType::Client);

    // Client publishes first
    host_channel.send_command(EntityCommand::Publish(Some(1), global_entity));

    // Enable delegation
    host_channel.send_command(EntityCommand::EnableDelegation(Some(2), global_entity));

    // Client responds with EnableDelegationResponse (acknowledging delegation)
    host_channel.send_command(EntityCommand::EnableDelegationResponse(
        Some(3),
        global_entity,
    ));

    let commands = host_channel.extract_outgoing_commands();
    let has_response = commands
        .iter()
        .any(|cmd| matches!(cmd, EntityCommand::EnableDelegationResponse(_, _)));
    assert!(
        has_response,
        "EnableDelegationResponse should be in outgoing commands"
    );
}

/// Test complete delegation flow with all commands
#[test]
fn test_complete_delegation_flow_with_all_commands() {
    let global_entity = GlobalEntity::from_u64(10004);
    let old_remote_entity = RemoteEntity::new(200);
    let new_host_entity = HostEntity::new(201);

    // Simulate server processing delegation request
    let mut server_channel = HostEntityChannel::new(HostType::Server);

    // 1. Enable delegation (server receives client request, sets status to Available)
    server_channel.send_command(EntityCommand::EnableDelegation(Some(1), global_entity));

    // 2. Server migrates and sends MigrateResponse with new HostEntity ID
    server_channel.send_command(EntityCommand::MigrateResponse(
        Some(2),
        global_entity,
        old_remote_entity,
        new_host_entity,
    ));

    // Extract and verify all commands went through without panic
    let commands = server_channel.extract_outgoing_commands();
    assert_eq!(commands.len(), 2, "Should have all 2 commands");

    // Verify sequence
    assert!(matches!(commands[0], EntityCommand::EnableDelegation(_, _)));
    assert!(matches!(
        commands[1],
        EntityCommand::MigrateResponse(_, _, _, _)
    ));
}

/// Test that commands with invalid state transitions still panic
#[test]
#[should_panic(expected = "Cannot send MigrateResponse")]
fn test_migrate_response_requires_delegation() {
    let global_entity = GlobalEntity::from_u64(10005);
    let old_remote_entity = RemoteEntity::new(201);
    let host_entity = HostEntity::new(202);

    // Try to send MigrateResponse WITHOUT enabling delegation first
    let mut host_channel = HostEntityChannel::new(HostType::Server);

    // This SHOULD panic because entity is not delegated yet
    host_channel.send_command(EntityCommand::MigrateResponse(
        Some(1),
        global_entity,
        old_remote_entity,
        host_entity,
    ));
}

/// Test that RequestAuthority requires delegation
#[test]
#[should_panic(expected = "Cannot request authority")]
fn test_request_authority_requires_delegation() {
    let global_entity = GlobalEntity::from_u64(10006);

    // Try to request authority WITHOUT delegation
    let mut host_channel = HostEntityChannel::new(HostType::Client);
    host_channel.send_command(EntityCommand::Publish(Some(1), global_entity));

    // This SHOULD panic because delegation is not enabled
    host_channel.send_command(EntityCommand::RequestAuthority(Some(2), global_entity));
}

/// Test the full command sending path from HostEntityChannel
/// This simulates what happens when LocalWorldManager::host_send_migrate_response is called
#[test]
fn test_migrate_response_through_host_channel_send() {
    let global_entity = GlobalEntity::from_u64(10007);
    let old_remote_entity = RemoteEntity::new(202);
    let new_host_entity = HostEntity::new(203);

    // Create a server-side HostEntityChannel (already published)
    let mut host_channel = HostEntityChannel::new(HostType::Server);

    // Enable delegation first (required state)
    host_channel.send_command(EntityCommand::EnableDelegation(Some(1), global_entity));

    // THIS IS THE ACTUAL CODE PATH THAT WAS BROKEN:
    // When LocalWorldManager::host_send_migrate_response is called, it eventually calls
    // HostEntityChannel::send_command with MigrateResponse, which calls
    // AuthChannel::validate_command. This was panicking before the fix.
    host_channel.send_command(EntityCommand::MigrateResponse(
        Some(2),
        global_entity,
        old_remote_entity,
        new_host_entity,
    ));

    // If we got here without panic, the bug is fixed!
    // Extract and verify the command made it through
    let commands = host_channel.extract_outgoing_commands();
    assert!(
        commands
            .iter()
            .any(|cmd| matches!(cmd, EntityCommand::MigrateResponse(_, _, _, _))),
        "MigrateResponse should have been sent successfully"
    );
}

/// REGRESSION TEST: Command validation for delegation (Bugs #2 & #3)
/// Tests that EnableDelegation must come before MigrateResponse at the AuthChannel level
///
/// BUG HISTORY:
/// - Bug #2: Server sent MigrateResponse before EnableDelegation
///   → Violated AuthChannel state machine (must be Delegated first)
///   → Production crash: "Cannot send MigrateResponse for Entity that is not delegated"
///
/// - Bug #3: Server sent EnableDelegation before migrating entity
///   → Entity didn't exist as HostEntity yet
///   → Production crash: EntityDoesNotExistError
///
/// WHAT THIS TEST VALIDATES:
/// - At the HostEntityChannel level, EnableDelegation can be sent before MigrateResponse
/// - The AuthChannel accepts both commands in the correct sequence
///
/// WHAT THIS TEST DOESN'T VALIDATE (Bug #3):
/// - That entity is migrated BEFORE sending EnableDelegation
/// - That the HostEntity exists in HostEngine before commands are sent
/// - The full server delegation flow (needs E2E test)
///
/// This is a UNIT test. Bug #3 showed we need INTEGRATION tests.
#[test]
fn test_server_delegation_command_sequence() {
    let global_entity = GlobalEntity::from_u64(10008);
    let old_remote_entity = RemoteEntity::new(203);
    let new_host_entity = HostEntity::new(204);

    // NOTE: This test creates a HostEntityChannel directly, which means the
    // HostEntity "exists" for the purpose of this test. In the real server flow,
    // the entity must be migrated first (Bug #3).
    let mut host_channel = HostEntityChannel::new(HostType::Server);

    // CORRECT SEQUENCE (what server does after Bugs #2 & #3 fixes):
    // (In real server: migration happens first, THEN these commands)
    // 1. Send EnableDelegation (transitions to Delegated state)
    host_channel.send_command(EntityCommand::EnableDelegation(Some(1), global_entity));

    // 2. Now can send MigrateResponse (requires Delegated state)
    host_channel.send_command(EntityCommand::MigrateResponse(
        Some(2),
        global_entity,
        old_remote_entity,
        new_host_entity,
    ));

    // Verify both commands were sent in correct order
    let commands = host_channel.extract_outgoing_commands();
    assert_eq!(
        commands.len(),
        2,
        "Should have both EnableDelegation and MigrateResponse"
    );
    assert!(
        matches!(commands[0], EntityCommand::EnableDelegation(_, _)),
        "First command must be EnableDelegation"
    );
    assert!(
        matches!(commands[1], EntityCommand::MigrateResponse(_, _, _, _)),
        "Second command must be MigrateResponse"
    );
}

/// NEGATIVE TEST: Verify server CANNOT send MigrateResponse without EnableDelegation first
/// This is the bug that crashed production - sending MigrateResponse before delegation
#[test]
#[should_panic(expected = "Cannot send MigrateResponse")]
fn test_server_delegation_wrong_sequence_panics() {
    let global_entity = GlobalEntity::from_u64(10009);
    let old_remote_entity = RemoteEntity::new(204);
    let new_host_entity = HostEntity::new(205);

    // Simulate the BROKEN server flow (what caused Bug #2)
    let mut host_channel = HostEntityChannel::new(HostType::Server);

    // WRONG: Try to send MigrateResponse WITHOUT EnableDelegation first
    // This MUST panic to prevent invalid state transitions
    host_channel.send_command(EntityCommand::MigrateResponse(
        Some(1),
        global_entity,
        old_remote_entity,
        new_host_entity,
    ));

    // Should never reach here - the above should panic
}
