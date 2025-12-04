use std::collections::HashMap;
use std::time::Duration;

use log::{info, warn};

use naia_shared::{TestClock, Instant};
use naia_client::{Client as NaiaClient, ClientConfig, ConnectEvent as ClientConnectEvent, JitterBufferType, transport::local::Socket as LocalClientSocket};
use naia_server::{Server as NaiaServer, ServerConfig, RoomKey, UserKey, Events, AuthEvent, transport::local::Socket as LocalServerSocket};

use crate::{
    TestWorld, Auth, TestEntity, LocalTransportBuilder,
};
use super::keys::{ClientKey, EntityKey};
use super::entity_registry::EntityRegistry;

type Client = NaiaClient<TestEntity>;
type Server = NaiaServer<TestEntity>;

pub(crate) struct ClientState {
    pub(crate) client: Client,
    pub(crate) world: TestWorld,
    pub(crate) user_key: UserKey,
}

pub struct Scenario {
    now: Instant,
    builder: LocalTransportBuilder,
    server: Option<Server>,
    server_world: TestWorld,
    main_room: Option<RoomKey>,
    clients: HashMap<ClientKey, ClientState>,
    entity_registry: EntityRegistry,
    next_client_key: u32,
    protocol: naia_shared::Protocol,
    client_user_map: HashMap<ClientKey, UserKey>,
}

impl Scenario {
    pub fn new(protocol: naia_shared::Protocol) -> Self {
        // Initialize simulated clock for deterministic test time
        TestClock::init(0);
        
        Self {
            now: Instant::now(),
            builder: LocalTransportBuilder::default(),
            server: None,
            server_world: TestWorld::default(),
            main_room: None,
            clients: HashMap::new(),
            entity_registry: EntityRegistry::new(),
            next_client_key: 1,
            protocol,
            client_user_map: HashMap::new(),
        }
    }

    pub fn server_start(&mut self) {
        if self.server.is_some() {
            panic!("server_start() called multiple times");
        }

        let mut server = Server::new(ServerConfig::default(), self.protocol.clone());
        let server_socket = create_server_socket(&self.builder);
        server.listen(server_socket);
        let main_room = server.make_room().key();

        self.server = Some(server);
        self.main_room = Some(main_room);
    }

    pub fn client_connect(&mut self, auth: Auth, display_name: &str) -> ClientKey {
        if self.server.is_none() {
            panic!("server_start() must be called before client_connect()");
        }

        let client_key = ClientKey::new(self.next_client_key);
        self.next_client_key += 1;

        let mut client = Client::new(default_client_config(), self.protocol.clone());
        let mut world = TestWorld::default();
        let socket = create_client_socket(&self.builder);
        client.auth(auth);
        client.connect(socket);

        let server = self.server.as_mut().unwrap();
        let main_room = self.main_room.as_ref().unwrap();
        let user_key = complete_handshake_with_name(
            &mut client,
            server,
            &mut world,
            &mut self.server_world,
            main_room,
            display_name,
        )
        .expect("client should connect");

        self.clients.insert(
            client_key,
            ClientState {
                client,
                world,
                user_key,
            },
        );
        
        // Update client-user mapping for Users handle
        self.client_user_map.insert(client_key, user_key);

        client_key
    }

    pub fn main_room_key(&self) -> Option<&RoomKey> {
        self.main_room.as_ref()
    }

    pub(crate) fn client_state_mut(&mut self, client_key: ClientKey) -> &mut ClientState {
        self.clients.get_mut(&client_key).expect("client not found")
    }

    /// Get client-side EntityRef by EntityKey
    /// This helper encapsulates the LocalEntity lookup and EntityRef creation
    /// to avoid double-borrow issues in ClientMut
    pub(crate) fn client_entity_ref(
        &'_ mut self,
        client_key: ClientKey,
        user_key: UserKey,
        key: EntityKey,
    ) -> Option<naia_client::EntityRef<'_, TestEntity, naia_demo_world::WorldRef<'_>>> {
        let local_entity = self.local_entity_for(key, user_key)?;

        // Single mutable borrow of Scenario -> &mut ClientState
        let state = self.client_state_mut(client_key);

        // Short-lived shared borrows inside a block
        let world_ref = state.world.proxy();
        state.client.local_entity(world_ref, &local_entity)
    }

