//! TRUE End-to-End Test with Real Client and Server
//!
//! This test uses REAL naia::Client<> and naia::Server<> instances with an
//! in-memory socket implementation, following the pattern from demos/basic

use log::info;
use naia_client::{Client as NaiaClient, ClientConfig, ConnectEvent as ClientConnectEvent};
use naia_server::{
    AuthEvent, ConnectEvent as ServerConnectEvent, DelegateEntityEvent, EntityAuthGrantEvent,
    Server as NaiaServer, ServerConfig,
};
use naia_shared::{Instant, WorldRefType};
use naia_test::{local_socket_pair, protocol, Auth, Position, TestEntity, TestWorld};

type Client = NaiaClient<TestEntity>;
type Server = NaiaServer<TestEntity>;

/// Initialize logger for tests
fn init_logger() {
    let _ = env_logger::builder()
        .is_test(true)
        .filter_level(log::LevelFilter::Trace)
        .try_init();
}

/// Helper to run client and server update loops
fn update_client_server(
    client: &mut Client,
    server: &mut Server,
    client_world: &mut TestWorld,
    server_world: &mut TestWorld,
) {
    let now = Instant::now();
    
    // Client update
    client.receive_all_packets();
    client.process_all_packets(client_world.proxy_mut(), &now);
    client.send_all_packets(client_world.proxy_mut());
    
    // Server update
    server.receive_all_packets();
    server.process_all_packets(server_world.proxy_mut(), &now);
    server.send_all_packets(server_world.proxy());
}

/// Helper to wait for a condition with timeout
fn wait_for_condition<F>(
    client: &mut Client,
    server: &mut Server,
    client_world: &mut TestWorld,
    server_world: &mut TestWorld,
    max_attempts: usize,
    mut condition: F,
    debug_msg: &str,
) -> bool
where
    F: FnMut(&Client, &Server) -> bool,
{
    for attempt in 0..max_attempts {
        update_client_server(client, server, client_world, server_world);
        
        if condition(client, server) {
            return true;
        }
        
        if attempt % 10 == 0 && attempt > 0 {
            info!("{} - Attempt {}", debug_msg, attempt);
        }
    }
    false
}

/// Test setup: creates client, server, and worlds with default config
fn setup_test() -> (Client, Server, TestWorld, TestWorld, naia_server::RoomKey) {
    let protocol = protocol();
    let (client_socket, server_socket) = local_socket_pair();

    let mut server = Server::new(ServerConfig::default(), protocol.clone());
    
    let mut client_config = ClientConfig::default();
    client_config.send_handshake_interval = std::time::Duration::from_millis(0);
    let mut client = Client::new(client_config, protocol);
    let client_world = TestWorld::default();
    let server_world = TestWorld::default();

    server.listen(server_socket);
    let main_room_key = server.make_room().key();

    let auth = Auth::new("test_user", "test_password");
    client.auth(auth);
    client.connect(client_socket);

    (client, server, client_world, server_world, main_room_key)
}

/// Helper to run N update cycles
fn run_updates(
    client: &mut Client,
    server: &mut Server,
    client_world: &mut TestWorld,
    server_world: &mut TestWorld,
    count: usize,
) {
    for _ in 0..count {
        update_client_server(client, server, client_world, server_world);
    }
}

/// Helper to wait for delegation event
fn wait_for_delegation_event(
    client: &mut Client,
    server: &mut Server,
    client_world: &mut TestWorld,
    server_world: &mut TestWorld,
    max_attempts: usize,
) -> bool {
    for attempt in 0..max_attempts {
        update_client_server(client, server, client_world, server_world);
        
        let mut server_events = server.take_world_events();
        let mut found = false;
        for (_, _entity) in server_events.read::<DelegateEntityEvent>() {
            found = true;
            break;
        }
        
        if found {
            info!("Server received delegation event at attempt {}", attempt);
            return true;
        }
        
        if attempt % 10 == 0 && attempt > 0 {
            info!("Waiting for delegation event - Attempt {}", attempt);
        }
    }
    false
}

/// Helper to wait for a specific authority status
fn wait_for_authority_status(
    client: &mut Client,
    server: &mut Server,
    client_world: &mut TestWorld,
    server_world: &mut TestWorld,
    entity: &TestEntity,
    expected_status: naia_shared::EntityAuthStatus,
    max_attempts: usize,
    debug_msg: &str,
) -> bool {
    wait_for_condition(
        client,
        server,
        client_world,
        server_world,
        max_attempts,
        |client, _| {
            client.entity_authority_status(entity) == Some(expected_status)
        },
        debug_msg,
    )
}

/// Helper to complete handshake
fn complete_handshake(
    client: &mut Client,
    server: &mut Server,
    client_world: &mut TestWorld,
    server_world: &mut TestWorld,
    main_room_key: &naia_server::RoomKey,
) -> Option<naia_server::UserKey> {
    let mut user_key_opt = None;
    let mut connected = false;
    
    for attempt in 1..=100 {
        if !client.connection_status().is_connected() {
            client.receive_all_packets();
            client.send_all_packets(client_world.proxy_mut());
        } else {
            let now = Instant::now();
            client.receive_all_packets();
            client.process_all_packets(client_world.proxy_mut(), &now);
            
            let mut client_events = client.take_world_events();
            for _ in client_events.read::<ClientConnectEvent>() {
                info!("Client connected in {} attempts", attempt);
                connected = true;
                break;
            }
        }

        let now = Instant::now();
        server.receive_all_packets();
        server.process_all_packets(server_world.proxy_mut(), &now);

        let mut server_events = server.take_world_events();
        
        for (user_key, _auth) in server_events.read::<AuthEvent<Auth>>() {
            info!("Server accepting connection for user: {:?}", user_key);
            server.accept_connection(&user_key);
            server.room_mut(main_room_key).add_user(&user_key);
            user_key_opt = Some(user_key);
        }

        server.send_all_packets(server_world.proxy());
        
        if connected { break; }
    }
    
    if connected {
        user_key_opt
    } else {
        None
    }
}

