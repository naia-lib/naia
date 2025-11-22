use naia_shared::{BigMapKey, GlobalEntity, HostType, LocalWorldManager, RemoteEntity};
use naia_test::TestGlobalWorldManager;
/// Simplified E2E test focusing on the entity ID conversion bug
///
/// THE BUG: When server sends MigrateResponse for a client-created delegated entity,
/// it sends the wrong entity IDs - the server's local IDs instead of converting
/// them to the client's perspective.
use std::collections::HashSet;

#[test]
fn server_migrate_response_uses_wrong_entity_ids() {
    println!("\n=== BUG TEST: Server MigrateResponse Entity ID Conversion ===\n");

    // === SETUP: SERVER's view of a client-owned entity ===
    let server_gwm = TestGlobalWorldManager::new();
    let mut server_lwm = LocalWorldManager::new(&None, HostType::Server, 1, &server_gwm);

    let global_entity = GlobalEntity::from_u64(100);

    // On the SERVER, the client's entity exists as a RemoteEntity
    // because it's "remote" from the server's perspective
    let server_remote_entity = RemoteEntity::new(42);
    server_lwm.insert_remote_entity(&global_entity, server_remote_entity, HashSet::new());

    println!(
        "SERVER: Client entity GlobalEntity({}) = RemoteEntity({})",
        BigMapKey::to_u64(&global_entity),
        server_remote_entity.value()
    );

    // === DELEGATION: Server migrates entity from RemoteEntity to HostEntity ===
    // This is what happens in enable_delegation_client_owned_entity

    // Step 1: Server reserves a HostEntity for this entity
    // But wait - this will panic because GlobalEntity is already in the map!
    // This reveals the first problem: The server can't migrate properly

    println!("\nATTEMPTING: Server tries to reserve HostEntity for already-mapped entity...");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        server_lwm.host_reserve_entity(&global_entity)
    }));

    if result.is_err() {
        println!(
            "❌ BUG CAUGHT: Server cannot reserve HostEntity for GlobalEntity that already exists!"
        );
        println!("   This is because host_reserve_entity tries to INSERT the global entity,");
        println!("   but it's already in the entity_map as a RemoteEntity.");
        println!("\n   The server needs a DIFFERENT method to migrate RemoteEntity -> HostEntity");
        println!("   without trying to insert the GlobalEntity again!");

        panic!("TEST FAILED: Server cannot properly migrate client-owned entities!");
    }

    println!("✓ Server successfully reserved HostEntity");
}

#[test]
fn client_cannot_look_up_servers_remote_entity() {
    println!("\n=== BUG TEST: Client Cannot Look Up Server's RemoteEntity ===\n");

    // === SETUP: CLIENT's view ===
    let client_gwm = TestGlobalWorldManager::new();
    let mut client_lwm = LocalWorldManager::new(&None, HostType::Client, 1, &client_gwm);

    let global_entity = GlobalEntity::from_u64(100);

    // Client creates entity - gets HostEntity
    let client_host_entity = client_lwm.host_reserve_entity(&global_entity);

    println!(
        "CLIENT: Created entity - GlobalEntity({}) = HostEntity({})",
        BigMapKey::to_u64(&global_entity),
        client_host_entity.value()
    );

    // === THE BUG: Server sends MigrateResponse with Server's RemoteEntity ===
    // Server sends: MigrateResponse(GlobalEntity(100), RemoteEntity(42), HostEntity(5))
    // where RemoteEntity(42) is the SERVER's local ID for this entity

    let server_remote_entity = RemoteEntity::new(42); // Server's ID, meaningless to client!

    println!(
        "\nSERVER SENDS: MigrateResponse with RemoteEntity({})",
        server_remote_entity.value()
    );
    println!("  (This is the SERVER's local ID for the client's entity)");

    // Client tries to look up this RemoteEntity
    println!(
        "\nCLIENT: Attempting to look up RemoteEntity({})...",
        server_remote_entity.value()
    );

    let lookup_result = client_lwm
        .entity_converter()
        .remote_entity_to_global_entity(&server_remote_entity);

    match lookup_result {
        Ok(ge) => {
            panic!("UNEXPECTED: Client found RemoteEntity({}) -> GlobalEntity({}). This shouldn't work!",
                server_remote_entity.value(), BigMapKey::to_u64(&ge));
        }
        Err(_) => {
            println!(
                "❌ BUG CONFIRMED: Client cannot find RemoteEntity({}) in its entity_map!",
                server_remote_entity.value()
            );
            println!(
                "   The client only knows about HostEntity({}).",
                client_host_entity.value()
            );
            println!("   The server sent the wrong entity ID!");
            println!("\n   CORRECT BEHAVIOR:");
            println!(
                "   - Server should send the CLIENT's HostEntity({})",
                client_host_entity.value()
            );
            println!(
                "   - NOT the server's RemoteEntity({})",
                server_remote_entity.value()
            );
        }
    }
}

#[test]
fn correct_migrate_response_uses_client_host_entity() {
    println!("\n=== CORRECT BEHAVIOR: MigrateResponse Uses Client's HostEntity ===\n");

    // === SETUP: CLIENT ===
    let client_gwm = TestGlobalWorldManager::new();
    let mut client_lwm = LocalWorldManager::new(&None, HostType::Client, 1, &client_gwm);

    let global_entity = GlobalEntity::from_u64(100);
    let client_host_entity = client_lwm.host_reserve_entity(&global_entity);

    println!(
        "CLIENT: Created entity - GlobalEntity({}) = HostEntity({})",
        BigMapKey::to_u64(&global_entity),
        client_host_entity.value()
    );

    // === CORRECT: Server sends MigrateResponse with CLIENT's HostEntity ===
    let new_remote_entity = RemoteEntity::new(200); // What client will create

    println!("\nSERVER SENDS (CORRECT): MigrateResponse with:");
    println!("  - CLIENT's HostEntity({})", client_host_entity.value());
    println!("  - New RemoteEntity({})", new_remote_entity.value());

    // Client looks up its own HostEntity
    let lookup_result = client_lwm
        .entity_converter()
        .host_entity_to_global_entity(&client_host_entity);

    match lookup_result {
        Ok(ge) => {
            println!(
                "\n✓ SUCCESS: Client found HostEntity({}) -> GlobalEntity({})",
                client_host_entity.value(),
                BigMapKey::to_u64(&ge)
            );
            assert_eq!(ge, global_entity);
            println!("✓ Client can process the MigrateResponse!");
        }
        Err(_) => {
            panic!("UNEXPECTED: Client cannot find its own HostEntity!");
        }
    }
}
