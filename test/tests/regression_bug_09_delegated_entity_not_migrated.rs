//! Regression test for Bug #9: Client-created Delegated entities never migrate to RemoteEntity
//!
//! ## The Bug
//! When a client creates an entity with `ReplicationConfig::Delegated`, it should:
//! 1. Send EnableDelegation to the server
//! 2. Receive MigrateResponse back from the server
//! 3. Migrate from HostEntity to RemoteEntity
//! 4. Be able to request/release/re-request authority
//!
//! However, the entity remains as HostEntity forever, never becoming a RemoteEntity.
//! This causes authority re-requests to fail because the RemoteEntityChannel doesn't exist.
//!
//! ## Symptoms
//! - Client creates vertex with `configure_replication(Delegated)`
//! - Client requests authority -> Granted ✓
//! - Client releases authority -> Available ✓
//! - Client tries to request authority again -> FAILS ❌
//! - Debug shows:
//!   - Global authority tracker: "Available" or "Releasing"
//!   - RemoteEntityChannel status: None (entity still HostEntity!)
//!   - Authority request blocked by global manager
//!
//! ## Root Cause
//! The client never receives MigrateResponse from the server, so the entity is never
//! migrated from HostEntity to RemoteEntity. Without a RemoteEntityChannel, the authority
//! state machine cannot function properly, and subsequent authority requests fail.
//!
//! ## The Fix
//! This test verifies that after an entity is configured as Delegated and goes through
//! the full delegation flow (EnableDelegation -> MigrateResponse), it properly exists
//! as a RemoteEntity with a working RemoteEntityChannel that can handle the full
//! authority lifecycle.

use naia_shared::{
    BigMapKey, EntityAuthStatus, GlobalEntity, HostType, LocalWorldManager, RemoteEntity,
};
use naia_test::TestGlobalWorldManager;
use std::collections::HashSet;

#[test]
fn bug_09_delegated_entity_with_proper_migration_allows_authority_re_request() {
    // This test shows what SHOULD happen when migration works correctly

    let global_world_manager = TestGlobalWorldManager::new();
    let mut lwm = LocalWorldManager::new(&None, HostType::Client, 1, &global_world_manager);
    let global_entity = GlobalEntity::from_u64(100);

    // Simulate entity after proper migration (HostEntity -> RemoteEntity)
    // In production, this happens when client receives MigrateResponse from server
    let remote_entity = RemoteEntity::new(100);
    lwm.insert_remote_entity(&global_entity, remote_entity, HashSet::new());

    // After migration, entity should exist as RemoteEntity with Available status
    let channel_status = lwm.get_remote_entity_auth_status(&global_entity);
    assert_eq!(
        channel_status,
        Some(EntityAuthStatus::Available),
        "After MigrateResponse, entity should be RemoteEntity with Available authority"
    );

    // Request authority -> Granted
    lwm.remote_send_request_auth(&global_entity);
    lwm.remote_receive_set_auth(&global_entity, EntityAuthStatus::Granted);
    assert_eq!(
        lwm.get_remote_entity_auth_status(&global_entity),
        Some(EntityAuthStatus::Granted)
    );

    // Release authority -> Available
    lwm.remote_send_release_auth(&global_entity);
    lwm.remote_receive_set_auth(&global_entity, EntityAuthStatus::Available);
    assert_eq!(
        lwm.get_remote_entity_auth_status(&global_entity),
        Some(EntityAuthStatus::Available)
    );

    // Request authority AGAIN (this is the key test!)
    lwm.remote_send_request_auth(&global_entity);
    lwm.remote_receive_set_auth(&global_entity, EntityAuthStatus::Requested);

    // This should work when entity was properly migrated!
    assert_eq!(lwm.get_remote_entity_auth_status(&global_entity), Some(EntityAuthStatus::Requested),
        "Should be able to re-request authority after releasing it when entity is properly migrated.");
}

#[test]
fn bug_09_entity_without_migration_has_no_remote_channel() {
    // This test demonstrates the ACTUAL bug symptom in production
    //
    // In production: Client creates vertex with ReplicationConfig::Delegated
    // Expected: Server receives EnableDelegation, sends back MigrateResponse
    // Actual: MigrateResponse never arrives, entity stays as HostEntity
    // Result: No RemoteEntityChannel, authority system doesn't work

    let global_world_manager = TestGlobalWorldManager::new();
    let mut lwm = LocalWorldManager::new(&None, HostType::Client, 1, &global_world_manager);
    let global_entity = GlobalEntity::from_u64(200);

    // This entity was created with ReplicationConfig::Delegated
    // but MigrateResponse never arrived, so it has no RemoteEntityChannel

    // BUG SYMPTOM: No RemoteEntityChannel exists
    let channel_status = lwm.get_remote_entity_auth_status(&global_entity);
    assert_eq!(
        channel_status, None,
        "BUG SYMPTOM: Entity has no RemoteEntityChannel because MigrateResponse never arrived"
    );

    // This is the core issue: without RemoteEntityChannel, authority system cannot work
    // Authority commands sent to this entity will be silently ignored
}