#[test]
fn e2e_client_server_handshake() {
    init_logger();
    info!("=== E2E TEST: Client-Server Handshake ===");

    let (mut client, mut server, mut client_world, mut server_world, main_room_key) = setup_test();

    let user_key = complete_handshake(
        &mut client,
        &mut server,
        &mut client_world,
        &mut server_world,
        &main_room_key,
    );

    assert!(user_key.is_some(), "Client should have connected");
    info!("✓ Client and Server successfully connected via handshake");
}

#[test]
fn e2e_authority_release_and_reacquire() {
    init_logger();
    info!("=== E2E TEST: Authority Release and Re-Acquire (Bug #7) ===");
    
    let (mut client, mut server, mut client_world, mut server_world, main_room_key) = setup_test();

    // Step 1: Complete handshake
    info!("Step 1: Completing handshake...");
    let _user_key = complete_handshake(
        &mut client,
        &mut server,
        &mut client_world,
        &mut server_world,
        &main_room_key,
    ).expect("Failed to establish connection");
    
    info!("✓ Handshake complete");
    
    // Step 2: Client creates entity, inserts component, and publishes
    info!("\nStep 2: Client creating and publishing entity...");
    let client_entity = client
        .spawn_entity(client_world.proxy_mut())
        .insert_component(Position::new(10.0, 20.0))
        .configure_replication(naia_client::ReplicationConfig::Public)
        .id();
    
    info!("Client created entity");
    
    // Wait for entity to sync to server
    run_updates(&mut client, &mut server, &mut client_world, &mut server_world, 10);
    
    assert!(server_world.proxy().entities().len() > 0, "Server should have received entity");
    info!("✓ Entity published to server");
    
    // Step 3: Client enables delegation
    info!("\nStep 3: Client enabling delegation...");
    client
        .entity_mut(client_world.proxy_mut(), &client_entity)
        .configure_replication(naia_client::ReplicationConfig::Delegated);
    
    // Wait for delegation event
    let delegation_complete = wait_for_delegation_event(
        &mut client,
        &mut server,
        &mut client_world,
        &mut server_world,
        50,
    );
    
    assert!(delegation_complete, "Delegation should complete");
    
    // Run more loops to ensure migration completes
    run_updates(&mut client, &mut server, &mut client_world, &mut server_world, 10);
    
    info!("✓ Delegation and migration complete");
    
    // Wait for authority to become Available OR Granted after migration  
    info!("\nWaiting for authority state to settle after migration...");
    let authority_settled = wait_for_condition(
        &mut client,
        &mut server,
        &mut client_world,
        &mut server_world,
        50,
        |client, _| {
            if let Some(auth_status) = client.entity_authority_status(&client_entity) {
                auth_status == naia_shared::EntityAuthStatus::Granted ||
                auth_status == naia_shared::EntityAuthStatus::Available
            } else {
                false
            }
        },
        "Waiting for authority to settle",
    );
    
    assert!(authority_settled, "Authority should settle after migration");
    
    let initial_auth_status = client.entity_authority_status(&client_entity).unwrap();
    info!("✓ Authority status after migration: {:?}", initial_auth_status);
    
    // Ensure we have authority before releasing (either auto-granted or manually request)
    if initial_auth_status != naia_shared::EntityAuthStatus::Granted {
        info!("\nRequesting authority (it wasn't auto-granted)...");
        client
            .entity_mut(client_world.proxy_mut(), &client_entity)
            .request_authority();
        
        let authority_granted = wait_for_authority_status(
            &mut client,
            &mut server,
            &mut client_world,
            &mut server_world,
            &client_entity,
            naia_shared::EntityAuthStatus::Granted,
            50,
            "Waiting for authority grant",
        );
        
        assert!(authority_granted, "Client should have authority");
        info!("✓ Authority granted");
    }
    
    // Step 4: Client releases authority (deselect)
    info!("\nStep 4: Client releasing authority (deselect)...");
    client
        .entity_mut(client_world.proxy_mut(), &client_entity)
        .release_authority();
    
    // Process release
    run_updates(&mut client, &mut server, &mut client_world, &mut server_world, 10);
    
    // Verify authority was released
    let auth_status = client.entity_authority_status(&client_entity);
    info!("Authority status after release: {:?}", auth_status);
    assert_ne!(
        auth_status,
        Some(naia_shared::EntityAuthStatus::Granted),
        "Authority should not be Granted after release"
    );
    info!("✓ Authority released");
    
    // Step 5: Client requests authority AGAIN (reselect) ← THIS IS WHERE BUG #7 APPEARED
    info!("\nStep 5: Client requesting authority AGAIN (reselect - Bug #7 test)...");
    client
        .entity_mut(client_world.proxy_mut(), &client_entity)
        .request_authority();
    
    let authority_regranted = wait_for_authority_status(
        &mut client,
        &mut server,
        &mut client_world,
        &mut server_world,
        &client_entity,
        naia_shared::EntityAuthStatus::Granted,
        50,
        "Waiting for authority re-grant",
    );
    
    assert!(
        authority_regranted, 
        "BUG #7: Client should be able to regain authority after releasing it!"
    );
    info!("\n✓✓✓ SUCCESS: Bug #7 is fixed! Client can regain authority after release ✓✓✓");
}
