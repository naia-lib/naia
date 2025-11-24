//! Shared helpers for End-to-End tests
//!
//! This module provides common utilities for E2E tests including:
//! - Client/server update loops
//! - Handshake completion
//! - Waiting for conditions/events
//! - Local transport setup

use std::time::Duration;

use log::info;

use naia_shared::Instant;
use naia_client::{Client as NaiaClient, ClientConfig, ConnectEvent as ClientConnectEvent, transport::local::Socket as LocalClientSocket, JitterBufferType};
use naia_server::{AuthEvent, DelegateEntityEvent, Server as NaiaServer, transport::local::Socket as LocalServerSocket};

use crate::{Auth, LocalTransportBuilder, TestEntity, TestWorld};

type Client = NaiaClient<TestEntity>;
type Server = NaiaServer<TestEntity>;

/// Create a client socket from the builder
pub fn create_client_socket(builder: &LocalTransportBuilder) -> LocalClientSocket {
    let client_endpoint = builder.connect_client();
    LocalClientSocket::new(client_endpoint.into_socket(), None)
}

/// Create a server socket from the builder
pub fn create_server_socket(builder: &LocalTransportBuilder) -> LocalServerSocket {
    let server_endpoint = builder.server_endpoint();
    LocalServerSocket::new(server_endpoint.into_socket(), None)
}

/// Create default client config for tests (fast handshake, no jitter buffer)
pub fn default_client_config() -> ClientConfig {
    let mut config = ClientConfig::default();
    config.send_handshake_interval = Duration::from_millis(0);
    config.jitter_buffer = JitterBufferType::Bypass;
    config
}

/// Update a single client and server at a specific time
pub fn update_client_server_at(
    now: Instant,
    client: &mut Client,
    server: &mut Server,
    client_world: &mut TestWorld,
    server_world: &mut TestWorld,
) {
    // Client update
    if client.connection_status().is_connected() {
        client.receive_all_packets();
        client.take_tick_events(&now);
        client.process_all_packets(client_world.proxy_mut(), &now);
        client.send_all_packets(client_world.proxy_mut());
    } else {
        client.receive_all_packets();
        client.send_all_packets(client_world.proxy_mut());
    }

    // Server update
    server.receive_all_packets();
    server.take_tick_events(&now);
    server.process_all_packets(server_world.proxy_mut(), &now);
    server.send_all_packets(server_world.proxy());
}

/// Update a single client and server
pub fn update_client_server(
    client: &mut Client,
    server: &mut Server,
    client_world: &mut TestWorld,
    server_world: &mut TestWorld,
) {
    let now = Instant::now();
    update_client_server_at(now, client, server, client_world, server_world);
}

/// Update two clients and a server at a specific time
pub fn update_all_at(
    now: Instant,
    client_a: &mut Client,
    client_b: &mut Client,
    server: &mut Server,
    client_a_world: &mut TestWorld,
    client_b_world: &mut TestWorld,
    server_world: &mut TestWorld,
) {
    // Client A update
    if client_a.connection_status().is_connected() {
        client_a.receive_all_packets();
        client_a.take_tick_events(&now);
        client_a.process_all_packets(client_a_world.proxy_mut(), &now);
        client_a.send_all_packets(client_a_world.proxy_mut());
    } else {
        client_a.receive_all_packets();
        client_a.send_all_packets(client_a_world.proxy_mut());
    }

    // Client B update
    if client_b.connection_status().is_connected() {
        client_b.receive_all_packets();
        client_b.take_tick_events(&now);
        client_b.process_all_packets(client_b_world.proxy_mut(), &now);
        client_b.send_all_packets(client_b_world.proxy_mut());
    } else {
        client_b.receive_all_packets();
        client_b.send_all_packets(client_b_world.proxy_mut());
    }

    // Server update
    server.receive_all_packets();
    server.take_tick_events(&now);
    server.process_all_packets(server_world.proxy_mut(), &now);
    server.send_all_packets(server_world.proxy());
}

/// Update two clients and a server
pub fn update_all(
    client_a: &mut Client,
    client_b: &mut Client,
    server: &mut Server,
    client_a_world: &mut TestWorld,
    client_b_world: &mut TestWorld,
    server_world: &mut TestWorld,
) {
    let now = Instant::now();
    update_all_at(now, client_a, client_b, server, client_a_world, client_b_world, server_world);
}

/// Run N update cycles for a single client-server pair
pub fn run_updates(
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

/// Complete handshake for a client
pub fn complete_handshake(
    client: &mut Client,
    server: &mut Server,
    client_world: &mut TestWorld,
    server_world: &mut TestWorld,
    main_room_key: &naia_server::RoomKey,
) -> Option<naia_server::UserKey> {
    complete_handshake_with_name(client, server, client_world, server_world, main_room_key, "Client")
}

/// Complete handshake for a client with a custom name for logging
pub fn complete_handshake_with_name(
    client: &mut Client,
    server: &mut Server,
    client_world: &mut TestWorld,
    server_world: &mut TestWorld,
    main_room_key: &naia_server::RoomKey,
    client_name: &str,
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
                info!("{} connected in {} attempts", client_name, attempt);
                connected = true;
                break;
            }
        }

        let now = Instant::now();
        server.receive_all_packets();
        server.process_all_packets(server_world.proxy_mut(), &now);

        let mut server_events = server.take_world_events();

        for (user_key, _auth) in server_events.read::<AuthEvent<Auth>>() {
            info!("Server accepting connection for {}: {:?}", client_name, user_key);
            server.accept_connection(&user_key);
            server.room_mut(main_room_key).add_user(&user_key);
            user_key_opt = Some(user_key);
        }

        server.send_all_packets(server_world.proxy());

        if connected {
            break;
        }
    }

    if connected {
        user_key_opt
    } else {
        None
    }
}

/// Wait for a condition to become true
pub fn wait_for_condition<F>(
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

/// Wait for a specific authority status on a client
pub fn wait_for_authority_status(
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

/// Wait for a delegation event on the server
pub fn wait_for_delegation_event(
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