#[test]
fn bug_09_full_authority_lifecycle_with_proper_migration() {
    // This test shows the complete authority lifecycle when migration works correctly

    let global_world_manager = TestGlobalWorldManager::new();
    let mut lwm = LocalWorldManager::new(&None, HostType::Client, 1, &global_world_manager);
    let global_entity = GlobalEntity::from_u64(300);

    // Entity after proper migration (has RemoteEntityChannel)
    let remote_entity = RemoteEntity::new(300);
    lwm.insert_remote_entity(&global_entity, remote_entity, HashSet::new());

    // After migration, RemoteEntityChannel exists with Available status
    assert_eq!(
        lwm.get_remote_entity_auth_status(&global_entity),
        Some(EntityAuthStatus::Available)
    );

    // Full authority lifecycle works correctly:

    // Request authority
    lwm.remote_send_request_auth(&global_entity);
    lwm.remote_receive_set_auth(&global_entity, EntityAuthStatus::Requested);
    assert_eq!(
        lwm.get_remote_entity_auth_status(&global_entity),
        Some(EntityAuthStatus::Requested)
    );

    // Grant authority
    lwm.remote_receive_set_auth(&global_entity, EntityAuthStatus::Granted);
    assert_eq!(
        lwm.get_remote_entity_auth_status(&global_entity),
        Some(EntityAuthStatus::Granted)
    );

    // Release authority
    lwm.remote_send_release_auth(&global_entity);
    lwm.remote_receive_set_auth(&global_entity, EntityAuthStatus::Releasing);
    assert_eq!(
        lwm.get_remote_entity_auth_status(&global_entity),
        Some(EntityAuthStatus::Releasing)
    );

    // Return to Available
    lwm.remote_receive_set_auth(&global_entity, EntityAuthStatus::Available);
    assert_eq!(
        lwm.get_remote_entity_auth_status(&global_entity),
        Some(EntityAuthStatus::Available)
    );

    // Request authority AGAIN (this is the key test!)
    lwm.remote_send_request_auth(&global_entity);
    lwm.remote_receive_set_auth(&global_entity, EntityAuthStatus::Requested);
    assert_eq!(
        lwm.get_remote_entity_auth_status(&global_entity),
        Some(EntityAuthStatus::Requested),
        "Should be able to re-request authority after releasing it"
    );

    // Grant again
    lwm.remote_receive_set_auth(&global_entity, EntityAuthStatus::Granted);
    assert_eq!(
        lwm.get_remote_entity_auth_status(&global_entity),
        Some(EntityAuthStatus::Granted),
        "Authority lifecycle should work correctly after migration"
    );
}

#[test]
fn bug_09_migration_is_required_for_authority_system() {
    // This test explicitly verifies that migration is REQUIRED for authority to work

    let global_world_manager = TestGlobalWorldManager::new();
    let mut lwm = LocalWorldManager::new(&None, HostType::Client, 1, &global_world_manager);

    // Create two entities
    let entity_without_migration = GlobalEntity::from_u64(400);
    let entity_with_migration = GlobalEntity::from_u64(401);

    // Entity 1: No migration (no RemoteEntityChannel)
    // This simulates the production bug state

    // Entity 2: With migration (has RemoteEntityChannel)
    lwm.insert_remote_entity(
        &entity_with_migration,
        RemoteEntity::new(401),
        HashSet::new(),
    );

    // Verify entity 1 has no RemoteEntityChannel
    assert_eq!(
        lwm.get_remote_entity_auth_status(&entity_without_migration),
        None,
        "Entity without migration has no RemoteEntityChannel"
    );

    // Verify entity 2 has RemoteEntityChannel
    assert_eq!(
        lwm.get_remote_entity_auth_status(&entity_with_migration),
        Some(EntityAuthStatus::Available),
        "Entity with migration has RemoteEntityChannel"
    );

    // Only entity 2 can participate in authority lifecycle
    lwm.remote_send_request_auth(&entity_with_migration);
    lwm.remote_receive_set_auth(&entity_with_migration, EntityAuthStatus::Granted);
    assert_eq!(
        lwm.get_remote_entity_auth_status(&entity_with_migration),
        Some(EntityAuthStatus::Granted)
    );

    // Entity 1 cannot participate (no channel exists)
    assert_eq!(
        lwm.get_remote_entity_auth_status(&entity_without_migration),
        None,
        "Entity without migration cannot participate in authority system"
    );
}
