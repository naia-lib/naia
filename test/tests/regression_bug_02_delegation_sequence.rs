/// REGRESSION TEST FOR BUG #2: Delegation command sequencing error
///
/// THE BUG: Server sent MigrateResponse before EnableDelegation, causing the client's
/// AuthChannel to not be in Delegated state when MigrateResponse arrived.
///
/// ROOT CAUSE: Incorrect ordering in enable_delegation_client_owned_entity():
/// - Wrong: host_send_migrate_response() → migrate_entity() → host_send_enable_delegation()
/// - Right: migrate_entity() → host_send_enable_delegation() → host_send_migrate_response()
///
/// THE SYMPTOM: Server panicked with:
/// "Cannot send MigrateResponse for Entity that is not delegated"
///
/// THE FIX: Reordered operations to ensure AuthChannel transitions to Delegated before MigrateResponse.
///
/// This test documents the correct sequencing.
use naia_shared::{
    BigMapKey, EntityCommand, GlobalEntity, HostEntity, HostEntityChannel, HostType, RemoteEntity,
};

/// Test correct delegation sequence: EnableDelegation → MigrateResponse
#[test]
fn bug_02_delegation_sequence_correct_order() {
    let global_entity = GlobalEntity::from_u64(2001);
    let old_remote_entity = RemoteEntity::new(200);
    let new_host_entity = HostEntity::new(300);

    let mut host_channel = HostEntityChannel::new(HostType::Server);

    // CORRECT SEQUENCE (post-fix):
    // Step 1: EnableDelegation (transitions AuthChannel to Delegated)
    host_channel.send_command(EntityCommand::EnableDelegation(Some(1), global_entity));

    // Step 2: MigrateResponse (valid because entity is now Delegated)
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        host_channel.send_command(EntityCommand::MigrateResponse(
            Some(2),
            global_entity,
            old_remote_entity,
            new_host_entity,
        ));
    }));

    assert!(
        result.is_ok(),
        "BUG #2: MigrateResponse should succeed after EnableDelegation. \
         Before fix, sending MigrateResponse first would panic."
    );
}

/// Test that sending MigrateResponse before EnableDelegation fails
#[test]
#[should_panic(expected = "not delegated")]
fn bug_02_wrong_sequence_panics() {
    let global_entity = GlobalEntity::from_u64(2002);
    let old_remote_entity = RemoteEntity::new(201);
    let new_host_entity = HostEntity::new(301);

    let mut host_channel = HostEntityChannel::new(HostType::Server);

    // WRONG SEQUENCE (pre-fix behavior):
    // Try to send MigrateResponse WITHOUT EnableDelegation first
    // This should panic because AuthChannel is not in Delegated state
    host_channel.send_command(EntityCommand::MigrateResponse(
        Some(1),
        global_entity,
        old_remote_entity,
        new_host_entity,
    ));
}

/// Test complete server-side delegation sequence
#[test]
fn bug_02_complete_server_delegation_sequence() {
    let global_entity = GlobalEntity::from_u64(2003);
    let old_remote_entity = RemoteEntity::new(202);
    let new_host_entity = HostEntity::new(302);

    let mut host_channel = HostEntityChannel::new(HostType::Server);

    // Complete sequence as it should happen on server:
    // 1. Entity exists as Published (default for Server)
    // 2. Migrate entity (internal operation - not a command)
    // 3. Enable delegation
    host_channel.send_command(EntityCommand::EnableDelegation(Some(1), global_entity));
    // 4. Send MigrateResponse
    host_channel.send_command(EntityCommand::MigrateResponse(
        Some(2),
        global_entity,
        old_remote_entity,
        new_host_entity,
    ));

    let commands = host_channel.extract_outgoing_commands();
    assert_eq!(
        commands.len(),
        2,
        "Both delegation commands should be buffered"
    );

    // Verify order
    assert!(matches!(commands[0], EntityCommand::EnableDelegation(_, _)));
    assert!(matches!(
        commands[1],
        EntityCommand::MigrateResponse(_, _, _, _)
    ));
}
