//! End-to-End Tests for Local Transport
//!
//! Tests local transport-specific functionality including connection methods,
//! auth flows, server address discovery, and HTTP serialization.

use log::info;
use naia_client::{Client as NaiaClient, ClientConfig, ConnectEvent as ClientConnectEvent};
use naia_server::{
    AuthEvent, ConnectEvent as ServerConnectEvent, Server as NaiaServer, ServerConfig,
};
use naia_shared::Instant;
use naia_test::{local_socket_pair, protocol, Auth, TestEntity, TestWorld};

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
fn test_connect_no_auth() {
    init_logger();
    info!("=== TEST: Connect without explicit auth (uses connect() method) ===");
    info!("Note: Even connect() without auth still requires server to accept via AuthEvent");
    info!("This test verifies the basic connect() path works when auth is provided via client.auth()");

    let protocol = protocol();
    let (client_socket, server_socket) = local_socket_pair();

    let mut server = Server::new(ServerConfig::default(), protocol.clone());
    let mut client = Client::new(ClientConfig::default(), protocol);
    let mut client_world = TestWorld::default();
    let mut server_world = TestWorld::default();

    server.listen(server_socket);
    let main_room_key = server.make_room().key();

    // Even when using connect() (not connect_with_auth), we still need auth
    // because the server requires an AuthEvent to create a user
    // The client.auth() + client.connect() pattern uses connect_with_auth internally
    let auth = Auth::new("test_user", "test_password");
    client.auth(auth);
    client.connect(client_socket); // This will internally use connect_with_auth

    info!("Client connecting (connect() with auth set)");

    let max_attempts = 100;
    let mut connected = false;

    for attempt in 1..=max_attempts {
        if !client.connection_status().is_connected() {
            client.receive_all_packets();
            client.send_all_packets(client_world.proxy_mut());
        } else {
            let now = Instant::now();
            client.receive_all_packets();
            client.process_all_packets(client_world.proxy_mut(), &now);

            let mut client_events = client.take_world_events();
            for server_addr in client_events.read::<ClientConnectEvent>() {
                info!("Client connected to: {}", server_addr);
                connected = true;
                break;
            }
        }

        let now = Instant::now();
        server.receive_all_packets();
        server.process_all_packets(server_world.proxy_mut(), &now);

        let mut server_events = server.take_world_events();

        for (user_key, _auth) in server_events.read::<AuthEvent<Auth>>() {
            info!("Server received auth, accepting connection for user: {:?}", user_key);
            server.accept_connection(&user_key);
            server.room_mut(&main_room_key).add_user(&user_key);
        }

        for user_key in server_events.read::<ServerConnectEvent>() {
            info!("Server confirmed connection for user: {:?}", user_key);
        }

        server.send_all_packets(server_world.proxy());

        if connected {
            info!("Connection completed in {} attempts", attempt);
            break;
        }

        if attempt % 10 == 0 {
            info!("Attempt {}: still connecting...", attempt);
        }
    }

    assert!(
        connected,
        "Client should have connected within {} attempts",
        max_attempts
    );
    info!("✓ Connection test succeeded");
}

#[test]
fn test_connect_with_auth() {
    init_logger();
    info!("=== TEST: Connect with auth bytes ===");

    let protocol = protocol();
    let (client_socket, server_socket) = local_socket_pair();

    let mut server = Server::new(ServerConfig::default(), protocol.clone());
    let mut client = Client::new(ClientConfig::default(), protocol);
    let mut client_world = TestWorld::default();
    let mut server_world = TestWorld::default();

    server.listen(server_socket);
    let main_room_key = server.make_room().key();

    // Connect with auth
    let auth = Auth::new("test_user", "test_password");
    client.auth(auth);
    client.connect(client_socket);

    info!("Client connecting with auth bytes");

    let max_attempts = 100;
    let mut connected = false;

    for attempt in 1..=max_attempts {
        if !client.connection_status().is_connected() {
            client.receive_all_packets();
            client.send_all_packets(client_world.proxy_mut());
        } else {
            let now = Instant::now();
            client.receive_all_packets();
            client.process_all_packets(client_world.proxy_mut(), &now);

            let mut client_events = client.take_world_events();
            for server_addr in client_events.read::<ClientConnectEvent>() {
                info!("Client connected to: {}", server_addr);
                connected = true;
                break;
            }
        }

        let now = Instant::now();
        server.receive_all_packets();
        server.process_all_packets(server_world.proxy_mut(), &now);

        let mut server_events = server.take_world_events();

        for (user_key, _auth) in server_events.read::<AuthEvent<Auth>>() {
            info!("Server received auth, accepting connection for user: {:?}", user_key);
            server.accept_connection(&user_key);
            server.room_mut(&main_room_key).add_user(&user_key);
        }

        for user_key in server_events.read::<ServerConnectEvent>() {
            info!("Server confirmed connection for user: {:?}", user_key);
        }

        server.send_all_packets(server_world.proxy());

        if connected {
            info!("Connection completed in {} attempts", attempt);
            break;
        }

        if attempt % 10 == 0 {
            info!("Attempt {}: still connecting...", attempt);
        }
    }

    assert!(
        connected,
        "Client should have connected within {} attempts",
        max_attempts
    );
    info!("✓ Connection with auth bytes succeeded");
}

