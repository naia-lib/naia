use std::{time::Duration, collections::HashMap};

use log::{info, warn};

use naia_shared::{TestClock, Instant, Protocol, OwnedLocalEntity, LocalEntity};
use naia_client::{Client as NaiaClient, ClientConfig, ConnectEvent as ClientConnectEvent, JitterBufferType, transport::local::Socket as LocalClientSocket, EntityRef, EntityMut, SpawnEntityEvent as ClientSpawnEntityEvent};
use naia_demo_world::{WorldMut, WorldRef};
use naia_server::{Server as NaiaServer, ServerConfig, RoomKey, UserKey, Events, AuthEvent, transport::local::Socket as LocalServerSocket, SpawnEntityEvent as ServerSpawnEntityEvent};

use crate::{TestWorld, Auth, TestEntity, harness::{client_state::ClientState, users::Users, mutate_ctx::MutateCtx, ExpectCtx, ClientKey, EntityKey, builder::LocalTransportBuilder, entity_registry::EntityRegistry}};

type Client = NaiaClient<TestEntity>;
type Server = NaiaServer<TestEntity>;

/// Extract the comparable value from a LocalEntity.
/// 
/// This relies on Naia's current internal representation where `LocalEntity` wraps
/// an `OwnedLocalEntity` enum with a `u16` value. The server and client share the
/// same value for the same user's view of an entity.
/// 
/// # Brittleness
/// 
/// This assumes Naia's internal representation. If Naia changes how `LocalEntity`
/// is represented or provides a public API for comparison, this should be updated.
fn extract_local_entity_value(local_entity: &LocalEntity) -> u16 {
    let owned: OwnedLocalEntity = (*local_entity).into();
    match owned {
        OwnedLocalEntity::Host(v) | OwnedLocalEntity::Remote(v) => v,
    }
}

pub struct Scenario {
    builder: LocalTransportBuilder,
    server: Option<Server>,
    server_world: TestWorld,
    main_room: Option<RoomKey>,
    clients: HashMap<ClientKey, ClientState>,
    entity_registry: EntityRegistry,
    next_client_key: u32,
    protocol: Protocol,
    /// Forward mapping: ClientKey -> UserKey
    client_user_map: HashMap<ClientKey, UserKey>,
    /// Reverse mapping: UserKey -> ClientKey (for O(1) reverse lookups)
    user_to_client_map: HashMap<UserKey, ClientKey>,
}

impl Scenario {
    pub fn new(protocol: Protocol) -> Self {
        // Initialize simulated clock for deterministic test time
        TestClock::init(0);
        
        Self {
            builder: LocalTransportBuilder::default(),
            server: None,
            server_world: TestWorld::default(),
            main_room: None,
            clients: HashMap::new(),
            entity_registry: EntityRegistry::new(),
            next_client_key: 1,
            protocol,
            client_user_map: HashMap::new(),
            user_to_client_map: HashMap::new(),
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
            ClientState::new(client, world, user_key),
        );
        
        // Update bidirectional client-user mappings
        self.client_user_map.insert(client_key, user_key);
        self.user_to_client_map.insert(user_key, client_key);

        client_key
    }

    pub fn main_room_key(&self) -> Option<&RoomKey> {
        self.main_room.as_ref()
    }

    pub(crate) fn client_state_ref(&self, client_key: &ClientKey) -> &ClientState {
        self.clients.get(&client_key).expect("client not found")
    }

    pub(crate) fn client_state_mut(&mut self, client_key: &ClientKey) -> &mut ClientState {
        self.clients.get_mut(&client_key).expect("client not found")
    }

    /// Get client-side EntityRef by EntityKey.
    /// 
    /// Encapsulates LocalEntity lookup and EntityRef creation to avoid double-borrow issues.
    pub(crate) fn client_entity_ref(
        &'_ self,
        client_key: &ClientKey,
        user_key: &UserKey,
        key: &EntityKey,
    ) -> Option<EntityRef<'_, TestEntity, WorldRef<'_>>> {
        let local_entity = self.local_entity_for(key, user_key)?;

        let state = self.client_state_ref(client_key);
        let world_ref = state.world().proxy();
        state.client().local_entity(world_ref, &local_entity)
    }

    /// Get client-side EntityMut by EntityKey.
    /// 
    /// Encapsulates LocalEntity lookup and EntityMut creation to avoid double-borrow issues.
    pub(crate) fn client_entity_mut(
        &'_ mut self,
        client_key: &ClientKey,
        user_key: &UserKey,
        key: &EntityKey,
    ) -> Option<EntityMut<'_, TestEntity, WorldMut<'_>>> {
        let local_entity = self.local_entity_for(key, user_key)?;
        let state = self.client_state_mut(client_key);

        // Derive entity ID in tight scope to drop borrows before getting mutable references
        let entity = {
            let world_ref = state.world().proxy();
            let client_ref = state.client().local_entity(world_ref, &local_entity)?;
            client_ref.id()
        };

        let (client_mut, world_mut) = state.client_and_world_mut();
        let world_mut_proxy = world_mut.proxy_mut();
        Some(client_mut.entity_mut(world_mut_proxy, &entity))
    }

