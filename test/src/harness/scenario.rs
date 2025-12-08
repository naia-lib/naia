use std::{time::Duration, collections::HashMap};

use log::{info, warn};

use naia_shared::{TestClock, Instant, Protocol, OwnedLocalEntity, LocalEntity};
use naia_client::{Client as NaiaClient, ClientConfig, ConnectEvent as ClientConnectEvent, JitterBufferType, transport::local::Socket as LocalClientSocket, EntityRef, EntityMut, SpawnEntityEvent as ClientSpawnEntityEvent};
use naia_demo_world::{WorldMut, WorldRef};
use naia_server::{Server as NaiaServer, ServerConfig, RoomKey, UserKey, Events, AuthEvent, transport::local::Socket as LocalServerSocket, SpawnEntityEvent as ServerSpawnEntityEvent};

use crate::{TestWorld, Auth, TestEntity, harness::{users::Users, mutate_ctx::MutateCtx, ExpectCtx, ClientKey, EntityKey, builder::LocalTransportBuilder, entity_registry::EntityRegistry}};

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
    protocol: Protocol,
    client_user_map: HashMap<ClientKey, UserKey>,
}

impl Scenario {
    pub fn new(protocol: Protocol) -> Self {
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

    pub fn client_connect(&mut self, display_name: &str, auth: Auth) -> ClientKey {
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

    pub(crate) fn client_state_mut(&mut self, client_key: &ClientKey) -> &mut ClientState {
        self.clients.get_mut(&client_key).expect("client not found")
    }

    /// Get client-side EntityRef by EntityKey
    /// This helper encapsulates the LocalEntity lookup and EntityRef creation
    /// to avoid double-borrow issues in ClientMut
    pub(crate) fn client_entity_ref(
        &'_ mut self,
        client_key: &ClientKey,
        user_key: &UserKey,
        key: &EntityKey,
    ) -> Option<EntityRef<'_, TestEntity, WorldRef<'_>>> {
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
        client_key: &ClientKey,
        user_key: &UserKey,
        key: &EntityKey,
    ) -> Option<EntityMut<'_, TestEntity, WorldMut<'_>>> {
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

        // Collect client spawns to register after all updates complete
        let mut spawns_to_register = Vec::new();

        // update each client-server pair sequentially
        // This is not ideal but works for now
        // Note: We use Instant::now() for each iteration since now is moved
        let mut server_spawn_data = Vec::new();
        {
            let server = self.server.as_mut().expect("server not started");
            for (client_key, state) in self.clients.iter_mut() {
                update_client_server_at(
                    now.clone(),
                    &mut state.client,
                    server,
                    &mut state.world,
                    &mut self.server_world,
                );

                // Collect spawn events for this client
                let mut client_events = state.client.take_world_events();
                
                // Process SpawnEntityEvent for this client
                for spawned_entity in client_events.read::<ClientSpawnEntityEvent>() {
                    // Get the client's LocalEntity for this entity
                    let world_ref = state.world.proxy();
                    let client_ref = state.client.entity(world_ref, &spawned_entity);
                    
                    if let Some(local_entity) = client_ref.local_entity() {
                        // Look up EntityKey via LocalEntity mapping
                        // We'll do the actual lookup after dropping the state borrow
                        spawns_to_register.push((*client_key, local_entity, spawned_entity));
                    }
                }
            }

            // Collect server events before dropping server borrow
            let mut server_events = server.take_world_events();
            for (spawn_user_key, spawn_entity) in server_events.read::<ServerSpawnEntityEvent>() {
                server_spawn_data.push((spawn_user_key, spawn_entity));
            }
        }

        // Process server spawn events to register server entities and LocalEntity mappings
        let mut server_entities_to_register = Vec::new();
        let mut server_local_entity_mappings = Vec::new();
        
        for (spawn_user_key, spawn_entity) in server_spawn_data {
            // Find the ClientKey for this UserKey
            if let Some(client_key) = self.client_key_for_user(&spawn_user_key) {
                // Get the server's LocalEntity for this user (need server borrow)
                let server_local_entity = {
                    let server = self.server.as_ref().expect("server not started");
                    let world_ref = self.server_world.proxy();
                    let server_ref = server.entity(world_ref, &spawn_entity);
                    server_ref.local_entity(&spawn_user_key)
                };
                
                if let Some(local_entity) = server_local_entity {
                    // Try to find EntityKey via client's LocalEntity mapping (for client-spawned entities)
                    // The LocalEntity is the same on server and client for the same user
                    if let Some(entity_key) = self.entity_registry.entity_key_for_client_entity(&client_key, &local_entity) {
                        // This is a client-spawned entity - register the server entity
                        server_entities_to_register.push((entity_key, spawn_entity));
                    } else if let Some(entity_key) = self.entity_registry.entity_key_for_server_entity(&spawn_entity) {
                        // This is a server-spawned entity - register the LocalEntity mapping
                        server_local_entity_mappings.push((entity_key, client_key, local_entity));
                    } else if let Some(entity_key) = self.entity_registry.find_pending_client_spawned_entity(&client_key) {
                        // This is a client-spawned entity that we haven't registered on server yet
                        server_entities_to_register.push((entity_key, spawn_entity));
                        // Also register the server's LocalEntity mapping for this client
                        server_local_entity_mappings.push((entity_key, client_key, local_entity));
                    }
                }
            }
        }
        
        // Now register all the client entities (all borrows are dropped)
        for (client_key, local_entity, client_entity) in spawns_to_register {
            // Try to find EntityKey via LocalEntity mapping first
            let entity_key = if let Some(ek) = self.entity_registry.entity_key_for_client_entity(&client_key, &local_entity) {
                Some(ek)
            } else {
                // If not found, try to find by matching LocalEntity value with server's LocalEntity
                // This handles the case where Client B receives a SpawnEntityEvent for an entity spawned by Client A
                // The server's LocalEntity for UserKey(1) will have the same value as Client B's LocalEntity
                let user_key = self.user_key(&client_key);
                let local_entity_value = {
                    let owned: OwnedLocalEntity = local_entity.into();
                    // value() is pub(crate), so we need to match to extract the value
                    match owned {
                        OwnedLocalEntity::Host(v) | OwnedLocalEntity::Remote(v) => v,
                    }
                };
                
                // Look for EntityKey that has a server entity and a LocalEntity mapping for this user with matching value
                // We need to check all EntityKeys and see if any have a server LocalEntity for this user with the same value
                let server = self.server.as_ref().expect("server not started");
                self.entity_registry.server_entities_iter()
                    .find_map(|(ek, server_entity)| {
                        // Get server's LocalEntity for this user
                        let world_ref = self.server_world.proxy();
                        let server_ref = server.entity(world_ref, &server_entity);
                        if let Some(server_local_entity) = server_ref.local_entity(&user_key) {
                            let server_value = {
                                let owned: OwnedLocalEntity = server_local_entity.into();
                                match owned {
                                    OwnedLocalEntity::Host(v) | OwnedLocalEntity::Remote(v) => v,
                                }
                            };
                            if server_value == local_entity_value {
                                return Some(ek);
                            }
                        }
                        None
                    })
            };
            
            if let Some(entity_key) = entity_key {
                // Register the client entity and LocalEntity mapping
                self.entity_registry_mut()
                    .register_client_entity(&entity_key, &client_key, &client_entity, &local_entity);
            }
            // If EntityKey not found, the entity hasn't been registered on server yet
            // It will be registered in a future tick when server processes it
        }
        
        // Register server entities for client-spawned entities
        for (entity_key, server_entity) in server_entities_to_register {
            self.entity_registry_mut()
                .register_server_entity(&entity_key, &server_entity);
        }
        
        // Register LocalEntity mappings for server-spawned entities
        // Note: For server-spawned entities, we only register the LocalEntity mapping here
        // The client entity will be registered later when the client receives SpawnEntityEvent
        for (entity_key, client_key, local_entity) in server_local_entity_mappings {
            // Only register if not already registered (avoid duplicates)
            if self.entity_registry.entity_key_for_client_entity(&client_key, &local_entity).is_none() {
                self.entity_registry_mut()
                    .register_client_local_entity_mapping(&entity_key, &client_key, &local_entity);
            }
        }

        self.now = Instant::now();
    }

    pub(crate) fn take_server_events(&mut self) -> Events<TestEntity> {
        self.server.as_mut().expect("server not started").take_world_events()
    }

    pub(crate) fn user_key(&self, client_key: &ClientKey) -> UserKey {
        self.clients
            .get(&client_key)
            .expect("client not found")
            .user_key
    }

    /// Get server host entity for an EntityKey
    pub(crate) fn server_host_entity(&self, entity_key: &EntityKey) -> Option<TestEntity> {
        self.entity_registry.server_entity(entity_key)
    }

    /// Get UserKey for a ClientKey
    pub(crate) fn user_key_for_client(&self, client_key: &ClientKey) -> Option<UserKey> {
        self.client_user_map.get(&client_key).copied()
    }

    /// Get immutable access to server and registry for expect operations
    pub(crate) fn server_and_registry(&self) -> Option<(&Server, &EntityRegistry)> {
        Some((self.server.as_ref()?, &self.entity_registry))
    }

    /// Get LocalEntity for an EntityKey and UserKey
    /// Uses EntityRegistry as source of truth - checks client entities first, then falls back to server lookup
    pub(crate) fn local_entity_for(&self, entity_key: &EntityKey, user_key: &UserKey) -> Option<LocalEntity> {
        // Find the client_key for this user_key
        let client_key = self.client_key_for_user(user_key)?;
        
        // Check if client's TestEntity is registered in EntityRegistry
        if let Some(client_entity) = self.entity_registry.client_entity(entity_key, &client_key) {
            // Client entity is registered - get LocalEntity from client using the TestEntity
            // Note: client.entity() will panic if entity doesn't exist, but if it's registered it should exist
            let state = self.clients.get(&client_key)?;
            let world_ref = state.world.proxy();
            // Try to get LocalEntity - if entity doesn't exist, this will panic, but that's OK
            // because if it's registered in EntityRegistry, it should exist
            let client_ref = state.client.entity(world_ref, &client_entity);
            if let Some(local_entity) = client_ref.local_entity() {
                return Some(local_entity);
            }
        }
        
        // Fallback: get LocalEntity from server's perspective
        // This will return None if the entity hasn't been replicated to that user yet
        // The expect() loop will keep ticking until replication completes and this returns Some
        let server_entity = self.entity_registry.server_entity(entity_key)?;
        let server = self.server.as_ref()?;
        let server_ref = server.entity(self.server_world.proxy(), &server_entity);
        server_ref.local_entity(&user_key) // Now returns Option, so propagate it
    }
    
    /// Get ClientKey for a UserKey (reverse lookup)
    fn client_key_for_user(&self, user_key: &UserKey) -> Option<ClientKey> {
        self.clients.iter()
            .find(|(_, state)| state.user_key == *user_key)
            .map(|(key, _)| *key)
    }

    /// Perform actions in a mutate phase
    pub fn mutate<R>(&mut self, f: impl FnOnce(&mut MutateCtx) -> R) -> R {
        let mut ctx = MutateCtx::new(self);
        let result = f(&mut ctx);
        // Tick at least once after actions to propagate immediate effects
        self.tick_once();
        result
    }

    /// Register expectations and wait until they all pass or timeout
    pub fn expect(&mut self, f: impl FnMut(&mut ExpectCtx) -> bool) {
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
        Users<'_>,
    ) {
        let server = self.server.as_mut().expect("server not started");
        let world = &mut self.server_world;
        let registry = &mut self.entity_registry;
        let users = Users {
            mapping: &self.client_user_map,
        };
        (server, world, registry, users)
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
    main_room_key: &RoomKey,
    client_name: &str,
) -> Option<UserKey> {
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
            user_key_opt = Some(user_key);
        }
        
        // After accept_connection, receive_all_packets will process ConnectEvent and add user to WorldServer
        // We need to call receive_all_packets again to process the ConnectEvent, then add user to room
        server.receive_all_packets();
        server.process_all_packets(server_world.proxy_mut(), &now);
        
        // Now check if user exists in WorldServer and add them to room
        if let Some(user_key) = user_key_opt {
            if server.user_exists(&user_key) {
                info!("Server adding user to room for {}: {:?}", client_name, user_key);
                server.room_mut(main_room_key).add_user(&user_key);
            }
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

