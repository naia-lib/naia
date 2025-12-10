use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Duration,
};

use log::{debug, warn};

use naia_client::{
    transport::local::{LocalAddrCell, Socket as LocalClientSocket},
    Client as NaiaClient,
    ClientConfig,
    EntityMut,
    EntityRef,
    JitterBufferType,
    SpawnEntityEvent as ClientSpawnEntityEvent,
    WorldEvents as ClientEvents,
};
use naia_demo_world::{WorldMut, WorldRef};
use naia_server::{
    transport::local::Socket as LocalServerSocket,
    AuthEvent,
    ConnectEvent,
    Events as ServerEvents,
    Server as NaiaServer,
    ServerConfig,
    SpawnEntityEvent as ServerSpawnEntityEvent,
    UserKey,
};
use naia_shared::{
    transport::local::{LocalTransportHub, FAKE_SERVER_ADDR},
    Instant,
    LocalEntity,
    OwnedLocalEntity,
    Protocol,
    TestClock,
};

use crate::{
    harness::{
        client_state::ClientState,
        entity_registry::EntityRegistry,
        mutate_ctx::MutateCtx,
        users::Users,
        ClientKey,
        EntityKey,
        ExpectCtx,
    },
    Auth,
    TestEntity,
    TestWorld,
};

type Client = NaiaClient<TestEntity>;
type Server = NaiaServer<TestEntity>;

// Constants for simulation timing and retry behavior
const TICK_DURATION_MS: u64 = 16; // Default tick duration (~60 FPS)
const DEFAULT_MAX_EXPECT_TICKS: usize = 50; // Maximum ticks before expect() times out

/// Extract the comparable value from a LocalEntity.
/// 
/// This relies on Naia's current internal representation where `LocalEntity` wraps
/// an `OwnedLocalEntity` enum with a `u16` value. The server and client share the
/// same value for the same user's view of an entity.
/// 
/// # TODO: Brittleness
/// 
/// This assumes Naia's internal representation. If Naia changes how `LocalEntity`
/// is represented or provides a public API for comparison, this should be updated.
/// Consider contributing a public comparison API to the naia crate.
fn extract_local_entity_value(local_entity: &LocalEntity) -> u16 {
    let owned: OwnedLocalEntity = (*local_entity).into();
    match owned {
        OwnedLocalEntity::Host(v) | OwnedLocalEntity::Remote(v) => v,
    }
}

pub struct Scenario {
    hub: LocalTransportHub,
    server: Option<Server>,
    server_world: TestWorld,
    clients: HashMap<ClientKey, ClientState>,
    entity_registry: EntityRegistry,
    next_client_key: u32,
    protocol: Protocol,
    /// Forward mapping: ClientKey -> UserKey
    client_user_map: HashMap<ClientKey, UserKey>,
    /// Reverse mapping: UserKey -> ClientKey (for O(1) reverse lookups)
    user_to_client_map: HashMap<UserKey, ClientKey>,
    /// Pending auth payloads for clients that have started connecting but not yet authenticated
    pending_auths: HashMap<ClientKey, Auth>,
}

impl Scenario {
    pub fn new(protocol: Protocol) -> Self {
        // Initialize simulated clock for deterministic test time
        TestClock::init(0);
        
        let server_addr: SocketAddr = FAKE_SERVER_ADDR.parse().expect("invalid server addr");
        let hub = LocalTransportHub::new(server_addr);
        
        Self {
            hub,
            server: None,
            server_world: TestWorld::default(),
            clients: HashMap::new(),
            entity_registry: EntityRegistry::new(),
            next_client_key: 1,
            protocol,
            client_user_map: HashMap::new(),
            user_to_client_map: HashMap::new(),
            pending_auths: HashMap::new(),
        }
    }

    pub fn server_start(&mut self) {
        if self.server.is_some() {
            panic!("server_start() called multiple times");
        }

        let mut server = Server::new(ServerConfig::default(), self.protocol.clone());
        let server_socket = self.create_server_socket();
        server.listen(server_socket);

        self.server = Some(server);
    }

