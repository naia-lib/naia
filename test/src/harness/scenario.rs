use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use log::{debug, warn};

use naia_client::{
    transport::local::{LocalAddrCell, Socket as ClientSocket, LocalClientSocket},
    Client as NaiaClient,
    ClientConfig,
    EntityMut,
    EntityRef,
    TickEvents as ClientTickEvents,
    WorldEvents as ClientWorldEvents,
};
use naia_demo_world::{WorldMut, WorldRef};
use naia_server::{
    transport::local::{Socket as ServerSocket, LocalServerSocket},
    Server as NaiaServer,
    ServerConfig,
    UserKey,
};
use naia_shared::{
    transport::local::{LocalTransportHub, FAKE_SERVER_ADDR},
    Instant,
    LocalEntity,
    OwnedLocalEntity,
    Protocol,
    TestClock,
    LinkConditionerConfig,
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
use crate::harness::client_events::{ClientEvents, process_spawn_events};
use crate::harness::server_events::ServerEvents;

type Client = NaiaClient<TestEntity>;
type Server = NaiaServer<TestEntity>;

// Constants for simulation timing and retry behavior
const TICK_DURATION_MS: u64 = 16; // Default tick duration (~60 FPS)
const DEFAULT_MAX_EXPECT_TICKS: usize = 50; // Maximum ticks before expect() times out

pub struct Scenario {
    hub: LocalTransportHub,
    server: Option<Server>,
    server_world: TestWorld,

    entity_registry: EntityRegistry,
    next_client_key: u32,
    clients: HashMap<ClientKey, ClientState>,
    /// Reverse mapping: UserKey -> ClientKey (for O(1) reverse lookups)
    user_to_client_map: HashMap<UserKey, ClientKey>,
    /// Pending auth payloads for clients that have started connecting but not yet authenticated
    pending_auths: HashMap<ClientKey, Auth>,
    /// Mapping: ClientKey -> SocketAddr (for link conditioner configuration)
    client_to_addr_map: HashMap<ClientKey, SocketAddr>,
}

impl Scenario {
    pub fn new() -> Self {
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
            user_to_client_map: HashMap::new(),
            pending_auths: HashMap::new(),
            client_to_addr_map: HashMap::new(),
        }
    }

    pub fn server_start(&mut self, server_config: ServerConfig, protocol: Protocol) {
        if self.server.is_some() {
            panic!("server_start() called multiple times");
        }

        let mut server = Server::new(server_config, protocol);
        let server_socket = ServerSocket::new(LocalServerSocket::new(self.hub.clone()), None);
        server.listen(server_socket);

        self.server = Some(server);
    }

    pub fn client_start(
        &mut self,
        _display_name: &str,
        auth: Auth,
        client_config: ClientConfig,
        protocol: Protocol,
    ) -> ClientKey {
        if self.server.is_none() {
            panic!("server_start() must be called before client_start()");
        }

        let client_key = ClientKey::new(self.next_client_key);
        self.next_client_key += 1;

        let mut client = Client::new(client_config, protocol);
        let world = TestWorld::default();
        let (socket, identity_token, rejection_code, client_addr) = self.create_client_socket();
        
        // Store client address for link conditioner configuration
        self.client_to_addr_map.insert(client_key, client_addr);
        
        // Store auth in pending_auths for later matching with AuthEvent
        self.pending_auths.insert(client_key, auth.clone());
        
        client.auth(auth);
        client.connect(socket);

        // Insert client state without user_key (will be set when AuthEvent is processed)
        self.clients.insert(
            client_key,
            ClientState::new(client, world, identity_token, rejection_code),
        );

        client_key
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
        self.tick();
        result
    }

    /// Create a context for expectations with a custom tick timeout.
    ///
    /// # Example
    /// ```rust,no_run
    /// // Wait up to 200 ticks for a condition that may take longer
    /// scenario.until(200.ticks()).expect(|ctx| {
    ///     // ... check conditions
    /// });
    /// ```
    pub fn until(&mut self, ticks: crate::harness::Ticks) -> crate::harness::UntilCtx<'_> {
        crate::harness::UntilCtx::new(self, ticks.0)
    }

    /// Register expectations and wait until they all pass or timeout.
    ///
    /// The closure is called each tick and should return `Some(T)` when expectations are met.
    /// Ticks the simulation until the closure returns `Some(value)` or the maximum tick count is reached.
    pub fn expect<T>(&mut self, f: impl FnMut(&mut ExpectCtx<'_>) -> Option<T>) -> T {
        self.expect_with_ticks_internal(DEFAULT_MAX_EXPECT_TICKS, f)
    }

    /// Internal method for expectations with a custom tick limit.
    /// Use `scenario.until(ticks).expect(...)` instead.
    pub(crate) fn expect_with_ticks_internal<T>(&mut self, max_ticks: usize, mut f: impl FnMut(&mut ExpectCtx<'_>) -> Option<T>) -> T {
        for _tick_count in 1..=max_ticks {
            // Update network without draining events
            self.tick();

            // Collect per-tick events (EXACTLY once per tick)
            let mut server_events = self.take_server_events();
            let mut client_events_map = self.take_client_events();

            // Process spawn events to match client-spawned entities with server entities
            crate::harness::client_events::process_spawn_events(self, &mut server_events, &mut client_events_map);

            // Create immutable ExpectCtx for this tick with translated events
            let mut ctx = ExpectCtx::new(self, server_events, client_events_map);

            // Call user closure
            if let Some(value) = f(&mut ctx) {
                return value;
            }
        }

        panic!(
            "Scenario::expect timed out after {} ticks without satisfying condition",
            max_ticks
        );
    }

    /// Get read-only access to entity registry
    pub(crate) fn entity_registry(&self) -> &EntityRegistry {
        &self.entity_registry
    }

    pub(crate) fn entity_registry_mut(&mut self) -> &mut EntityRegistry {
        &mut self.entity_registry
    }

    pub(crate) fn server(&self) -> &Option<Server> {
        &self.server
    }

    pub(crate) fn server_world(&self) -> &TestWorld {
        &self.server_world
    }

    pub(crate) fn clients_mut(&mut self) -> &mut HashMap<ClientKey, ClientState> {
        &mut self.clients
    }

    pub(crate) fn user_to_client_map_mut(&mut self) -> &mut HashMap<UserKey, ClientKey> {
        &mut self.user_to_client_map
    }

    pub(crate) fn pending_auths(&self) -> &HashMap<ClientKey, Auth> {
        &self.pending_auths
    }

    pub(crate) fn pending_auths_mut(&mut self) -> &mut HashMap<ClientKey, Auth> {
        &mut self.pending_auths
    }

    pub(crate) fn clients(&self) -> &HashMap<ClientKey, ClientState> {
        &self.clients
    }

    /// Get immutable access to server and registry for expect operations
    pub(crate) fn server_and_registry(&self) -> Option<(&Server, &EntityRegistry)> {
        Some((self.server.as_ref()?, &self.entity_registry))
    }

    /// Get ClientKey for a UserKey (reverse lookup).
    ///
    /// Uses reverse map for O(1) lookup instead of iterating clients.
    #[must_use]
    pub(crate) fn user_to_client_key(&self, user_key: &UserKey) -> Option<ClientKey> {
        self.user_to_client_map.get(user_key).copied()
    }

    pub(crate) fn client_to_user_key(&self, client_key: &ClientKey) -> Option<UserKey> {
        self.clients.get(&client_key)?.user_key()
    }

    /// Get client_user_map for creating Users handle
    pub(crate) fn client_users(&'_ self) -> Users<'_> {
        Users::new(&self.clients, &self.user_to_client_map)
    }

    /// Get read-only access to server world
    pub(crate) fn server_world_ref(&self) -> WorldRef<'_> {
        self.server_world.proxy()
    }

    /// Get read-only access to client state
    pub(crate) fn client_state(&self, client_key: &ClientKey) -> &ClientState {
        self.clients.get(client_key).expect("client not found")
    }

    pub(crate) fn client_state_mut(&mut self, client_key: &ClientKey) -> &mut ClientState {
        self.clients.get_mut(&client_key).expect("client not found")
    }

    /// Pause all network traffic (drop all packets)
    /// 
    /// This is useful for testing timeout behavior. The TestClock will continue
    /// to advance via `tick()`, but no packets will be delivered.
    /// 
    /// # Examples
    /// 
    /// ```rust,ignore
    /// // Pause traffic to test timeout
    /// scenario.pause_traffic();
    /// // ... wait for timeout ...
    /// scenario.resume_traffic();
    /// ```
    pub fn pause_traffic(&mut self) {
        self.hub.pause_traffic();
    }

    /// Resume normal network traffic delivery
    pub fn resume_traffic(&mut self) {
        self.hub.resume_traffic();
    }

    /// Check if traffic is currently paused
    pub fn is_traffic_paused(&self) -> bool {
        self.hub.is_traffic_paused()
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

        let state = self.client_state(client_key);
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

        let user_key = self.client_state(client_key).user_key()?;
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

    /// Get LocalEntity for an EntityKey and UserKey.
    ///
    /// Uses EntityRegistry as source of truth: checks client entities first, then falls back to server lookup.
    #[must_use]
    pub(crate) fn local_entity_for(&self, entity_key: &EntityKey, user_key: &UserKey) -> Option<LocalEntity> {
        let client_key = self.user_to_client_key(user_key)?;

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
    fn tick(&mut self) {
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


    pub(crate) fn take_server_events(&mut self) -> ServerEvents {
        let server = self.server.as_mut().expect("server not started");
        let now = Instant::now();
        let mut events = server.take_world_events();
        let tick_events = server.take_tick_events(&now);
        let auths = events.take_auths();
        
        ServerEvents::new(self, auths, tick_events, events)
    }

    pub(crate) fn take_client_events(&mut self) -> HashMap<ClientKey, ClientEvents> {
        let mut map = HashMap::new();
        let now = Instant::now();
        // Collect events first to avoid borrow conflicts
        let mut events_data: Vec<(ClientKey, ClientWorldEvents<TestEntity>, ClientTickEvents)> = Vec::new();
        for (client_key, client_state) in self.clients.iter_mut() {
            let client = client_state.client_mut();
            let tick_events = client.take_tick_events(&now);
            let world_events = client.take_world_events();
            events_data.push((*client_key, world_events, tick_events));
        }
        // Now process events (no longer borrowing self.clients)
        for (client_key, world_events, tick_events) in events_data {
            let client_events = ClientEvents::new(self, client_key, world_events, tick_events);
            map.insert(client_key, client_events);
        }
        map
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
        let users = Users::new(&self.clients, &self.user_to_client_map);
        (server, world, registry, users)
    }

    /// Create a client socket from the transport hub
    /// Returns the socket along with handles to identity_token, rejection_code, and client_addr
    fn create_client_socket(&self) -> (ClientSocket, Arc<Mutex<Option<naia_shared::IdentityToken>>>, Arc<Mutex<Option<u16>>>, SocketAddr) {
        let (client_addr, auth_req_tx, auth_resp_rx, client_data_tx, client_data_rx) = 
            self.hub.register_client();
        
        let addr_cell = LocalAddrCell::new();
        // For local transport, we know the server address immediately
        addr_cell.set_sync(self.hub.server_addr());

        // Each client gets its own identity token storage (not shared!)
        let identity_token = Arc::new(Mutex::new(None));
        let rejection_code = Arc::new(Mutex::new(None));

        // Use the inner socket type from the module
        let inner_socket = LocalClientSocket::new_with_tokens(
            client_addr,
            self.hub.server_addr(),
            auth_req_tx,
            auth_resp_rx,
            client_data_tx,
            client_data_rx,
            addr_cell,
            identity_token.clone(),
            rejection_code.clone(),
        );
        let socket = ClientSocket::new(inner_socket, None);
        (socket, identity_token, rejection_code, client_addr)
    }
    
    /// Configure link conditioner for a specific client
    /// `client_to_server` applies to packets from client to server (loss, jitter, latency)
    /// `server_to_client` applies to packets from server to client (loss, jitter, latency)
    /// Pass `None` to disable conditioning for that direction (perfect connection)
    pub fn configure_link_conditioner(
        &self,
        client_key: &ClientKey,
        client_to_server: Option<LinkConditionerConfig>,
        server_to_client: Option<LinkConditionerConfig>,
    ) -> bool {
        if let Some(client_addr) = self.client_to_addr_map.get(client_key) {
            self.hub.configure_link_conditioner(client_addr, client_to_server, server_to_client)
        } else {
            false
        }
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