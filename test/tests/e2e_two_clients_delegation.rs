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

use naia_server::DelegateEntityEvent;
use naia_test::{protocol, Auth, Position, Scenario};

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

    // Step 1: Setup Server and Client A

    let protocol = protocol();
    let mut scenario = Scenario::new(protocol);

    let _user_key_a = scenario.connect_client_a("Client A", Auth::new("client_a", "password"));

    // Step 2: Client A creates & delegates entity

    let (client_a, world_a) = scenario.client_a();
    let client_a_entity = client_a
        .spawn_entity(world_a.proxy_mut())
        .insert_component(Position::new(10.0, 20.0))
        .id();

    // Wait for entity to be replicated to server
    scenario.tick_until(30, "server has entity", |scenario| {
        scenario.server_has_entity()
    });

    // Add entity to room (entities must be explicitly added to rooms)
    let main_room_key = scenario.main_room_key().clone();
    let server_entity = {
        let entity = scenario.server_first_entity();
        let (server, server_world) = scenario.server();
        server
            .entity_mut(server_world.proxy_mut(), &entity)
            .enter_room(&main_room_key);
        entity
    };

    // Configure to Delegated
    let (client_a, world_a) = scenario.client_a();
    client_a
        .entity_mut(world_a.proxy_mut(), &client_a_entity)
        .configure_replication(naia_client::ReplicationConfig::Delegated);

    // Wait for delegation to complete
    // Note: take_server_events() drains events every tick. If checking multiple events,
    // consider checking them in a single predicate or caching the result.
    scenario.tick_until(50, "delegation completes", |scenario| {
        let mut events = scenario.take_server_events();
        let mut found = false;
        for _ in events.read::<DelegateEntityEvent>() {
            found = true;
            break;
        }
        found
    });
    scenario.tick_until(20, "Client A sees Entity as Delegated", |scenario| {
        let (client_a, _) = scenario.client_a();
        client_a.entity_replication_config(&client_a_entity) == Some(naia_client::ReplicationConfig::Delegated)
    });

    // Wait for authority to be granted to Client A
    scenario.tick_until(30, "Client A sees Entity as having Auth Granted", |scenario| {
        let (client_a, _) = scenario.client_a();
        client_a.entity_authority_status(&client_a_entity) == Some(naia_shared::EntityAuthStatus::Granted)
    });

    // Client A releases authority (key step for bug reproduction!)
    let (client_a, world_a) = scenario.client_a();
    client_a
        .entity_mut(world_a.proxy_mut(), &client_a_entity)
        .release_authority();

    // Wait for authority to be released
    scenario.tick_until(30, "A available", |sc| {
        let (ca, _) = sc.client_a();
        ca.entity_authority_status(&client_a_entity) == Some(naia_shared::EntityAuthStatus::Available)
    });

    // Step 3: Setup Client B

    let _client_b_user_key = scenario.connect_b("Client B", Auth::new("client_b", "password"));

    // Setup: put entity in B scope (wrapped helper)
    scenario.include_in_scope_b(&server_entity, &_client_b_user_key);

    // Wait for Client B to receive entity and capture Client B's handle
    let client_b_entity = scenario
        .tick_until_map(100, "Client B receives entity", |sc| {
            sc.client_b_first_entity()
        })
        .expect("Client B should receive entity");

    // Wait for delegation handshake completion on B (no assumptions)
    scenario.tick_until(50, "Client B's entity is Delegated & Available", |scenario| {
        let (client_b, _) = scenario.client_b();
        client_b.entity_replication_config(&client_b_entity) == Some(naia_client::ReplicationConfig::Delegated)
            && client_b.entity_authority_status(&client_b_entity) == Some(naia_shared::EntityAuthStatus::Available)
    });

    // CONTRACT TEST: After EnableDelegation handshake completes, the entity should be ready for SetAuthority
    // This means the RemoteEntityChannel's AuthChannel should have auth_status=Available
    // NOTE: We can't directly access the channel, but we can verify the behavior through the entity_update_authority flow
    // The contract is: when SetAuthority arrives, get_remote_entity_auth_status should return Some(Available), not None

    // Step 4: Client B requests authority & modifies

    // B requests authority
    {
        let (client_b, client_b_world) = scenario.client_b();
        client_b.entity_mut(client_b_world.proxy_mut(), &client_b_entity).request_authority();
    }

    scenario.tick_until(50, "Client B's Entity has Auth Granted", |sc| {
        let (client_b, _) = sc.client_b();
        client_b.entity_authority_status(&client_b_entity) == Some(naia_shared::EntityAuthStatus::Granted)
    });

    // Verify Client A sees Available while B holds Granted
    scenario.tick_until(20, "A sees Available while B Granted", |scenario| {
        let (client_a, _) = scenario.client_a();
        client_a.entity_authority_status(&client_a_entity) == Some(naia_shared::EntityAuthStatus::Available)
    });

    // Guard + mutate
    let new_x = 100.0;
    let new_y = 200.0;
    {
        let (client_b, client_b_world) = scenario.client_b();
        assert_eq!(
            client_b.entity_authority_status(&client_b_entity),
            Some(naia_shared::EntityAuthStatus::Granted),
            "Client B must have authority before mutating"
        );
        client_b
            .entity_mut(client_b_world.proxy_mut(), &client_b_entity)
            .insert_component(Position::new(new_x, new_y));
    }

    // Step 5: Verify Client A receives update

    // Assert A receives update (using A's entity handle)
    scenario.tick_until(50, "A sees new position", |scenario| {
        scenario.a_entity_position(&client_a_entity) == Some((new_x, new_y))
    });
}
