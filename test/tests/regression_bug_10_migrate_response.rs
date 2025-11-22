//! Regression test for Bug #10: MigrateResponse entity ID lookup failures
//!
//! ## The Bug
//! When the server sends MigrateResponse to the client, it uses the future RemoteEntity ID
//! to tag the message. The client tries to look up this RemoteEntity to convert the message
//! to an event, but the entity is still registered as a HostEntity on the client!
//! The lookup fails and the MigrateResponse is never processed.
//!
//! ## Root Cause
//! MigrateResponse message is tagged with the FUTURE RemoteEntity ID, but the client
//! doesn't have that mapping yet. The entity is still registered as a HostEntity.
//!
//! ## Expected Behavior
//! MigrateResponse should use the GlobalEntity or HostEntity for lookup, not the future RemoteEntity.

use naia_shared::{BigMapKey, GlobalEntity, HostType, LocalWorldManager, RemoteEntity};
use naia_test::TestGlobalWorldManager;

#[test]
fn bug_10_remote_entity_lookup_fails() {
    // This test demonstrates the bug: when MigrateResponse arrives,
    // the RemoteEntity doesn't exist in the entity_map yet

    let global_world_manager = TestGlobalWorldManager::new();
    let mut lwm = LocalWorldManager::new(&None, HostType::Client, 1, &global_world_manager);
    let global_entity = GlobalEntity::from_u64(100);

    // Client creates entity as HostEntity (this is what happens when vertex is spawned)
    let host_entity = lwm.host_reserve_entity(&global_entity);

    // Server would send MigrateResponse with RemoteEntity(200) as the future entity ID
    // But that RemoteEntity doesn't exist in the client's entity_map yet!

    let entity_converter = lwm.entity_converter();

    // Entity exists as HostEntity
    let found_host = entity_converter.global_entity_to_host_entity(&global_entity);
    assert!(found_host.is_ok(), "Entity exists as HostEntity");
    assert_eq!(found_host.unwrap(), host_entity);

    // But RemoteEntity doesn't exist
    let future_remote_entity = RemoteEntity::new(200);
    let found_remote = entity_converter.remote_entity_to_global_entity(&future_remote_entity);
    assert!(
        found_remote.is_err(),
        "RemoteEntity doesn't exist yet - this is the bug!"
    );
}

#[test]
fn bug_10_server_sends_wrong_entity_ids() {
    // The server has the entity as RemoteEntity, so when it migrates,
    // it sends that RemoteEntity ID in the MigrateResponse.
    // But the CLIENT has the entity as HostEntity!

    let client_gwm = TestGlobalWorldManager::new();
    let mut client_lwm = LocalWorldManager::new(&None, HostType::Client, 1, &client_gwm);

    let server_gwm = TestGlobalWorldManager::new();
    let mut server_lwm = LocalWorldManager::new(&None, HostType::Server, 1, &server_gwm);

    let global_entity = GlobalEntity::from_u64(100);

    // Client creates entity
    let client_host_entity = client_lwm.host_reserve_entity(&global_entity);
    println!(
        "Client: Created entity as HostEntity({:?})",
        client_host_entity
    );

    // Server would have the same entity - but for testing just verify
    // that client can't look up server's entity IDs

    let client_converter = client_lwm.entity_converter();
    let some_random_remote = RemoteEntity::new(999);
    let lookup_result = client_converter.remote_entity_to_global_entity(&some_random_remote);
    assert!(
        lookup_result.is_err(),
        "Client can't find server's RemoteEntity ID!"
    );

    // The CORRECT approach: server should send client's HostEntity ID
    let should_send = client_host_entity;
    let correct_lookup = client_converter.host_entity_to_global_entity(&should_send);
    assert!(
        correct_lookup.is_ok(),
        "Client CAN find its own HostEntity!"
    );
}

#[test]
fn bug_10_correct_id_conversion_works() {
    // This test shows what SHOULD happen for correct behavior

    let client_gwm = TestGlobalWorldManager::new();
    let mut client_lwm = LocalWorldManager::new(&None, HostType::Client, 1, &client_gwm);
    let global_entity = GlobalEntity::from_u64(100);

    // Client has entity as HostEntity
    let client_host_entity = client_lwm.host_reserve_entity(&global_entity);

    // Server should send MigrateResponse with:
    // - old_entity: client's HostEntity (so client can look it up!)
    // - new_entity: new RemoteEntity for client to create

    let entity_converter = client_lwm.entity_converter();

    // Using client's HostEntity works!
    let found = entity_converter.host_entity_to_global_entity(&client_host_entity);
    assert!(found.is_ok(), "Client can find its own HostEntity");
    assert_eq!(found.unwrap(), global_entity);

    // This is how MigrateResponse should be tagged
}