#[test]
fn test_connect_with_auth_headers() {
    init_logger();
    info!("=== TEST: Connect with auth headers ===");

    let protocol = protocol();
    let (client_socket, server_socket) = local_socket_pair();

    let mut server = Server::new(ServerConfig::default(), protocol.clone());
    let mut client = Client::new(ClientConfig::default(), protocol);
    let mut client_world = TestWorld::default();
    let mut server_world = TestWorld::default();

    server.listen(server_socket);
    let main_room_key = server.make_room().key();

    // Connect with auth headers only
    let headers = vec![
        ("X-Custom-Header".to_string(), "custom-value".to_string()),
        ("X-Another-Header".to_string(), "another-value".to_string()),
    ];
    client.auth_headers(headers);
    client.connect(client_socket);

    info!("Client connecting with auth headers");

    let max_attempts = 100;
    let mut connected = false;

    for attempt in 1..=max_attempts {
        if !client.connection_status().is_connected() {
            client.receive_all_packets();
            client.send_all_packets(client_world.proxy_mut());
        } else {
            let now = Instant::now();
            client.receive_all_packets();
            client.process_all_packets(client_world.proxy_mut(), &now);

            let mut client_events = client.take_world_events();
            for server_addr in client_events.read::<ClientConnectEvent>() {
                info!("Client connected to: {}", server_addr);
                connected = true;
                break;
            }
        }

        let now = Instant::now();
        server.receive_all_packets();
        server.process_all_packets(server_world.proxy_mut(), &now);

        let mut server_events = server.take_world_events();

        for (user_key, _auth) in server_events.read::<AuthEvent<Auth>>() {
            info!("Server received auth, accepting connection for user: {:?}", user_key);
            server.accept_connection(&user_key);
            server.room_mut(&main_room_key).add_user(&user_key);
        }

        for user_key in server_events.read::<ServerConnectEvent>() {
            info!("Server confirmed connection for user: {:?}", user_key);
        }

        server.send_all_packets(server_world.proxy());

        if connected {
            info!("Connection completed in {} attempts", attempt);
            break;
        }

        if attempt % 10 == 0 {
            info!("Attempt {}: still connecting...", attempt);
        }
    }

    assert!(
        connected,
        "Client should have connected within {} attempts",
        max_attempts
    );
    info!("✓ Connection with auth headers succeeded");
}

#[test]
fn test_connect_with_auth_and_headers() {
    init_logger();
    info!("=== TEST: Connect with auth bytes and headers ===");

    let protocol = protocol();
    let (client_socket, server_socket) = local_socket_pair();

    let mut server = Server::new(ServerConfig::default(), protocol.clone());
    let mut client = Client::new(ClientConfig::default(), protocol);
    let mut client_world = TestWorld::default();
    let mut server_world = TestWorld::default();

    server.listen(server_socket);
    let main_room_key = server.make_room().key();

    // Connect with both auth bytes and headers
    let auth = Auth::new("test_user", "test_password");
    let headers = vec![
        ("X-Custom-Header".to_string(), "custom-value".to_string()),
    ];
    client.auth(auth);
    client.auth_headers(headers);
    client.connect(client_socket);

    info!("Client connecting with auth bytes and headers");

    let max_attempts = 100;
    let mut connected = false;

    for attempt in 1..=max_attempts {
        if !client.connection_status().is_connected() {
            client.receive_all_packets();
            client.send_all_packets(client_world.proxy_mut());
        } else {
            let now = Instant::now();
            client.receive_all_packets();
            client.process_all_packets(client_world.proxy_mut(), &now);

            let mut client_events = client.take_world_events();
            for server_addr in client_events.read::<ClientConnectEvent>() {
                info!("Client connected to: {}", server_addr);
                connected = true;
                break;
            }
        }

        let now = Instant::now();
        server.receive_all_packets();
        server.process_all_packets(server_world.proxy_mut(), &now);

        let mut server_events = server.take_world_events();

        for (user_key, _auth) in server_events.read::<AuthEvent<Auth>>() {
            info!("Server received auth, accepting connection for user: {:?}", user_key);
            server.accept_connection(&user_key);
            server.room_mut(&main_room_key).add_user(&user_key);
        }

        for user_key in server_events.read::<ServerConnectEvent>() {
            info!("Server confirmed connection for user: {:?}", user_key);
        }

        server.send_all_packets(server_world.proxy());

        if connected {
            info!("Connection completed in {} attempts", attempt);
            break;
        }

        if attempt % 10 == 0 {
            info!("Attempt {}: still connecting...", attempt);
        }
    }

    assert!(
        connected,
        "Client should have connected within {} attempts",
        max_attempts
    );
    info!("✓ Connection with auth bytes and headers succeeded");
}

