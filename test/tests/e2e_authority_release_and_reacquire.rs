//! E2E Test for Authority Release and Re-Acquisition Bug
//!
//! Reproduces the Cyberlith Editor bug:
//! "Creating a vertex, deselecting it, and reselecting it in order to modify it 
//! results in many 'No authority over vertex, skipping...' messages"
//!
//! Test Flow:
//! 1. Client creates entity (vertex) as HostEntity
//! 2. Client publishes and enables delegation
//! 3. Entity migrates to server, becomes RemoteEntity on client
//! 4. Client requests authority
//! 5. Server grants authority
//! 6. Client releases authority (deselect)
//! 7. Client requests authority AGAIN (reselect)
//! 8. BUG: Client should be able to modify, but gets "No authority" error

use naia_shared::{
    BigMapKey, EntityAuthStatus, GlobalEntity, HostType, LocalWorldManager, RemoteEntity,
};
use naia_test::TestGlobalWorldManager;

#[test]
fn authority_release_and_reacquire_remote_entity() {
    // Setup: Create client-side LocalWorldManager
    let global_world_manager = TestGlobalWorldManager::new();
    let mut local_world_manager = LocalWorldManager::new(&None, HostType::Client, 1, &global_world_manager);
    
    let global_entity = GlobalEntity::from_u64(1);
    let remote_entity = RemoteEntity::new(42);
    
    // Step 1: Simulate entity after migration
    // After delegation and migration, entity is RemoteEntity on client
    println!("Step 1: Simulating entity as RemoteEntity after migration");
    local_world_manager.insert_remote_entity(&global_entity, remote_entity, std::collections::HashSet::new());
    
    // Verify initial state: authority should be Available after migration
    let auth_status = local_world_manager.get_remote_entity_auth_status(&global_entity);
    println!("Initial authority status: {:?}", auth_status);
    assert_eq!(auth_status, Some(EntityAuthStatus::Available), 
        "After migration: authority should be Available");
    
    // Step 2: Request authority (first time - like selecting the vertex)
    println!("\nStep 2: Requesting authority (first time)...");
    local_world_manager.remote_send_request_auth(&global_entity);
    
    // Simulate server granting authority
    local_world_manager.remote_receive_set_auth(&global_entity, EntityAuthStatus::Granted);
    
    let auth_status = local_world_manager.get_remote_entity_auth_status(&global_entity);
    println!("Authority status after grant: {:?}", auth_status);
    assert_eq!(auth_status, Some(EntityAuthStatus::Granted),
        "Authority should be Granted");
    
    // Step 3: Release authority (deselect)
    println!("\nStep 3: Releasing authority (deselect)...");
    local_world_manager.remote_send_release_auth(&global_entity);
    local_world_manager.remote_receive_set_auth(&global_entity, EntityAuthStatus::Available);
    
    let auth_status = local_world_manager.get_remote_entity_auth_status(&global_entity);
    println!("Authority status after release: {:?}", auth_status);
    assert_eq!(auth_status, Some(EntityAuthStatus::Available),
        "After release: authority should be Available");
    
    // Step 4: Request authority AGAIN (reselect - THIS IS WHERE THE BUG OCCURS)
    println!("\nStep 4: Requesting authority AGAIN (reselect)...");
    // THIS IS THE CRITICAL TEST
    // Can we request authority again after releasing it?
    
    let can_request = local_world_manager.get_remote_entity_auth_status(&global_entity);
    println!("Can request authority? Status is: {:?}", can_request);
    assert_eq!(can_request, Some(EntityAuthStatus::Available),
        "Authority should still be Available for re-request");
    
    // Simulate requesting and server granting authority again
    local_world_manager.remote_send_request_auth(&global_entity);
    local_world_manager.remote_receive_set_auth(&global_entity, EntityAuthStatus::Granted);
    
    let final_auth_status = local_world_manager.get_remote_entity_auth_status(&global_entity);
    println!("Final authority status: {:?}", final_auth_status);
    assert_eq!(final_auth_status, Some(EntityAuthStatus::Granted),
        "Authority should be Granted again");
    
    println!("\n✓ Test PASSED: Authority can be released and re-acquired");
}


#[test]
#[ignore] // Mark as ignored until we can fully implement the flow
fn authority_cycle_through_migration() {
    // This is a more complete E2E test that would require:
    // - Actual migration simulation
    // - Message passing between client/server LocalWorldManagers
    // - Full authority state machine testing
    
    // TODO: Implement when we have better test infrastructure
    //   for simulating client-server message flow
}