    /// Get client-side EntityMut by EntityKey
    /// This helper encapsulates the LocalEntity lookup and EntityMut creation
    /// to avoid double-borrow issues in ClientMut
    pub(crate) fn client_entity_mut(
        &'_ mut self,
        client_key: ClientKey,
        user_key: UserKey,
        key: EntityKey,
    ) -> Option<naia_client::EntityMut<'_, TestEntity, naia_demo_world::WorldMut<'_>>> {
        let local_entity = self.local_entity_for(key, user_key)?;

        // Single mutable borrow of Scenario -> &mut ClientState
        let state = self.client_state_mut(client_key);

        // First, derive the underlying entity id in a tight scope
        let entity = {
            let world_ref = state.world.proxy();                    // &TestWorld
            let client_ref = state.client.local_entity(world_ref, &local_entity)?;
            client_ref.id()
        }; // world_ref and client_ref dropped here

        // Then get a mutable world proxy and EntityMut
        let world_mut = state.world.proxy_mut();                   // &mut TestWorld → WorldMut<'_>
        Some(state.client.entity_mut(world_mut, &entity))
    }

    pub(crate) fn entity_registry_mut(&mut self) -> &mut EntityRegistry {
        &mut self.entity_registry
    }

    /// Tick the simulation once - updates all clients and server
    pub(crate) fn tick_once(&mut self) {
        // Advance simulated clock by 16ms (default tick duration for ~60 FPS)
        TestClock::advance(16);
        let now = Instant::now();
        
        // Use current time for this tick (we update self.now at the end)
        let server = self.server.as_mut().expect("server not started");

        // update each client-server pair sequentially
        // This is not ideal but works for now
        // Note: We use Instant::now() for each iteration since now is moved
        for state in self.clients.values_mut() {
            update_client_server_at(
                now.clone(),
                &mut state.client,
                server,
                &mut state.world,
                &mut self.server_world,
            );
        }

        self.now = Instant::now();
    }

    pub(crate) fn take_server_events(&mut self) -> Events<TestEntity> {
        self.server.as_mut().expect("server not started").take_world_events()
    }

    pub(crate) fn user_key(&self, client_key: ClientKey) -> UserKey {
        self.clients
            .get(&client_key)
            .expect("client not found")
            .user_key
    }

    /// Get server host entity for an EntityKey
    pub(crate) fn server_host_entity(&self, entity_key: EntityKey) -> Option<TestEntity> {
        self.entity_registry.host_world(entity_key)
    }

    /// Get LocalEntity for an EntityKey and UserKey
    /// For client-spawned entities, if the user_key matches the spawning client, use their LocalEntity directly
    /// For other clients, get LocalEntity from server's perspective
    /// This will return None if the entity hasn't been replicated to that user yet
    pub(crate) fn local_entity_for(&self, entity_key: EntityKey, user_key: UserKey) -> Option<naia_shared::LocalEntity> {
        // Check if this is a client-spawned entity and if the user_key matches the spawning client
        if let Some((spawning_client_key, spawning_local_entity)) = self.entity_registry.spawning_client(entity_key) {
            let spawning_user_key = self.user_key(spawning_client_key);
            if user_key == spawning_user_key {
                // This is the spawning client - use their LocalEntity directly
                return Some(spawning_local_entity);
            }
        }
        
        // For other clients or server-spawned entities, get LocalEntity from server's perspective
        // This will return None if the entity hasn't been replicated to that user yet
        // The expect() loop will keep ticking until replication completes and this returns Some
        let host_entity = self.entity_registry.host_world(entity_key)?;
        let server = self.server.as_ref()?;
        let host_ref = server.entity(self.server_world.proxy(), &host_entity);
        host_ref.local_entity(&user_key) // Now returns Option, so propagate it
    }

    /// Perform actions in a mutate phase
    pub fn mutate<R>(&mut self, f: impl FnOnce(&mut super::mutate_ctx::MutateCtx) -> R) -> R {
        use super::mutate_ctx::MutateCtx;
        let mut ctx = MutateCtx::new(self);
        let result = f(&mut ctx);
        // Tick at least once after actions to propagate immediate effects
        self.tick_once();
        result
    }

    /// Register expectations and wait until they all pass or timeout
    pub fn expect(&mut self, f: impl FnMut(&mut super::expect_ctx::ExpectCtx) -> bool) {
        use super::expect_ctx::ExpectCtx;
        let mut ctx = ExpectCtx::new(self, 50); // Default max_ticks
        ctx.run(f);
    }