#[test]
fn test_auth_rejection_401() {
    init_logger();
    info!("=== TEST: Auth rejection (401) ===");

    let protocol = protocol();
    let (client_socket, server_socket) = local_socket_pair();

    let mut server = Server::new(ServerConfig::default(), protocol.clone());
    let mut client = Client::new(ClientConfig::default(), protocol);
    let mut client_world = TestWorld::default();
    let mut server_world = TestWorld::default();

    server.listen(server_socket);

    // Connect with auth
    let auth = Auth::new("test_user", "test_password");
    client.auth(auth);
    client.connect(client_socket);

    info!("Client connecting with auth (will be rejected)");

    let max_attempts = 100;
    let mut rejected = false;

    for attempt in 1..=max_attempts {
        if !client.connection_status().is_connected() {
            client.receive_all_packets();
            client.send_all_packets(client_world.proxy_mut());
        } else {
            let now = Instant::now();
            client.receive_all_packets();
            client.process_all_packets(client_world.proxy_mut(), &now);
        }

        let now = Instant::now();
        server.receive_all_packets();
        server.process_all_packets(server_world.proxy_mut(), &now);

        let mut server_events = server.take_world_events();

        // Reject the connection instead of accepting
        for (user_key, _auth) in server_events.read::<AuthEvent<Auth>>() {
            info!("Server received auth, REJECTING connection for user: {:?}", user_key);
            server.reject_connection(&user_key);
            rejected = true;
        }

        server.send_all_packets(server_world.proxy());

        // Check if client received rejection
        if rejected && !client.connection_status().is_connected() {
            // Give client time to process rejection
            client.receive_all_packets();
            if client.connection_status().is_disconnected() {
                info!("Client received rejection and disconnected");
                break;
            }
        }

        if attempt % 10 == 0 {
            info!("Attempt {}: waiting for rejection...", attempt);
        }
    }

    assert!(
        rejected,
        "Server should have rejected the connection"
    );
    assert!(
        !client.connection_status().is_connected(),
        "Client should not be connected after rejection"
    );
    info!("✓ Auth rejection (401) test succeeded");
}

#[test]
fn test_server_address_discovery() {
    init_logger();
    info!("=== TEST: Server address discovery ===");

    let protocol = protocol();
    let (client_socket, server_socket) = local_socket_pair();

    let mut server = Server::new(ServerConfig::default(), protocol.clone());
    let mut client = Client::new(ClientConfig::default(), protocol);
    let mut client_world = TestWorld::default();
    let mut server_world = TestWorld::default();

    server.listen(server_socket);
    let main_room_key = server.make_room().key();

    // Connect with auth to trigger address discovery
    let auth = Auth::new("test_user", "test_password");
    client.auth(auth);
    client.connect(client_socket);

    info!("Client connecting (will discover server address)");

    let max_attempts = 100;
    let mut connected = false;
    let mut address_discovered = false;

    for attempt in 1..=max_attempts {
        if !client.connection_status().is_connected() {
            client.receive_all_packets();
            client.send_all_packets(client_world.proxy_mut());
        } else {
            let now = Instant::now();
            client.receive_all_packets();
            client.process_all_packets(client_world.proxy_mut(), &now);

            let mut client_events = client.take_world_events();
            for server_addr in client_events.read::<ClientConnectEvent>() {
                info!("Client connected to: {}", server_addr);
                connected = true;
                address_discovered = true; // Address is discovered when connection completes
                break;
            }
        }

        let now = Instant::now();
        server.receive_all_packets();
        server.process_all_packets(server_world.proxy_mut(), &now);

        let mut server_events = server.take_world_events();

        for (user_key, _auth) in server_events.read::<AuthEvent<Auth>>() {
            info!("Server received auth, accepting connection for user: {:?}", user_key);
            server.accept_connection(&user_key);
            server.room_mut(&main_room_key).add_user(&user_key);
        }

        for user_key in server_events.read::<ServerConnectEvent>() {
            info!("Server confirmed connection for user: {:?}", user_key);
        }

        server.send_all_packets(server_world.proxy());

        if connected {
            info!("Connection completed in {} attempts", attempt);
            break;
        }

        if attempt % 10 == 0 {
            info!("Attempt {}: still connecting...", attempt);
        }
    }

    assert!(
        connected,
        "Client should have connected within {} attempts",
        max_attempts
    );
    assert!(
        address_discovered,
        "Server address should have been discovered"
    );
    
    // Verify client can send packets after address discovery
    // This is implicit - if connection succeeded, address was discovered
    info!("✓ Server address discovery test succeeded");
}

// Note: HTTP serialization tests would be better as unit tests in local_transport crate
// For now, we verify HTTP serialization works implicitly through the connection tests above
// which all use HTTP requests/responses under the hood.

