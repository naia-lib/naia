use naia_shared::{
    BigMapKey, EntityAuthStatus, GlobalEntity, HostType, LocalWorldManager, RemoteEntity,
};
use naia_test::TestGlobalWorldManager;
/// Integration tests for authority synchronization
/// These tests verify that authority state stays synchronized across
/// multiple components (Gap #2: Authority Synchronization)
use std::collections::HashSet;

/// Test that authority status sync is maintained through state transitions
/// This is Gap #2 from TEST_COVERAGE_GAPS_AND_FIXES.md
#[test]
fn authority_status_stays_synced() {
    let global_world_manager = TestGlobalWorldManager::new();
    let mut local_world_manager =
        LocalWorldManager::new(&None, HostType::Client, 1, &global_world_manager);

    let global_entity = GlobalEntity::from_u64(1);
    let remote_entity = RemoteEntity::new(100);

    local_world_manager.insert_remote_entity(&global_entity, remote_entity, HashSet::new());

    // Test Available → Requested → Granted
    assert_eq!(
        local_world_manager.get_remote_entity_auth_status(&global_entity),
        Some(EntityAuthStatus::Available)
    );

    local_world_manager.remote_send_request_auth(&global_entity);
    // (Server would process request here)

    local_world_manager.remote_receive_set_auth(&global_entity, EntityAuthStatus::Granted);
    assert_eq!(
        local_world_manager.get_remote_entity_auth_status(&global_entity),
        Some(EntityAuthStatus::Granted),
        "Channel status should match after grant"
    );

    // Test Granted → Released → Available
    local_world_manager.remote_send_release_auth(&global_entity);
    local_world_manager.remote_receive_set_auth(&global_entity, EntityAuthStatus::Available);
    assert_eq!(
        local_world_manager.get_remote_entity_auth_status(&global_entity),
        Some(EntityAuthStatus::Available),
        "Channel status should match after release"
    );
}

/// Test authority synchronization during migration
/// This is the core of Bug #7 - after migration, both trackers must be in sync
#[test]
fn migration_maintains_authority_sync() {
    let global_world_manager = TestGlobalWorldManager::new();
    let mut local_world_manager =
        LocalWorldManager::new(&None, HostType::Client, 1, &global_world_manager);

    let global_entity = GlobalEntity::from_u64(2);
    let remote_entity = RemoteEntity::new(200);

    // Simulate entity after migration
    local_world_manager.insert_remote_entity(&global_entity, remote_entity, HashSet::new());

    // Simulate receiving MigrateResponse with Granted authority
    local_world_manager.remote_receive_set_auth(&global_entity, EntityAuthStatus::Granted);

    // CRITICAL: Verify channel status matches
    let channel_status = local_world_manager.get_remote_entity_auth_status(&global_entity);
    assert_eq!(
        channel_status,
        Some(EntityAuthStatus::Granted),
        "After migration, channel status must be Granted"
    );

    // Verify subsequent operations work (Bug #7 broke this)
    local_world_manager.remote_send_release_auth(&global_entity);
    local_world_manager.remote_receive_set_auth(&global_entity, EntityAuthStatus::Available);

    local_world_manager.remote_send_request_auth(&global_entity);
    local_world_manager.remote_receive_set_auth(&global_entity, EntityAuthStatus::Granted);

    assert_eq!(
        local_world_manager.get_remote_entity_auth_status(&global_entity),
        Some(EntityAuthStatus::Granted),
        "Authority sync should be maintained through complete lifecycle"
    );
}

/// Test authority cycles maintain synchronization
/// Multiple request/release cycles should not cause drift
#[test]
fn authority_cycles_maintain_sync() {
    let global_world_manager = TestGlobalWorldManager::new();
    let mut local_world_manager =
        LocalWorldManager::new(&None, HostType::Client, 1, &global_world_manager);

    let global_entity = GlobalEntity::from_u64(3);
    let remote_entity = RemoteEntity::new(300);

    local_world_manager.insert_remote_entity(&global_entity, remote_entity, HashSet::new());

    // Perform 5 complete cycles to stress-test synchronization
    for cycle in 1..=5 {
        // Request
        local_world_manager.remote_send_request_auth(&global_entity);
        local_world_manager.remote_receive_set_auth(&global_entity, EntityAuthStatus::Granted);

        let status_after_grant = local_world_manager.get_remote_entity_auth_status(&global_entity);
        assert_eq!(
            status_after_grant,
            Some(EntityAuthStatus::Granted),
            "Cycle {}: Status should be Granted after grant",
            cycle
        );

        // Release
        local_world_manager.remote_send_release_auth(&global_entity);
        local_world_manager.remote_receive_set_auth(&global_entity, EntityAuthStatus::Available);

        let status_after_release =
            local_world_manager.get_remote_entity_auth_status(&global_entity);
        assert_eq!(
            status_after_release,
            Some(EntityAuthStatus::Available),
            "Cycle {}: Status should be Available after release",
            cycle
        );
    }
}

/// Test that denial doesn't break synchronization
#[test]
fn authority_denial_maintains_sync() {
    let global_world_manager = TestGlobalWorldManager::new();
    let mut local_world_manager =
        LocalWorldManager::new(&None, HostType::Client, 1, &global_world_manager);

    let global_entity = GlobalEntity::from_u64(4);
    let remote_entity = RemoteEntity::new(400);

    local_world_manager.insert_remote_entity(&global_entity, remote_entity, HashSet::new());

    // Request authority
    local_world_manager.remote_send_request_auth(&global_entity);

    // Server denies (another client has authority)
    local_world_manager.remote_receive_set_auth(&global_entity, EntityAuthStatus::Denied);

    assert_eq!(
        local_world_manager.get_remote_entity_auth_status(&global_entity),
        Some(EntityAuthStatus::Denied),
        "Status should be Denied after denial"
    );

    // After denial, entity becomes Available again
    local_world_manager.remote_receive_set_auth(&global_entity, EntityAuthStatus::Available);

    assert_eq!(
        local_world_manager.get_remote_entity_auth_status(&global_entity),
        Some(EntityAuthStatus::Available),
        "Status should return to Available after Denied → Available transition"
    );

    // Verify can request again after denial
    local_world_manager.remote_send_request_auth(&global_entity);
    local_world_manager.remote_receive_set_auth(&global_entity, EntityAuthStatus::Granted);

    assert_eq!(
        local_world_manager.get_remote_entity_auth_status(&global_entity),
        Some(EntityAuthStatus::Granted),
        "Should be able to gain authority after previous denial"
    );
}