    pub fn client_start(&mut self, _display_name: &str, auth: Auth) -> ClientKey {
        if self.server.is_none() {
            panic!("server_start() must be called before client_start()");
        }

        let client_key = ClientKey::new(self.next_client_key);
        self.next_client_key += 1;

        // Create client config for tests (fast handshake, no jitter buffer)
        let mut config = ClientConfig::default();
        config.send_handshake_interval = Duration::from_millis(0);
        config.jitter_buffer = JitterBufferType::Bypass;

        let mut client = Client::new(config, self.protocol.clone());
        let world = TestWorld::default();
        let socket = self.create_client_socket();
        
        // Store auth in pending_auths for later matching with AuthEvent
        self.pending_auths.insert(client_key, auth.clone());
        
        client.auth(auth);
        client.connect(socket);

        // Insert client state without user_key (will be set when AuthEvent is processed)
        self.clients.insert(
            client_key,
            ClientState::new(client, world),
        );

        client_key
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
    #[must_use]
    pub(crate) fn client_entity_ref(
        &'_ self,
        client_key: &ClientKey,
        key: &EntityKey,
    ) -> Option<EntityRef<'_, TestEntity, WorldRef<'_>>> {

        let state = self.client_state_ref(client_key);
        let user_key = state.user_key();
        let local_entity = self.local_entity_for(key, &user_key)?;
        let world_ref = state.world().proxy();
        state.client().local_entity(world_ref, &local_entity)
    }

    /// Get client-side EntityMut by EntityKey.
    /// 
    /// Encapsulates LocalEntity lookup and EntityMut creation to avoid double-borrow issues.
    #[must_use]
    pub(crate) fn client_entity_mut(
        &'_ mut self,
        client_key: &ClientKey,
        key: &EntityKey,
    ) -> Option<EntityMut<'_, TestEntity, WorldMut<'_>>> {

        let user_key = self.client_state_ref(client_key).user_key();
        let local_entity = self.local_entity_for(key, &user_key)?;

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
    /// Update simulation without draining events (for expect() to handle events)
    fn tick_network_only(&mut self) {
        // Advance simulated clock by default tick duration
        TestClock::advance(TICK_DURATION_MS);
        let now = Instant::now();

        // Update all clients and server network
        let server = self.server.as_mut().expect("server not started");
        
        for (_client_key, state) in self.clients.iter_mut() {
            let (client, world) = state.client_and_world_mut();
            Self::update_client_server_at(
                &now,
                client,
                server,
                world,
                &mut self.server_world,
            );
        }
    }

    /// Process spawn events from collected events and update entity registry
    fn process_spawn_events(
        &mut self,
        server_events: &mut ServerEvents<TestEntity>,
        client_events_map: &mut HashMap<ClientKey, ClientEvents<TestEntity>>,
    ) {
        // Collect client spawn events
        let mut spawns_to_register = Vec::new();
        for (client_key, client_events) in client_events_map.iter_mut() {
            for spawned_entity in client_events.read::<ClientSpawnEntityEvent>() {
                let state = self.clients.get(client_key).expect("client not found");
                let world_ref = state.world().proxy();
                let client_ref = state.client().entity(world_ref, &spawned_entity);
                
                if let Some(local_entity) = client_ref.local_entity() {
                    spawns_to_register.push((*client_key, local_entity, spawned_entity));
                }
            }
        }

        // Collect server spawn events
        let server_spawn_data = Self::extract_server_spawn_events(server_events);
        
        let (server_entities_to_register, server_local_entity_mappings) = {
            let mut server_entities_to_register = Vec::new();
            let mut server_local_entity_mappings = Vec::new();
            
            for (spawn_user_key, spawn_entity) in server_spawn_data {
                if let Some(client_key) = self.client_key_for_user(&spawn_user_key) {
                    let server_local_entity = {
                        let server = self.server.as_ref().expect("server not started");
                        let world_ref = self.server_world.proxy();
                        let server_ref = server.entity(world_ref, &spawn_entity);
                        server_ref.local_entity(&spawn_user_key)
                    };
                    
                    if let Some(local_entity) = server_local_entity {
                        if let Some(entity_key) = self.entity_registry.entity_key_for_client_entity(&client_key, &local_entity) {
                            debug!("Matched client-spawned entity {:?} for client {:?}", entity_key, client_key);
                            server_entities_to_register.push((entity_key, spawn_entity));
                        } else if let Some(entity_key) = self.entity_registry.entity_key_for_server_entity(&spawn_entity) {
                            debug!("Matched server-spawned entity {:?} for client {:?}", entity_key, client_key);
                            server_local_entity_mappings.push((entity_key, client_key, local_entity));
                        } else if let Some(entity_key) = self.entity_registry.remove_pending_client_spawn(&client_key) {
                            debug!("Matched pending client-spawned entity {:?} for client {:?}", entity_key, client_key);
                            server_entities_to_register.push((entity_key, spawn_entity));
                            server_local_entity_mappings.push((entity_key, client_key, local_entity));
                        } else {
                            debug!("No EntityKey match found for server spawn event (user: {:?}, entity: {:?})", spawn_user_key, spawn_entity);
                        }
                    }
                }
            }
            
            (server_entities_to_register, server_local_entity_mappings)
        };
        
        // Register server entities before Phase D to ensure they're available for client spawn matching
        self.apply_server_entity_registrations(server_entities_to_register);
        self.apply_local_entity_mappings(server_local_entity_mappings);
        
        // Phase D: Register client spawns
        self.register_client_spawns(spawns_to_register);
    }


    /// Phase C: Extract server spawn events from already-collected events.
    fn extract_server_spawn_events(server_events: &mut ServerEvents<TestEntity>) -> Vec<(UserKey, TestEntity)> {
        let mut server_spawn_data = Vec::new();
        
        for (spawn_user_key, spawn_entity) in server_events.read::<ServerSpawnEntityEvent>() {
            server_spawn_data.push((spawn_user_key, spawn_entity));
        }
        
        server_spawn_data
    }


    /// Phase D: Register client spawns by matching LocalEntity values with server entities.
    fn register_client_spawns(&mut self, spawns_to_register: Vec<(ClientKey, LocalEntity, TestEntity)>) {
        for (client_key, local_entity, client_entity) in spawns_to_register {
            let local_entity_value = extract_local_entity_value(&local_entity);
            
            // Skip if already registered
            if let Some(existing_key) = self.entity_registry.entity_key_for_client_entity(&client_key, &local_entity) {
                debug!("Skipping already-registered client entity {:?} for client {:?}", existing_key, client_key);
                continue;
            }
            
            // Match EntityKey by LocalEntity value
            let user_key = self.user_key(&client_key);
            let (entity_key, server_entities_count) = {
                let server = self.server.as_ref().expect("server not started");
                let mut matched_key = None;
                let server_entities: Vec<_> = self.entity_registry.server_entities_iter().collect();
                let count = server_entities.len();
                
                for (ek, server_entity) in &server_entities {
                    let world_ref = self.server_world.proxy();
                    let server_ref = server.entity(world_ref, server_entity);
                    if let Some(server_local_entity) = server_ref.local_entity(&user_key) {
                        let server_value = extract_local_entity_value(&server_local_entity);
                        if server_value == local_entity_value {
                            debug!("Matched LocalEntity value {} to server entity {:?}", local_entity_value, ek);
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
                warn!(
                    "Phase D: Failed to resolve EntityKey for client {:?} with LocalEntity value {} (checked {} server entities). \
                     This may indicate a mapping lifecycle violation - entity should resolve in a future tick.",
                    client_key, local_entity_value, server_entities_count
                );
            }
        }
    }

    pub(crate) fn take_server_events(&mut self) -> ServerEvents<TestEntity> {
        self.server.as_mut().expect("server not started").take_world_events()
    }

    pub(crate) fn take_client_events(&mut self) -> HashMap<ClientKey, ClientEvents<TestEntity>> {
        let mut map = HashMap::new();
        for (client_key, client_state) in self.clients.iter_mut() {
            let client_events = client_state.client_mut().take_world_events();
            map.insert(*client_key, client_events);
        }
        map
    }

    pub(crate) fn user_key(&self, client_key: &ClientKey) -> UserKey {
        self.clients
            .get(&client_key)
            .expect("client not found")
            .user_key()
    }

    /// Register a ClientKey ↔ UserKey mapping after handshake
    /// 
    /// This is called internally when AuthEvent is processed and matched to a ClientKey.
    fn register_client_user(&mut self, client_key: ClientKey, user_key: UserKey) {
        // Update ClientState
        if let Some(state) = self.clients.get_mut(&client_key) {
            state.set_user_key(user_key);
        }
        // Update bidirectional maps
        self.client_user_map.insert(client_key, user_key);
        self.user_to_client_map.insert(user_key, client_key);
        // Remove from pending (handshake complete)
        self.pending_auths.remove(&client_key);
    }

    /// Get server host entity for an EntityKey
    #[must_use]
    pub(crate) fn server_host_entity(&self, entity_key: &EntityKey) -> Option<TestEntity> {
        self.entity_registry.server_entity(entity_key)
    }

    /// Get UserKey for a ClientKey
    #[must_use]
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
    #[must_use]
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
    #[must_use]
    pub(crate) fn client_key_for_user(&self, user_key: &UserKey) -> Option<ClientKey> {
        self.user_to_client_map.get(user_key).copied()
    }

    /// Get client_user_map for creating Users handle
    pub(crate) fn client_users(&'_ self) -> Users<'_> {
        Users::new(&self.client_user_map)
    }

    /// Perform actions in a mutate phase and propagate changes.
    /// 
    /// The closure receives a mutable context for spawning entities and modifying world state.
    /// After the closure completes, updates the network to propagate changes (like entity spawns)
    /// but does not drain events - events are only drained by `expect()`.
    pub fn mutate<R>(&mut self, f: impl FnOnce(&mut MutateCtx) -> R) -> R {
        let mut ctx = MutateCtx::new(self);
        let result = f(&mut ctx);
        // Update network to propagate immediate effects without draining events
        self.tick_network_only();
        result
    }

    /// Register expectations and wait until they all pass or timeout.
    /// 
    /// The closure is called each tick and should return `Some(T)` when expectations are met.
    /// Ticks the simulation until the closure returns `Some(value)` or the maximum tick count is reached.
    pub fn expect<T>(&mut self, mut f: impl FnMut(&mut ExpectCtx<'_>) -> Option<T>) -> T {
        for _tick_count in 1..=DEFAULT_MAX_EXPECT_TICKS {
            // Update network without draining events
            self.tick_network_only();
            
            // Collect per-tick events (EXACTLY once per tick)
            let mut server_events = self.take_server_events();
            let mut client_events_map = self.take_client_events();
            
            // Process spawn events for entity registration
            self.process_spawn_events(&mut server_events, &mut client_events_map);
            
            // Process AuthEvents: match to ClientKey and establish mapping
            let mut auth_events = Vec::new();
            for (user_key, auth) in server_events.read::<AuthEvent<Auth>>() {
                // Find ClientKey by matching Auth payload
                if let Some((&client_key, _)) = self.pending_auths.iter()
                    .find(|(_, pending_auth)| 
                        pending_auth.username == auth.username && 
                        pending_auth.password == auth.password) 
                {
                    // Establish mapping
                    self.register_client_user(client_key, user_key);
                    // Store translated event
                    auth_events.push((client_key, auth));
                }
            }
            
            // Process ConnectEvents: translate UserKey to ClientKey
            let mut connect_events = Vec::new();
            for user_key in server_events.read::<ConnectEvent>() {
                if let Some(&client_key) = self.user_to_client_map.get(&user_key) {
                    connect_events.push(client_key);
                }
            }
            
            // Create immutable ExpectCtx for this tick with translated events
            let mut ctx = ExpectCtx::new(self, server_events, client_events_map, auth_events, connect_events);
            
            // Call user closure
            if let Some(value) = f(&mut ctx) {
                return value;
            }
        }
        
        panic!(
            "Scenario::expect timed out after {} ticks without satisfying condition",
            DEFAULT_MAX_EXPECT_TICKS
        );
    }

    /// Split borrow fields needed for ServerMut
    /// Returns disjoint mutable references to server, world, registry, and users
    pub(crate) fn split_for_server_mut(
        &'_ mut self,
    ) -> (
        &mut Server,
        &'_ mut TestWorld,
        &'_ mut EntityRegistry,
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

    /// Create a client socket from the transport hub
    fn create_client_socket(&self) -> LocalClientSocket {
        let (client_addr, auth_req_tx, auth_resp_rx, client_data_tx, client_data_rx) = 
            self.hub.register_client();
        
        let addr_cell = LocalAddrCell::new();
        // For local transport, we know the server address immediately
        addr_cell.set_sync(self.hub.server_addr());

        // Each client gets its own identity token storage (not shared!)
        let identity_token = Arc::new(Mutex::new(None));
        let rejection_code = Arc::new(Mutex::new(None));

        // Use the inner socket type from the module
        let inner_socket = naia_client::transport::local::LocalClientSocket::new_with_tokens(
            client_addr,
            self.hub.server_addr(),
            auth_req_tx,
            auth_resp_rx,
            client_data_tx,
            client_data_rx,
            addr_cell,
            identity_token,
            rejection_code,
        );
        LocalClientSocket::new(inner_socket, None)
    }

    /// Create a server socket from the transport hub
    fn create_server_socket(&self) -> LocalServerSocket {
        let inner_socket = naia_server::transport::local::LocalServerSocket::new(self.hub.clone());
        LocalServerSocket::new(inner_socket, None)
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


}