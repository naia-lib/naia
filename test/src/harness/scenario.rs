use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Duration,
};

use log::{debug, info, warn};

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
    DisconnectEvent,
    ErrorEvent,
    Events as ServerEvents,
    NaiaServerError,
    RoomKey,
    Server as NaiaServer,
    ServerConfig,
    SpawnEntityEvent as ServerSpawnEntityEvent,
    UserKey,
};
use naia_shared::{
    transport::local::{LocalTransportHub, FAKE_SERVER_ADDR},
    IdentityToken,
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
const HANDSHAKE_MAX_ATTEMPTS: usize = 100; // Maximum attempts for client handshake
const HANDSHAKE_TICK_ADVANCE_MS: u64 = 16; // Time advance per handshake attempt

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

/// Per-tick snapshot of server connection/auth events for test inspection
#[derive(Default)]
pub struct ServerTickEvents {
    pub auths: Vec<(UserKey, Auth)>,
    pub connects: Vec<UserKey>,
    pub disconnects: Vec<UserKey>,
    pub errors: Vec<NaiaServerError>,
}

/// Identity token and rejection code storage for a client
struct ClientIdentityData {
    identity_token: Arc<Mutex<Option<IdentityToken>>>,
    rejection_code: Arc<Mutex<Option<u16>>>,
}

pub struct Scenario {
    hub: LocalTransportHub,
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
    /// Per-tick snapshot of server connection/auth events
    last_server_tick_events: ServerTickEvents,
    /// Identity token and rejection code storage per client
    client_identity_data: HashMap<ClientKey, ClientIdentityData>,
    /// Current tick count
    tick_count: u64,
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
            main_room: None,
            clients: HashMap::new(),
            entity_registry: EntityRegistry::new(),
            next_client_key: 1,
            protocol,
            client_user_map: HashMap::new(),
            user_to_client_map: HashMap::new(),
            last_server_tick_events: ServerTickEvents::default(),
            client_identity_data: HashMap::new(),
            tick_count: 0,
        }
    }

    pub fn server_start(&mut self) {
        if self.server.is_some() {
            panic!("server_start() called multiple times");
        }

        let mut server = Server::new(ServerConfig::default(), self.protocol.clone());
        let server_socket = self.create_server_socket();
        server.listen(server_socket);
        let main_room = server.make_room().key();

        self.server = Some(server);
        self.main_room = Some(main_room);
    }

    /// Begin a connection without completing handshake (low-level API)
    pub fn client_begin_connect(&mut self, _display_name: &str, auth: Auth) -> ClientKey {
        if self.server.is_none() {
            panic!("server_start() must be called before client_begin_connect()");
        }

        let client_key = ClientKey::new(self.next_client_key);
        self.next_client_key += 1;

        // Create client config for tests (fast handshake, no jitter buffer)
        let mut config = ClientConfig::default();
        config.send_handshake_interval = Duration::from_millis(0);
        config.jitter_buffer = JitterBufferType::Bypass;

        let mut client = Client::new(config, self.protocol.clone());
        let world = TestWorld::default();
        let (socket, identity_data) = self.create_client_socket(client_key);
        client.auth(auth);
        client.connect(socket);

        // Store identity data
        self.client_identity_data.insert(client_key, identity_data);

        // Create client state without user_key (will be set after handshake)
        self.clients.insert(
            client_key,
            ClientState::new(client, world, None),
        );

        client_key
    }

    /// Complete connection with handshake and room join (high-level API)
    pub fn client_connect(&mut self, display_name: &str, auth: Auth) -> ClientKey {
        let client_key = self.client_begin_connect(display_name, auth);
        let user_key = self.advance_handshake_until_connected(
            client_key,
            HANDSHAKE_MAX_ATTEMPTS,
        )
        .expect("client should connect");

        // Update client state with user_key
        if let Some(state) = self.clients.get_mut(&client_key) {
            state.set_user_key(user_key);
        }

        // Update bidirectional client-user mappings
        self.client_user_map.insert(client_key, user_key);
        self.user_to_client_map.insert(user_key, client_key);

        client_key
    }

    pub fn main_room_key(&self) -> Option<&RoomKey> {
        self.main_room.as_ref()
    }

    /// Get the per-tick server event snapshot
    pub fn server_tick_events(&self) -> &ServerTickEvents {
        &self.last_server_tick_events
    }

    /// Get list of currently connected users
    pub fn connected_users(&self) -> Vec<UserKey> {
        self.user_to_client_map.keys().copied().collect()
    }

    /// Get current tick count
    pub fn tick_index(&self) -> u64 {
        self.tick_count
    }

    /// Get identity token for a client
    pub fn client_identity_token(&self, client_key: &ClientKey) -> Option<IdentityToken> {
        self.client_identity_data
            .get(client_key)
            .and_then(|data| data.identity_token.lock().ok())
            .and_then(|guard| guard.clone())
    }

    /// Get rejection code for a client
    pub fn client_rejection_code(&self, client_key: &ClientKey) -> Option<u16> {
        self.client_identity_data
            .get(client_key)
            .and_then(|data| data.rejection_code.lock().ok())
            .and_then(|guard| *guard)
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
        let user_key = state.user_key()?;
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

        let user_key = self.client_state_ref(client_key).user_key()?;
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
    pub(crate) fn tick_once(&mut self) {
        // Advance simulated clock by default tick duration
        TestClock::advance(TICK_DURATION_MS);
        let now = Instant::now();
        self.tick_count += 1;

        // Phase A & B: Update all clients and collect client spawn events
        let spawns_to_register = self.update_all_clients_and_collect_spawns(&now);

        // Phase C: Collect and process server events (single call to take_world_events)
        let (server_spawn_data, server_tick_events) = self.collect_server_events();
        self.last_server_tick_events = server_tick_events;
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

    /// Phase A & B: Update all clients and server, collect client spawn events.
    fn update_all_clients_and_collect_spawns(&mut self, now: &Instant) -> Vec<(ClientKey, LocalEntity, TestEntity)> {
        let mut spawns_to_register = Vec::new();
        let server = self.server.as_mut().expect("server not started");
        
        for (client_key, state) in self.clients.iter_mut() {
            let (client, world) = state.client_and_world_mut();
            Self::update_client_server_at(
                now,
                client,
                server,
                world,
                &mut self.server_world,
            );

            // Collect client spawn events
            let mut client_events = state.client_mut().take_world_events();
            for spawned_entity in client_events.read::<ClientSpawnEntityEvent>() {
                let world_ref = state.world().proxy();
                let client_ref = state.client().entity(world_ref, &spawned_entity);
                
                if let Some(local_entity) = client_ref.local_entity() {
                    spawns_to_register.push((*client_key, local_entity, spawned_entity));
                }
            }
        }
        
        spawns_to_register
    }

    /// Phase C: Collect all server events (must be called exactly once per tick).
    /// Returns both spawn events (for entity registry) and tick events (for test inspection).
    fn collect_server_events(&mut self) -> (Vec<(UserKey, TestEntity)>, ServerTickEvents) {
        let server = self.server.as_mut().expect("server not started");
        let mut server_events = server.take_world_events();
        
        // Drain connection/auth events into snapshot
        let mut tick_events = ServerTickEvents::default();
        for (user_key, auth) in server_events.read::<AuthEvent<Auth>>() {
            tick_events.auths.push((user_key, auth));
        }
        for user_key in server_events.read::<ConnectEvent>() {
            tick_events.connects.push(user_key);
        }
        for (user_key, _) in server_events.read::<DisconnectEvent>() {
            tick_events.disconnects.push(user_key);
        }
        for error in server_events.read::<ErrorEvent>() {
            tick_events.errors.push(error);
        }
        
        // Drain spawn events for entity registry
        let mut server_spawn_data = Vec::new();
        for (spawn_user_key, spawn_entity) in server_events.read::<ServerSpawnEntityEvent>() {
            server_spawn_data.push((spawn_user_key, spawn_entity));
        }
        
        (server_spawn_data, tick_events)
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
            let user_key = match self.user_key(&client_key) {
                Some(uk) => uk,
                None => {
                    warn!("Phase D: Client {:?} has no user_key, skipping entity registration", client_key);
                    continue;
                }
            };
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

    /// Deprecated: Use `server_tick_events()` instead for accessing server events.
    /// This method should not be called as it would drain events that have already been processed in `tick_once()`.
    #[deprecated(note = "Use server_tick_events() instead. Events are now drained centrally in tick_once().")]
    pub(crate) fn take_server_events(&mut self) -> ServerEvents<TestEntity> {
        // This will cause double-draining if called, but kept for backwards compatibility
        // In practice, this should not be called as events are now in server_tick_events()
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

    pub(crate) fn user_key(&self, client_key: &ClientKey) -> Option<UserKey> {
        self.clients
            .get(&client_key)
            .and_then(|state| state.user_key())
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

    /// Perform actions in a mutate phase and tick the simulation once.
    /// 
    /// The closure receives a mutable context for spawning entities and modifying world state.
    /// After the closure completes, the simulation is ticked once to propagate changes.
    pub fn mutate<R>(&mut self, f: impl FnOnce(&mut MutateCtx) -> R) -> R {
        let mut ctx = MutateCtx::new(self);
        let result = f(&mut ctx);
        // Tick at least once after actions to propagate immediate effects
        self.tick_once();
        result
    }

    /// Register expectations and wait until they all pass or timeout.
    /// 
    /// The closure is called each tick and should return `true` when all expectations are met.
    /// Ticks the simulation until the closure returns `true` or the maximum tick count is reached.
    pub fn expect(&mut self, f: impl FnMut(&ExpectCtx) -> bool) {
        let mut ctx = ExpectCtx::new(self, DEFAULT_MAX_EXPECT_TICKS);
        ctx.run(f);
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
    /// Returns both the socket and the identity data handles for storage
    fn create_client_socket(&mut self, _client_key: ClientKey) -> (LocalClientSocket, ClientIdentityData) {
        let (client_addr, auth_req_tx, auth_resp_rx, client_data_tx, client_data_rx) = 
            self.hub.register_client();
        
        let addr_cell = LocalAddrCell::new();
        // For local transport, we know the server address immediately
        addr_cell.set_sync(self.hub.server_addr());

        // Each client gets its own identity token storage (not shared!)
        let identity_token = Arc::new(Mutex::new(None));
        let rejection_code = Arc::new(Mutex::new(None));

        let identity_data = ClientIdentityData {
            identity_token: identity_token.clone(),
            rejection_code: rejection_code.clone(),
        };

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
        let socket = LocalClientSocket::new(inner_socket, None);
        (socket, identity_data)
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

    /// Advance handshake until client is fully connected
    /// Uses tick_once() to drive the simulation and checks ServerTickEvents for completion
    fn advance_handshake_until_connected(
        &mut self,
        client_key: ClientKey,
        timeout_ticks: usize,
    ) -> Option<UserKey> {
        let main_room = *self.main_room.as_ref().unwrap();
        let mut user_key_opt = None;
        let mut user_added_to_room = false;

        for _attempt in 1..=timeout_ticks {
            // Drive simulation via tick_once
            self.tick_once();

            // Check for auth events and accept connection
            for (auth_user_key, _auth) in &self.last_server_tick_events.auths {
                if user_key_opt.is_none() {
                    info!("Server accepting connection for client {:?}: {:?}", client_key, auth_user_key);
                    let server = self.server.as_mut().expect("server not started");
                    server.accept_connection(auth_user_key);
                    user_key_opt = Some(*auth_user_key);
                }
            }

            // Add user to room once they exist
            if let Some(user_key) = user_key_opt {
                if !user_added_to_room {
                    let server = self.server.as_mut().expect("server not started");
                    if server.user_exists(&user_key) {
                        server.room_mut(&main_room).add_user(&user_key);
                        user_added_to_room = true;
                    }
                }
            }

            // Check if client has seen ConnectEvent by checking connection status
            let client_connected = {
                let state = self.clients.get(&client_key)?;
                state.client().connection_status().is_connected()
            };

            // Check if server has emitted ConnectEvent for this user
            let server_connected = user_key_opt
                .map(|uk| self.last_server_tick_events.connects.contains(&uk))
                .unwrap_or(false);

            if client_connected && server_connected {
                return user_key_opt;
            }
        }

        warn!("Handshake failed for client {:?} after {} ticks", client_key, timeout_ticks);
        None
    }

}