    pub(crate) fn entity_registry_mut(&mut self) -> &mut EntityRegistry {
        &mut self.entity_registry
    }

    /// Get read-only access to entity registry
    pub(crate) fn entity_registry(&self) -> &EntityRegistry {
        &self.entity_registry
    }

    /// Get read-only access to server world
    pub(crate) fn server_world_ref(&self) -> WorldRef<'_> {
        self.server_world.proxy()
    }

    /// Get read-only access to client state
    pub(crate) fn client_state(&self, client_key: &ClientKey) -> &ClientState {
        self.clients.get(client_key).expect("client not found")
    }

    /// Tick the simulation once - updates all clients and server
    /// 
    /// # Tick Phase Architecture
    /// 
    /// This method orchestrates a single simulation tick with four distinct phases:
    /// 
    /// ## Phase A (per-client): Update client + server network and worlds
    /// - For each client: `update_client_server_at()` handles bidirectional packet processing
    /// - Server is borrowed mutably across the entire client loop
    /// - This phase processes network I/O and world updates, but does NOT read events
    /// 
    /// ## Phase B (per-client): Collect client spawn events
    /// - `client.take_world_events()` is called once per client, immediately after that client's update
    /// - Client-spawned entities are identified and queued for registration
    /// - Events are collected into `spawns_to_register` for later processing
    /// 
    /// ## Phase C (once per tick): Collect server spawn events
    /// - `server.take_world_events()` is called exactly ONCE, after all client updates
    /// - Server spawn events may correspond to:
    ///   - Client-spawned entities (via `pending_client_spawns`) that just replicated to server
    ///   - Server-spawned entities that need to be registered
    /// - Events are collected into `server_spawn_data` for later processing
    /// 
    /// ## Phase D (once per tick): Apply all registry updates
    /// - Register client entities, server entities, and LocalEntity mappings
    /// - Must NOT call `take_world_events()` or modify client/server/world state
    /// - All borrows are dropped before this phase begins
    /// 
    /// # Invariants
    /// 
    /// - **`server.take_world_events()` called exactly once per tick**: This is critical for
    ///   correct event processing. Calling it multiple times would consume events prematurely.
    /// 
    /// - **`client.take_world_events()` called exactly once per client per tick**: Each client's
    ///   events are collected immediately after that client's update, preserving ordering.
    /// 
    /// - **`pending_client_spawns` entries are created in mutate phase, resolved in Phase C**:
    ///   When a client spawns an entity, it's tracked as pending. When the server receives the
    ///   corresponding `ServerSpawnEntityEvent`, the pending entry is resolved and removed.
    /// 
    /// - **Registry updates happen after all event collection**: This ensures all events from
    ///   the current tick are available for matching and registration.
    pub(crate) fn tick_once(&mut self) {
        // Advance simulated clock by 16ms (default tick duration for ~60 FPS)
        TestClock::advance(16);
        let now = Instant::now();

        let mut spawns_to_register = Vec::new();
        let mut server_spawn_data = Vec::new();

        // === Phase A: Update Clients and Server ===
        {
            let server = self.server.as_mut().expect("server not started");
            for (client_key, state) in self.clients.iter_mut() {
                let (client, world) = state.client_and_world_mut();
                update_client_server_at(
                    &now,
                    client,
                    server,
                    world,
                    &mut self.server_world,
                );

                // === Phase B: Collect Client Spawn Events (per-client) ===
                let mut client_events = state.client_mut().take_world_events();
                
                for spawned_entity in client_events.read::<ClientSpawnEntityEvent>() {
                    let world_ref = state.world().proxy();
                    let client_ref = state.client().entity(world_ref, &spawned_entity);
                    
                    if let Some(local_entity) = client_ref.local_entity() {
                        // Defer EntityKey lookup until after borrows are dropped (Phase D)
                        spawns_to_register.push((*client_key, local_entity, spawned_entity));
                    }
                }
            }

            // === Phase C: Collect Server Spawn Events (once per tick) ===
            // CRITICAL: Must be called exactly once per tick, after all client updates
            let mut server_events = server.take_world_events();
            for (spawn_user_key, spawn_entity) in server_events.read::<ServerSpawnEntityEvent>() {
                server_spawn_data.push((spawn_user_key, spawn_entity));
            }
        }

        // === Phase C (continued): Process Server Spawn Events ===
        let mut server_entities_to_register = Vec::new();
        let mut server_local_entity_mappings = Vec::new();
        
        for (spawn_user_key, spawn_entity) in server_spawn_data {
            if let Some(client_key) = self.client_key_for_user(&spawn_user_key) {
                // Get server's LocalEntity for this user (LocalEntity is shared between server and client for same user)
                let server_local_entity = {
                    let server = self.server.as_ref().expect("server not started");
                    let world_ref = self.server_world.proxy();
                    let server_ref = server.entity(world_ref, &spawn_entity);
                    server_ref.local_entity(&spawn_user_key)
                };
                
                if let Some(local_entity) = server_local_entity {
                    // Match EntityKey: client-spawned (via client mapping), server-spawned (via server mapping), or pending
                    if let Some(entity_key) = self.entity_registry.entity_key_for_client_entity(&client_key, &local_entity) {
                        // Client-spawned entity that replicated to server
                        server_entities_to_register.push((entity_key, spawn_entity));
                    } else if let Some(entity_key) = self.entity_registry.entity_key_for_server_entity(&spawn_entity) {
                        // Server-spawned entity - register LocalEntity mapping for this client
                        server_local_entity_mappings.push((entity_key, client_key, local_entity));
                    } else if let Some(entity_key) = self.entity_registry.remove_pending_client_spawn(&client_key) {
                        // Client-spawned entity not yet registered on server - consume pending entry
                        server_entities_to_register.push((entity_key, spawn_entity));
                        server_local_entity_mappings.push((entity_key, client_key, local_entity));
                    }
                }
            }
        }
        
        // === Phase C (continued): Register Server Entities ===
        // Register server entities before Phase D to ensure they're available for client spawn matching
        self.apply_server_entity_registrations(server_entities_to_register);
        
        // Register LocalEntity mappings for server-spawned entities
        // Client entity registration happens later when client receives SpawnEntityEvent
        self.apply_local_entity_mappings(server_local_entity_mappings);
        
        // === Phase D: Apply All Registry Updates ===
        // All borrows dropped - safe to mutate registry
        for (client_key, local_entity, client_entity) in spawns_to_register {
            let local_entity_value = extract_local_entity_value(&local_entity);
            
            // Skip if already registered (e.g., host client's own spawns from mutate phase)
            if let Some(_existing_key) = self.entity_registry.entity_key_for_client_entity(&client_key, &local_entity) {
                continue;
            }
            
            // Infer EntityKey by matching client's LocalEntity with server's LocalEntity for this user
            let user_key = self.user_key(&client_key);
            let (entity_key, server_entities_count) = {
                let server = self.server.as_ref().expect("server not started");
                let mut matched_key = None;
                let server_entities: Vec<_> = self.entity_registry.server_entities_iter().collect();
                let count = server_entities.len();
                
                // Match by LocalEntity value (same for server-client pairs, different between clients)
                for (ek, server_entity) in &server_entities {
                    let world_ref = self.server_world.proxy();
                    let server_ref = server.entity(world_ref, server_entity);
                    if let Some(server_local_entity) = server_ref.local_entity(&user_key) {
                        let server_value = extract_local_entity_value(&server_local_entity);
                        if server_value == local_entity_value {
                            matched_key = Some(*ek);
                            break;
                        }
                    }
                }
                
                (matched_key, count)
            };
            
            if let Some(entity_key) = entity_key {
                self.entity_registry_mut()
                    .register_client_entity(&entity_key, &client_key, &client_entity, &local_entity);
            } else {
                // Debug instrumentation: flag unexpected mapping failures
                // This triggers in debug builds when no LocalEntity match is found after checking all server entities
                debug_assert!(
                    false,
                    "Phase D: Failed to resolve EntityKey for client {:?} with LocalEntity value {} (checked {} server entities). \
                     This may indicate a mapping lifecycle violation - entity should resolve in a future tick.",
                    client_key, local_entity_value, server_entities_count
                );
                // If no LocalEntity match found, leave EntityKey unresolved - will be handled in future tick
                // when the mapping becomes available (strictly deterministic, no guessing)
            }
        }
    }

    pub(crate) fn take_server_events(&mut self) -> Events<TestEntity> {
        self.server.as_mut().expect("server not started").take_world_events()
    }

    pub(crate) fn user_key(&self, client_key: &ClientKey) -> UserKey {
        self.clients
            .get(&client_key)
            .expect("client not found")
            .user_key()
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

    /// Get LocalEntity for an EntityKey and UserKey.
    /// 
    /// Uses EntityRegistry as source of truth: checks client entities first, then falls back to server lookup.
    pub(crate) fn local_entity_for(&self, entity_key: &EntityKey, user_key: &UserKey) -> Option<LocalEntity> {
        let client_key = self.client_key_for_user(user_key)?;
        
        // Try client entity first (if registered)
        if let Some(client_entity) = self.entity_registry.client_entity(entity_key, &client_key) {
            let state = self.clients.get(&client_key)?;
            let world_ref = state.world().proxy();
            let client_ref = state.client().entity(world_ref, &client_entity);
            if let Some(local_entity) = client_ref.local_entity() {
                return Some(local_entity);
            }
        }
        
        // Fallback: server's perspective (returns None if entity hasn't replicated to this user yet)
        let server_entity = self.entity_registry.server_entity(entity_key)?;
        let server = self.server.as_ref()?;
        let server_ref = server.entity(self.server_world.proxy(), &server_entity);
        server_ref.local_entity(&user_key)
    }
    
    /// Get ClientKey for a UserKey (reverse lookup).
    /// 
    /// Uses reverse map for O(1) lookup instead of iterating clients.
    pub(crate) fn client_key_for_user(&self, user_key: &UserKey) -> Option<ClientKey> {
        self.user_to_client_map.get(user_key).copied()
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
        let users = Users::new(&self.client_user_map);
        (server, world, registry, users)
    }

    /// Apply server entity registrations (pure registry operation).
    /// 
    /// Only touches `EntityRegistry`; does not call `take_world_events()` or modify client/server/world state.
    /// Safe to call after all borrows are dropped.
    fn apply_server_entity_registrations(&mut self, server_entities: Vec<(EntityKey, TestEntity)>) {
        for (entity_key, server_entity) in server_entities {
            self.entity_registry_mut()
                .register_server_entity(&entity_key, &server_entity);
        }
    }

    /// Apply LocalEntity mappings for server-spawned entities (pure registry operation).
    /// 
    /// Only touches `EntityRegistry`; does not call `take_world_events()` or modify client/server/world state.
    /// Safe to call after all borrows are dropped.
    fn apply_local_entity_mappings(&mut self, mappings: Vec<(EntityKey, ClientKey, LocalEntity)>) {
        for (entity_key, client_key, local_entity) in mappings {
            // Skip if already registered (idempotent)
            if self.entity_registry.entity_key_for_client_entity(&client_key, &local_entity).is_none() {
                self.entity_registry_mut()
                    .register_client_local_entity_mapping(&entity_key, &client_key, &local_entity);
            }
        }
    }

}