    /// Split borrow fields needed for ServerMut
    /// Returns disjoint mutable references to server, world, registry, and users
    pub(crate) fn split_for_server_mut(
        &mut self,
    ) -> (
        &mut Server,
        &mut TestWorld,
        &mut EntityRegistry,
        super::users::Users<'_>,
    ) {
        let server = self.server.as_mut().expect("server not started");
        let world = &mut self.server_world;
        let registry = &mut self.entity_registry;
        let users = super::users::Users {
            mapping: &self.client_user_map,
        };
        (server, world, registry, users)
    }

    /// Internal helper: wait for server to register client-spawned entity
    /// Reuses existing ClientSpawnBuilder::track() logic
    pub(crate) fn spawn_and_track_client_entity(
        &mut self,
        entity_key: EntityKey,
        client_key: ClientKey,
        local_entity: naia_shared::LocalEntity,
    ) {
        let user_key = self.user_key(client_key);
        let mut attempts = 0;
        const MAX_ATTEMPTS: usize = 200;

        loop {
            attempts += 1;
            if attempts > MAX_ATTEMPTS {
                panic!(
                    "spawn_and_track_client_entity() timed out after {} ticks waiting for server to have entity with LocalEntity {:?}",
                    MAX_ATTEMPTS, local_entity
                );
            }

            self.tick_once();

            // Check if server has SpawnEntityEvent for this user
            let mut server_events = self.take_server_events();
            for (spawn_user_key, spawn_entity) in server_events.read::<naia_server::SpawnEntityEvent>() {
                if spawn_user_key == user_key {
                    // Found the spawn event - this is the server's entity for the client-spawned entity
                    // Verify it exists in the server world and register it
                    let server = self.server.as_ref().expect("server not started");
                    if server.entities(self.server_world.proxy()).contains(&spawn_entity) {
                        // Register the server entity - this is the host entity for this EntityKey
                        self.entity_registry_mut()
                            .register_host_entity(entity_key, spawn_entity);
                        return;
                    }
                }
            }
        }
    }
}


/// Create a client socket from the builder
fn create_client_socket(builder: &LocalTransportBuilder) -> LocalClientSocket {
    let client_endpoint = builder.connect_client();
    LocalClientSocket::new(client_endpoint.into_socket(), None)
}

/// Create a server socket from the builder
fn create_server_socket(builder: &LocalTransportBuilder) -> LocalServerSocket {
    let server_endpoint = builder.server_endpoint();
    LocalServerSocket::new(server_endpoint.into_socket(), None)
}

/// Create default client config for tests (fast handshake, no jitter buffer)
fn default_client_config() -> ClientConfig {
    let mut config = ClientConfig::default();
    config.send_handshake_interval = Duration::from_millis(0);
    config.jitter_buffer = JitterBufferType::Bypass;
    config
}

/// Update a single client and server at a specific time
fn update_client_server_at(
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
        // Advance simulated time for each handshake attempt
        TestClock::advance(16); // Advance by one tick (16ms)
        let now = Instant::now();

        // Process server side first to receive client packets
        server.receive_all_packets();
        server.take_tick_events(&now);
        server.process_all_packets(server_world.proxy_mut(), &now);

        let mut server_events = server.take_world_events();
        for (user_key, _auth) in server_events.read::<AuthEvent<Auth>>() {
            info!("Server accepting connection for {}: {:?}", client_name, user_key);
            server.accept_connection(&user_key);
            server.room_mut(main_room_key).add_user(&user_key);
            user_key_opt = Some(user_key);
        }
        server.send_all_packets(server_world.proxy());

        // Then process client side
        let was_connected = client.connection_status().is_connected();
        if !was_connected {
            // For handshake, receive then send to allow handshake manager to process
            client.receive_all_packets();
            client.send_all_packets(client_world.proxy_mut());

            // Check for connection event even during handshake
            // (the handshake manager may have completed the connection)
            let mut client_events = client.take_world_events();
            for _ in client_events.read::<ClientConnectEvent>() {
                info!("{} connected in {} attempts", client_name, attempt);
                connected = true;
            }
        } else {
            // If client is already connected, process normally
            client.receive_all_packets();
            client.process_all_packets(client_world.proxy_mut(), &now);
            client.take_tick_events(&now);
            client.send_all_packets(client_world.proxy_mut());
        }

        if connected && user_key_opt.is_some() {
            break;
        }
    }

    if connected && user_key_opt.is_some() {
        user_key_opt
    } else {
        if !connected {
            warn!("{} handshake failed: client never connected after 100 attempts", client_name);
        } else if user_key_opt.is_none() {
            warn!("{} handshake failed: client connected but server never accepted after 100 attempts", client_name);
        }
        None
    }
}

