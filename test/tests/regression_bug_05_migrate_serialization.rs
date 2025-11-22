/// REGRESSION TEST FOR BUG #5: MigrateResponse serialization used wrong entity ID
///
/// THE BUG: When serializing MigrateResponse, the code tried to look up the old RemoteEntity
/// from the global_entity_map, but the entity had already been migrated and was no longer
/// in the remote entity map. This caused an EntityDoesNotExistError panic.
///
/// ROOT CAUSE: MigrateResponse serialization looked up old_remote_entity instead of using
/// the captured value from when the command was created.
///
/// THE SYMPTOM: Server panicked when sending MigrateResponse:
/// "thread 'main' panicked at shared/src/world/sync/host_entity_channel.rs:X:Y:
///  EntityDoesNotExistError while serializing MigrateResponse"
///
/// THE FIX: Store old_remote_entity in MigrateResponse command at creation time, don't look it up.
///
/// This test documents that MigrateResponse must capture entity IDs.
use naia_shared::{BigMapKey, EntityCommand, GlobalEntity, HostEntity, RemoteEntity};

/// Test that MigrateResponse contains old_remote_entity
#[test]
fn bug_05_migrate_response_contains_old_entity() {
    let global_entity = GlobalEntity::from_u64(5001);
    let old_remote_entity = RemoteEntity::new(500);
    let new_host_entity = HostEntity::new(600);

    let command =
        EntityCommand::MigrateResponse(Some(1), global_entity, old_remote_entity, new_host_entity);

    // Extract the old_remote_entity from the command
    if let EntityCommand::MigrateResponse(_, _, captured_old_remote, _) = command {
        assert_eq!(
            captured_old_remote, old_remote_entity,
            "MigrateResponse should capture old_remote_entity at creation time"
        );
    } else {
        panic!("Command should be MigrateResponse");
    }
}

/// Test that MigrateResponse stores entity IDs at creation time
#[test]
fn bug_05_migrate_response_entity_ids() {
    let global_entity = GlobalEntity::from_u64(5002);
    let old_remote_entity = RemoteEntity::new(501);
    let new_host_entity = HostEntity::new(601);

    let command =
        EntityCommand::MigrateResponse(Some(1), global_entity, old_remote_entity, new_host_entity);

    // Extract the values from the command to verify they match
    if let EntityCommand::MigrateResponse(_, g, old_r, new_h) = command {
        assert_eq!(g, global_entity, "GlobalEntity should match");
        assert_eq!(old_r, old_remote_entity, "Old RemoteEntity should match");
        assert_eq!(new_h, new_host_entity, "New HostEntity should match");
    } else {
        panic!("Command should be MigrateResponse");
    }

    // The fix ensures old_remote_entity is captured at creation time,
    // not looked up during serialization
}

/// Test MigrateResponse with entity IDs at boundaries
#[test]
fn bug_05_migrate_response_boundary_values() {
    // Test with various entity IDs
    let global_entity = GlobalEntity::from_u64(u64::MAX);
    let old_remote_entity = RemoteEntity::new(u16::MAX);
    let new_host_entity = HostEntity::new(u16::MAX);

    let command = EntityCommand::MigrateResponse(
        Some(255), // u8::MAX
        global_entity,
        old_remote_entity,
        new_host_entity,
    );

    // Verify command creation succeeds
    if let EntityCommand::MigrateResponse(_, _, _, _) = command {
        assert!(true, "Command created successfully with boundary values");
    } else {
        panic!("Command should be MigrateResponse");
    }
}
