//! End-to-End Test: Two Clients Delegation
//!
//! This test reproduces a bug where Client A delegates an entity, Client B requests
//! authority and modifies it, but Client A doesn't receive the update.
//!
//! Test Flow:
//! 1. Setup Server and Client A
//! 2. Client A creates & delegates entity
//! 3. Setup Client B (connects to same server)
//! 4. Client B requests authority & modifies entity
//! 5. Verify Client A receives update

use log::info;

use naia_client::{
    Client as NaiaClient, EntityAuthGrantedEvent,
};
use naia_server::{
    DelegateEntityEvent,
    Server as NaiaServer, ServerConfig,
};
use naia_shared::WorldRefType;
use naia_test::{protocol, Auth, Position, TestEntity, TestWorld, create_client_socket, create_server_socket, default_client_config, update_all, complete_handshake_with_name, LocalTransportBuilder};

type Client = NaiaClient<TestEntity>;
type Server = NaiaServer<TestEntity>;

/// Initialize logger for tests
fn init_logger() {
    let _ = env_logger::builder()
        .is_test(true)
        .filter_level(log::LevelFilter::Trace)
        .try_init();
}

#[test]
fn e2e_two_clients_delegation_sync() {
    init_logger();
    info!("=== E2E TEST: Two Clients Delegation Sync ===");

    // Step 1: Setup Server and Client A
    info!("\nStep 1: Setting up Server and Client A...");

    let protocol = protocol();
    let builder = LocalTransportBuilder::new();

    let mut server = Server::new(ServerConfig::default(), protocol.clone());
    let server_socket = create_server_socket(&builder);
    server.listen(server_socket);
    let main_room_key = server.make_room().key();

    let mut client_a = Client::new(default_client_config(), protocol.clone());
    let mut client_a_world = TestWorld::default();
    let mut server_world = TestWorld::default();

    let client_a_socket = create_client_socket(&builder);
    let auth_a = Auth::new("client_a", "password");
    client_a.auth(auth_a);
    client_a.connect(client_a_socket);

    let _user_key_a = complete_handshake_with_name(
        &mut client_a,
        &mut server,
        &mut client_a_world,
        &mut server_world,
        &main_room_key,
        "Client A",
    )
    .expect("Client A should connect");

    info!("✓ Server and Client A connected");

    // Step 2: Client A creates & delegates entity
    info!("\nStep 2: Client A creating and delegating entity...");

    let client_a_entity = client_a
        .spawn_entity(client_a_world.proxy_mut())
        .insert_component(Position::new(10.0, 20.0))
        .id();

    info!("Client A created entity");

    // Wait for entity to be replicated to server
    for _ in 0..10 {
        update_client_server(
            &mut client_a,
            &mut server,
            &mut client_a_world,
            &mut server_world,
        );
    }

    // Add entity to room (entities must be explicitly added to rooms)
    let server_entity = server_world.proxy().entities()[0];
    server
        .entity_mut(server_world.proxy_mut(), &server_entity)
        .enter_room(&main_room_key);
    info!("Added entity to room");

    // Configure to Delegated
    client_a
        .entity_mut(client_a_world.proxy_mut(), &client_a_entity)
        .configure_replication(naia_client::ReplicationConfig::Delegated);

    info!("Client A configured entity to Delegated");

    // Run updates to allow delegation to complete
    info!("Running updates to complete delegation...");
    use naia_test::{update_client_server, wait_for_authority_status};
    
    let mut delegation_complete = false;
    for i in 0..50 {
        update_client_server(
            &mut client_a,
            &mut server,
            &mut client_a_world,
            &mut server_world,
        );

        let mut server_events = server.take_world_events();
        for _ in server_events.read::<DelegateEntityEvent>() {
            info!("Server received delegation event at update {}", i);
            delegation_complete = true;
        }

        // Check if entity is now Delegated on client
        let config = client_a.entity_replication_config(&client_a_entity);
        if config == Some(naia_client::ReplicationConfig::Delegated) {
            info!("✓ Entity is Delegated on Client A at update {}", i);
            break;
        }

        if i % 10 == 0 {
            info!("  [Update {}] Config: {:?}", i, config);
        }
    }

    assert!(
        delegation_complete,
        "Delegation should complete"
    );

    // Wait for authority to be granted to Client A
    info!("Waiting for authority to be granted to Client A...");
    wait_for_authority_status(
        &mut client_a,
        &mut server,
        &mut client_a_world,
        &mut server_world,
        &client_a_entity,
        naia_shared::EntityAuthStatus::Granted,
        30,
        "Waiting for Client A authority",
    );

    info!("✓ Client A has authority");

    // Client A releases authority (key step for bug reproduction!)
    info!("Client A releasing authority...");
    client_a
        .entity_mut(client_a_world.proxy_mut(), &client_a_entity)
        .release_authority();

    // Wait for authority to be released
    wait_for_authority_status(
        &mut client_a,
        &mut server,
        &mut client_a_world,
        &mut server_world,
        &client_a_entity,
        naia_shared::EntityAuthStatus::Available,
        30,
        "Waiting for Client A to release authority",
    );

    info!("✓ Client A released authority - entity is now Available");

    // Clear any pending server events before Client B connects
    for _ in 0..5 {
        update_client_server(
            &mut client_a,
            &mut server,
            &mut client_a_world,
            &mut server_world,
        );
    }
    info!("Cleared pending events");

    // Step 3: Setup Client B
    info!("\nStep 3: Setting up Client B...");

    let mut client_b = Client::new(default_client_config(), protocol.clone());
    let mut client_b_world = TestWorld::default();

    let client_b_socket = create_client_socket(&builder);
    let auth_b = Auth::new("client_b", "password");
    client_b.auth(auth_b);
    client_b.connect(client_b_socket);

    let user_key_b = complete_handshake_with_name(
        &mut client_b,
        &mut server,
        &mut client_b_world,
        &mut server_world,
        &main_room_key,
        "Client B",
    )
    .expect("Client B should connect");

    info!("✓ Client B connected");

    // Verify Client B is in the room (should have been added during handshake)
    let room_user_count = server.room(&main_room_key).users_count();
    info!("Room has {} users (should be 2: Client A and Client B)", room_user_count);
    
    // Ensure Client B is in the room (in case handshake didn't add them)
    if room_user_count < 2 {
        server.room_mut(&main_room_key).add_user(&user_key_b);
        info!("Explicitly added Client B to room");
    }
    
    // Add entity to Client B's scope (entities in room should be in scope for all users)
    // This simulates what should happen automatically when a user joins a room with entities
    let server_entity = server_world.proxy().entities()[0];
    
    // Debug: Check entity owner and replication config
    let owner = server.entity_owner(&server_entity);
    let config = server.entity_replication_config(&server_entity);
    info!("Entity owner: {:?}, config: {:?}", owner, config);
    
    // Add entity to scope for Client B specifically
    server.user_scope_mut(&user_key_b).include(&server_entity);
    info!("Added entity to Client B's scope");

    // Run updates to allow update_entity_scopes to initialize the entity in Client B's connection
    // update_entity_scopes is called in send_all_packets, which is called in update_all
    info!("Running update cycles to allow entity replication...");
    for i in 0..10 {
        update_all(
            &mut client_a,
            &mut client_b,
            &mut server,
            &mut client_a_world,
            &mut client_b_world,
            &mut server_world,
        );
        
        // Check for server spawn events after each update
        let mut server_events = server.take_world_events();
        let spawn_count = server_events.read::<naia_server::SpawnEntityEvent>().count();
        if spawn_count > 0 {
            info!("  [Update {}] Server generated {} spawn event(s) - these were READ", i, spawn_count);
        }
    }
    info!("Ran 10 update cycles to allow entity replication");

    // Wait for Client B to receive the delegated entity
    info!("Waiting for Client B to receive the delegated entity...");
    
    // Verify entity is still on server and in the room
    let server_entities = server_world.proxy().entities();
    info!("Server has {} entities", server_entities.len());
    
    // Check if entity is in the room
    let room = server.room(&main_room_key);
    let room_entities = room.entities();
    info!("Room has {} entities", room_entities.len());
    
    let mut client_b_entity: Option<TestEntity> = None;
    for i in 0..100 {
        update_all(
            &mut client_a,
            &mut client_b,
            &mut server,
            &mut client_a_world,
            &mut client_b_world,
            &mut server_world,
        );

        // Check if entity exists on Client B
        let entities = client_b_world.proxy().entities();
        if !entities.is_empty() {
            client_b_entity = Some(entities[0]);
            info!("✓ Client B received entity at update {}", i);
            break;
        }

        if i % 10 == 0 {
            info!("  [Update {}] Client B has {} entities, waiting...", i, entities.len());
            
            // Debug: Check what events Client B is receiving
            let mut client_b_events = client_b.take_world_events();
            let spawn_count = client_b_events.read::<naia_client::SpawnEntityEvent>().count();
            if spawn_count > 0 {
                info!("    Client B received {} spawn events", spawn_count);
            }
        }
    }

    let client_b_entity = client_b_entity.expect("Client B should receive the entity");

    // Wait for entity to migrate from HostEntity to RemoteEntity
    // The entity should receive a MigrateResponse message to complete the migration
    info!("Waiting for entity to migrate to RemoteEntity on Client B...");
    let mut migrated = false;
    for i in 0..50 {
        update_all(
            &mut client_a,
            &mut client_b,
            &mut server,
            &mut client_a_world,
            &mut client_b_world,
            &mut server_world,
        );
        
        // Check if entity is now a RemoteEntity by checking if RemoteEntityChannel exists
        // We can't directly check this, but we can verify by checking if the entity
        // has been migrated by looking for the migration in logs or checking auth status
        // For now, just wait a few cycles - the migration should happen automatically
        if i >= 5 {
            // After a few cycles, assume migration should have happened
            migrated = true;
            break;
        }
        
        if i % 5 == 0 {
            info!("  [Update {}] Waiting for entity migration...", i);
        }
    }
    
    if !migrated {
        info!("⚠ Entity migration may not have completed, continuing anyway...");
    }

    // Verify entity is Delegated on Client B
    let config_b = client_b.entity_replication_config(&client_b_entity);
    assert_eq!(
        config_b,
        Some(naia_client::ReplicationConfig::Delegated),
        "Entity should be Delegated on Client B"
    );

    // Verify authority is Available (not Granted, since Client A released it)
    let auth_status_b = client_b.entity_authority_status(&client_b_entity);
    assert_eq!(
        auth_status_b,
        Some(naia_shared::EntityAuthStatus::Available),
        "Entity should have Available authority on Client B"
    );

    info!("✓ Client B received delegated entity with Available authority");

    // CONTRACT TEST: After EnableDelegation handshake completes, the entity should be ready for SetAuthority
    // This means the RemoteEntityChannel's AuthChannel should have auth_status=Available
    // NOTE: We can't directly access the channel, but we can verify the behavior through the entity_update_authority flow
    // The contract is: when SetAuthority arrives, get_remote_entity_auth_status should return Some(Available), not None
    info!("CONTRACT: RemoteEntityChannel should have auth_status=Available after EnableDelegation handshake");

    // Step 4: Client B requests authority & modifies
    info!("\nStep 4: Client B requesting authority and modifying entity...");

    // Client B requests authority
    client_b
        .entity_mut(client_b_world.proxy_mut(), &client_b_entity)
        .request_authority();

    info!("Client B requested authority");

    // Wait for authority to be granted to Client B
    let mut authority_granted = false;
    for i in 0..50 {
        update_all(
            &mut client_a,
            &mut client_b,
            &mut server,
            &mut client_a_world,
            &mut client_b_world,
            &mut server_world,
        );

        let mut client_b_events = client_b.take_world_events();
        for _ in client_b_events.read::<EntityAuthGrantedEvent>() {
            info!("✓ Client B received authority grant event at update {}", i);
            authority_granted = true;
        }

        let auth_status = client_b.entity_authority_status(&client_b_entity);
        if auth_status == Some(naia_shared::EntityAuthStatus::Granted) {
            if !authority_granted {
                info!("✓ Client B has authority at update {}", i);
            }
            authority_granted = true;
            break;
        }

        if i % 10 == 0 {
            info!("  [Update {}] Client B auth status: {:?}", i, auth_status);
        }
    }

    assert!(
        authority_granted,
        "Client B should receive authority"
    );

    // Verify Client A lost authority
    let auth_status_a = client_a.entity_authority_status(&client_a_entity);
    assert_ne!(
        auth_status_a,
        Some(naia_shared::EntityAuthStatus::Granted),
        "Client A should have lost authority"
    );
    info!("✓ Client A lost authority (status: {:?})", auth_status_a);

    // Client B modifies the entity
    // BUG REPRODUCTION: The entity on Client B is still a HostEntity and hasn't migrated to RemoteEntity
    // This causes the component modification to fail because GlobalDiffHandler hasn't registered the component
    info!("Client B modifying entity...");
    let new_x = 100.0;
    let new_y = 200.0;
    
    // Mutate the existing component directly
    // For delegated entities, we can mutate through the component wrapper
    let mut entity_mut = client_b.entity_mut(client_b_world.proxy_mut(), &client_b_entity);
    let mut pos_wrapper = entity_mut.component::<Position>()
        .expect("Position component should exist on entity received by Client B");
    let pos = &mut *pos_wrapper;
    *pos.x = new_x;
    *pos.y = new_y;
    drop(pos_wrapper);
    drop(entity_mut);
    
    info!("Client B modified entity position to ({}, {})", new_x, new_y);

    // Step 5: Verify Client A receives update
    info!("\nStep 5: Verifying Client A receives update...");

    // Run updates to propagate the change
    let mut update_received = false;
    let mut original_position: Option<Position> = None;
    for i in 0..50 {
        update_all(
            &mut client_a,
            &mut client_b,
            &mut server,
            &mut client_a_world,
            &mut client_b_world,
            &mut server_world,
        );

        // Check if Client A received the update
        if let Some(pos_wrapper) = client_a_world.proxy().component::<Position>(&client_a_entity) {
            let pos = &*pos_wrapper;
            if original_position.is_none() {
                // Clone the position for comparison
                original_position = Some(Position::new(*pos.x, *pos.y));
                info!("  Client A initial position: ({}, {})", *pos.x, *pos.y);
            } else if *pos.x == new_x && *pos.y == new_y {
                info!("✓ Client A received update at update {}: ({}, {})", i, *pos.x, *pos.y);
                update_received = true;
                break;
            }
        }

        if i % 10 == 0 {
            if let Some(pos_wrapper) = client_a_world.proxy().component::<Position>(&client_a_entity) {
                let pos = &*pos_wrapper;
                info!("  [Update {}] Client A position: ({}, {})", i, *pos.x, *pos.y);
            } else {
                info!("  [Update {}] Client A has no position component yet", i);
            }
        }
    }

    assert!(
        update_received,
        "BUG REPRODUCTION: Client A should receive the update from Client B's modification!"
    );

    info!("\n✓✓✓ SUCCESS: Two-client delegation sync test passed! ✓✓✓");
}