/// Create a client socket from the builder
fn create_client_socket(builder: &LocalTransportBuilder) -> LocalClientSocket {
    let client_endpoint = builder.connect_client();
    LocalClientSocket::new(client_endpoint, None)
}

/// Create a server socket from the builder
fn create_server_socket(builder: &LocalTransportBuilder) -> LocalServerSocket {
    let server_endpoint = builder.server_endpoint();
    LocalServerSocket::new(server_endpoint, None)
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
    now: &Instant,
    client: &mut Client,
    server: &mut Server,
    client_world: &mut TestWorld,
    server_world: &mut TestWorld,
) {
    // Client update
    if client.connection_status().is_connected() {
        client.receive_all_packets();
        client.take_tick_events(now);
        client.process_all_packets(client_world.proxy_mut(), now);
        client.send_all_packets(client_world.proxy_mut());
    } else {
        client.receive_all_packets();
        client.send_all_packets(client_world.proxy_mut());
    }

    // Server update
    server.receive_all_packets();
    server.take_tick_events(now);
    server.process_all_packets(server_world.proxy_mut(), now);
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
    let mut user_added_to_room = false;

    for attempt in 1..=100 {
        TestClock::advance(16);
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
        
        server.send_all_packets(server_world.proxy());
        
        // Add user to room once ConnectEvent is processed
        // NOTE: add_user requires user to exist in world_server.users (added by ConnectEvent)
        // ConnectEvent may not be processed until after this iteration, so we retry each attempt
        if let Some(user_key) = user_key_opt {
            if !user_added_to_room && server.user_exists(&user_key) {
                server.room_mut(main_room_key).add_user(&user_key);
                if server.room(main_room_key).has_user(&user_key) {
                    user_added_to_room = true;
                }
            }
        }

        // Process client side
        let was_connected = client.connection_status().is_connected();
        if !was_connected {
            client.receive_all_packets();
            client.send_all_packets(client_world.proxy_mut());

            // Check for connection event (handshake manager may have completed connection)
            let mut client_events = client.take_world_events();
            for _ in client_events.read::<ClientConnectEvent>() {
                info!("{} connected in {} attempts", client_name, attempt);
                connected = true;
            }
        } else {
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

