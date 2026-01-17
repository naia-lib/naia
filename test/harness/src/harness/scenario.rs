use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, Mutex},
};


use naia_demo_world::{WorldMut, WorldRef};
use naia_shared::{
    transport::local::{LocalTransportHub, FAKE_SERVER_ADDR},
    Instant, LinkConditionerConfig, LocalEntity, Protocol, TestClock, WorldRefType,
};
use naia_server::{
    transport::local::{LocalServerSocket, Socket as ServerSocket},
    Server as NaiaServer, ServerConfig, UserKey, RoomKey,
};
use naia_client::{
    transport::local::{LocalAddrCell, LocalClientSocket, Socket as ClientSocket},
    Client as NaiaClient, ClientConfig, TickEvents as ClientTickEvents,
    WorldEvents as ClientWorldEvents,
};

use crate::{
    harness::{
        client_events::{process_spawn_events, ClientEvents},
        client_state::ClientState, entity_registry::EntityRegistry, mutate_ctx::MutateCtx,
        users::Users, ClientKey, EntityKey, ExpectCtx, server_events::ServerEvents, ClientEntityRef
    },
    Auth, TestEntity, TestWorld,
};
use crate::harness::ClientEntityMut;

type Client = NaiaClient<TestEntity>;
type Server = NaiaServer<TestEntity>;

// Constants for simulation timing and retry behavior
const TICK_DURATION_MS: u64 = 16; // Default tick duration (~60 FPS)
const DEFAULT_MAX_EXPECT_TICKS: usize = 100; // Maximum ticks before expect() times out

/// Tracks the last operation type to enforce alternating mutate/expect calls
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LastOperation {
    None,
    Mutate,
    Expect,
}

/// Tracked server-side events for ordering assertions in BDD tests.
/// These represent observable events at the Naia protocol level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackedServerEvent {
    /// Server received an auth request
    Auth,
    /// Server established a connection (handshake complete)
    Connect,
    /// Server session ended
    Disconnect,
}

/// Tracked client-side events for ordering assertions in BDD tests.
/// These represent observable events at the Naia protocol level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackedClientEvent {
    /// Client connected successfully
    Connect,
    /// Client disconnected after being connected
    Disconnect,
    /// Client was rejected (never connected)
    Reject,
}

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
    /// Tracks the last operation to enforce alternating mutate/expect calls
    last_operation: LastOperation,
    /// Current tick count (incremented on each tick)
    global_tick: usize,
    /// Tracked server events in order of occurrence (for BDD ordering assertions)
    server_event_history: Vec<TrackedServerEvent>,
    /// Tracked client events per client in order of occurrence (for BDD ordering assertions)
    client_event_history: HashMap<ClientKey, Vec<TrackedClientEvent>>,
    /// Last client key started (convenience for single-client BDD tests)
    last_client_key: Option<ClientKey>,
    /// Last room key created (convenience for BDD tests)
    last_room_key: Option<RoomKey>,
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
            last_operation: LastOperation::None,
            global_tick: 0,
            server_event_history: Vec::new(),
            client_event_history: HashMap::new(),
            last_client_key: None,
            last_room_key: None,
        }
    }

    /// Access the server immutably.
    pub fn server(&self) -> Option<&Server> {
        self.server.as_ref()
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
        // Allow this to be called after either mutate() or expect()
        // This is a setup operation, not a mutate or expect, so it should be flexible
        self.allow_flexible_next();

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

        // Store as last client for convenience in single-client BDD tests
        self.last_client_key = Some(client_key);

        client_key
    }

    /// Perform actions in a mutate phase and propagate changes.
    ///
    /// The closure receives a mutable context for spawning entities and modifying world state.
    /// After the closure completes, updates the network to propagate changes (like entity spawns)
    /// but does not drain events - events are only drained by `expect()`.
    ///
    /// # Panics
    ///
    /// Panics if called immediately after another `mutate()` call. Tests MUST alternate
    /// between `mutate()` and `expect()` calls.
    pub fn mutate<R>(&mut self, f: impl FnOnce(&mut MutateCtx) -> R) -> R {
        if self.last_operation == LastOperation::Mutate {
            panic!("Scenario::mutate() called immediately after another mutate() call. Tests MUST alternate between mutate() and expect() calls.");
        }

        let mut ctx = MutateCtx::new(self);
        let result = f(&mut ctx);
        // Update network to propagate immediate effects without draining events
        self.tick();
        self.last_operation = LastOperation::Mutate;
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
    ///
    /// # Panics
    ///
    /// Panics if called immediately after another `expect()` call. Tests MUST alternate
    /// between `mutate()` and `expect()` calls.
    pub fn expect<T>(&mut self, f: impl FnMut(&mut ExpectCtx<'_>) -> Option<T>) -> T {
        self.expect_with_ticks_internal(DEFAULT_MAX_EXPECT_TICKS, f)
    }

    pub fn expect_msg<T>(
        &mut self,
        msg: &str,
        f: impl FnMut(&mut ExpectCtx<'_>) -> Option<T>,
    ) -> T {
        self.expect_with_ticks_internal_msg(DEFAULT_MAX_EXPECT_TICKS, msg, f)
    }

    /// TODO: THIS IS ABSOLUTELY HORRIBLE. FIX THIS! This should ONLY happen within a mutate block!
    /// Inject a raw packet from a client to the server (for testing malformed data)
    pub fn inject_client_packet(&mut self, client_key: &ClientKey, data: Vec<u8>) -> bool {
        if let Some(addr) = self.client_to_addr_map.get(client_key) {
            return self.hub.inject_client_packet(addr, data);
        }
        false
    }

    /// Register a labeled expectation for spec obligation tracing.
    ///
    /// This is the primary API for assertions that verify spec contract obligations.
    /// Labels should follow the format: `<contract-id>.tN: <description>` for obligations,
    /// or `<contract-id>: <description>` for contract-level assertions.
    ///
    /// The closure is called each tick and should return `Some(T)` when expectations are met.
    /// Ticks the simulation until the closure returns `Some(value)` or the maximum tick count is reached.
    ///
    /// # Example
    ///
    /// ```ignore
    /// scenario.spec_expect("messaging-15-a.t2: boundary tick is accepted", |ctx| {
    ///     ctx.client(key, |c| c.has_message()).then_some(())
    /// });
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if called immediately after another `expect()` call. Tests MUST alternate
    /// between `mutate()` and `expect()` calls.
    pub fn spec_expect<T>(
        &mut self,
        label: impl AsRef<str>,
        f: impl FnMut(&mut ExpectCtx<'_>) -> Option<T>,
    ) -> T {
        self.expect_with_ticks_internal_msg(DEFAULT_MAX_EXPECT_TICKS, label.as_ref(), f)
    }

    /// Internal method for expectations with a custom tick limit.
    /// Use `scenario.until(ticks).expect(...)` instead.
    ///
    /// # Panics
    ///
    /// Panics if called immediately after another `expect()` call. Tests MUST alternate
    /// between `mutate()` and `expect()` calls.
    pub(crate) fn expect_with_ticks_internal<T>(
        &mut self,
        max_ticks: usize,
        mut f: impl FnMut(&mut ExpectCtx<'_>) -> Option<T>,
    ) -> T {
        if self.last_operation == LastOperation::Expect {
            panic!("Scenario::expect() called immediately after another expect() call. Tests MUST alternate between mutate() and expect() calls.");
        }

        let result = (|| {
            for _tick_count in 1..=max_ticks {
                // Update network without draining events
                self.tick();

                // Collect per-tick events (EXACTLY once per tick)
                let mut server_events = self.take_server_events();
                let mut client_events_map = self.take_client_events();

                // Process spawn events to match client-spawned entities with server entities
                process_spawn_events(self, &mut server_events, &mut client_events_map);

                // Process despawn events to unregister entities from the registry
                crate::harness::client_events::process_despawn_events(
                    self,
                    &mut server_events,
                    &mut client_events_map,
                );

                // Create immutable ExpectCtx for this tick with translated events
                let mut ctx = ExpectCtx::new(self, server_events, client_events_map);

                // Call user closure
                let result = f(&mut ctx);
                if let Some(value) = result {
                    return Some(value);
                }
            }
            None
        })();

        // Update last operation after expect completes (whether success or timeout)
        self.last_operation = LastOperation::Expect;

        match result {
            Some(value) => value,
            None => {
                panic!(
                    "Scenario::expect timed out after {} ticks without satisfying condition",
                    max_ticks
                );
            }
        }
    }

    pub(crate) fn expect_with_ticks_internal_msg<T>(
        &mut self,
        max_ticks: usize,
        msg: &str,
        mut f: impl FnMut(&mut ExpectCtx<'_>) -> Option<T>,
    ) -> T {
        if self.last_operation == LastOperation::Expect {
            panic!("Scenario::expect() called immediately after another expect() call. Tests MUST alternate between mutate() and expect() calls.");
        }

        let result = (|| {
            for _tick_count in 1..=max_ticks {
                // Update network without draining events
                self.tick();

                // Collect per-tick events (EXACTLY once per tick)
                let mut server_events = self.take_server_events();
                let mut client_events_map = self.take_client_events();

                // Process spawn events to match client-spawned entities with server entities
                process_spawn_events(self, &mut server_events, &mut client_events_map);

                // Process despawn events to unregister entities from the registry
                crate::harness::client_events::process_despawn_events(
                    self,
                    &mut server_events,
                    &mut client_events_map,
                );

                // Create immutable ExpectCtx for this tick with translated events
                let mut ctx = ExpectCtx::new(self, server_events, client_events_map);

                // Call user closure
                let result = f(&mut ctx);
                if let Some(value) = result {
                    return Some(value);
                }
            }
            None
        })();

        // Update last operation after expect completes (whether success or timeout)
        self.last_operation = LastOperation::Expect;

        match result {
            Some(value) => value,
            None => {
                panic!(
                    "Scenario::expect timed out after {} ticks: {}",
                    max_ticks, msg
                );
            }
        }
    }

    /// Reset the operation state to allow the next call to be either `mutate()` or `expect()`.
    ///
    /// This is useful for helper functions like `client_connect()` that perform multiple
    /// operations internally and should allow the caller to follow with either type of operation.
    pub fn allow_flexible_next(&mut self) {
        self.last_operation = LastOperation::None;
    }

    /// Get read-only access to entity registry
    pub(crate) fn entity_registry(&self) -> &EntityRegistry {
        &self.entity_registry
    }

    pub(crate) fn entity_registry_mut(&mut self) -> &mut EntityRegistry {
        &mut self.entity_registry
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

    /// Check if a client is connected.
    pub fn client_is_connected(&self, client_key: ClientKey) -> bool {
        self.clients
            .get(&client_key)
            .map(|state| state.client().connection_status().is_connected())
            .unwrap_or(false)
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

    #[cfg(feature = "e2e_debug")]
    /// Debug helper to dump entity identity state for troubleshooting test failures
    pub fn debug_dump_identity_state(
        &self,
        label: &str,
        entity_key: &EntityKey,
        client_keys: &[ClientKey],
    ) {
        use crate::test_protocol::Position;

        eprintln!("=== Identity State Dump: {} ===", label);
        eprintln!("EntityKey: {:?}", entity_key);

        // Server state
        if let Some(server_entity) = self.entity_registry.server_entity(entity_key) {
            eprintln!("Server: has entity={:?}", server_entity);
            if let Some(server) = &self.server {
                let world_ref = self.server_world.proxy();
                if world_ref.has_entity(&server_entity) {
                    let server_ref = server.entity(world_ref, &server_entity);
                    let pos_value = server_ref.component::<Position>().map(|p| (*p.x, *p.y));
                    if let Some((x, y)) = pos_value {
                        eprintln!("Server Position: ({}, {})", x, y);
                    } else {
                        eprintln!("Server Position: missing");
                    }
                } else {
                    eprintln!("Server: entity not in world");
                }
            }
        } else {
            eprintln!("Server: entity not registered");
        }

        // Per-client state
        for client_key in client_keys {
            let client_state = match self.clients.get(client_key) {
                Some(s) => s,
                None => {
                    eprintln!("Client {:?}: not found", client_key);
                    continue;
                }
            };

            let world_ref = client_state.world().proxy();
            let client_entity = self.entity_registry.client_entity(entity_key, client_key);

            eprintln!("Client {:?}:", client_key);
            eprintln!("  Registered client entity: {:?}", client_entity);

            if let Some(ce) = client_entity {
                if world_ref.has_entity(&ce) {
                    eprintln!("  Has entity in world: true");
                    let client_ref = client_state.client().entity(world_ref, &ce);
                    if let Some(local_entity) = client_ref.local_entity() {
                        eprintln!("  LocalEntity: {:?}", local_entity);
                    }
                    let pos_value = client_ref.component::<Position>().map(|p| (*p.x, *p.y));
                    if let Some((x, y)) = pos_value {
                        eprintln!("  Position: ({}, {})", x, y);
                    } else {
                        eprintln!("  Position: missing");
                    }
                } else {
                    eprintln!("  Has entity in world: false");
                }
            } else {
                eprintln!("  Registered client entity: None");
            }
        }
        eprintln!("=== End Dump ===\n");
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
    ) -> Option<ClientEntityRef<'_, WorldRef<'_>>> {
        let state = self.client_state(client_key);
        let user_key = match state.user_key() {
            Some(uk) => uk,
            None => return None,
        };
        let local_entity = match self.local_entity_for(key, &user_key) {
            Some(le) => le,
            None => return None,
        };
        let world_ref = state.world().proxy();
        let entity_ref = state.client().local_entity(world_ref, &local_entity)?;
        let registry = self.entity_registry();
        Some(ClientEntityRef::new(entity_ref, registry, *client_key))
    }

    /// Get client-side EntityMut by EntityKey.
    ///
    /// Encapsulates LocalEntity lookup and EntityMut creation to avoid double-borrow issues.
    #[must_use]
    pub(crate) fn client_entity_mut(
        &'_ mut self,
        client_key: &ClientKey,
        key: &EntityKey,
    ) -> Option<ClientEntityMut<'_, WorldMut<'_>>> {
        let user_key = self.client_state(client_key).user_key()?;
        let local_entity = self.local_entity_for(key, &user_key)?;
        let state = self.clients.get_mut(client_key)?;
        let registry = &mut self.entity_registry;
        // Derive entity ID in tight scope to drop borrows before getting mutable references
        let entity = {
            let world_ref = state.world().proxy();
            let client_ref = state.client().local_entity(world_ref, &local_entity)?;
            client_ref.id()
        };

        let (client_mut, world_mut) = state.client_and_world_mut();
        let world_mut_proxy = world_mut.proxy_mut();
        let entity_mut = client_mut.entity_mut(world_mut_proxy, &entity);
        // Reborrow registry as immutable for ClientEntityMut::new
        Some(ClientEntityMut::new(entity_mut, &*registry, *client_key))
    }

    /// Get LocalEntity for an EntityKey and UserKey.
    ///
    /// Uses EntityRegistry as source of truth: checks client entities first, then falls back to server lookup.
    #[must_use]
    pub(crate) fn local_entity_for(
        &self,
        entity_key: &EntityKey,
        user_key: &UserKey,
    ) -> Option<LocalEntity> {
        let client_key = self.user_to_client_key(user_key)?;

        // Try client entity first (if registered)
        if let Some(client_entity) = self.entity_registry.client_entity(entity_key, &client_key) {
            let state = self.clients.get(&client_key)?;
            let world_ref = state.world().proxy();
            if world_ref.has_entity(&client_entity) {
                let client_ref = state.client().entity(world_ref, &client_entity);
                if let Some(local_entity) = client_ref.local_entity() {
                    return Some(local_entity);
                }
            }
        }

        // Fallback: server's perspective (returns None if entity hasn't replicated to this user yet)
        let server_entity = self.entity_registry.server_entity(entity_key)?;
        let server = self.server.as_ref()?;
        let world_proxy = self.server_world.proxy();
        if !world_proxy.has_entity(&server_entity) {
            return None;
        }
        let server_ref = server.entity(world_proxy, &server_entity);
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
        // Increment tick counter
        self.global_tick += 1;

        // Advance simulated clock by default tick duration
        TestClock::advance(TICK_DURATION_MS);
        let now = Instant::now();
        log::trace!("[SCENARIO] tick: Time advanced");

        // Process time queues to deliver any ready delayed packets
        // This is critical for link conditioner tests where packets are queued
        // with future timestamps and need to be delivered even when there's
        // no new traffic. Without this, queued packets would only be delivered
        // when send_data() or try_recv_data() is called, which doesn't happen
        // if there's nothing new to send/receive.
        self.hub.process_time_queues();

        // Update all clients and server network
        let server = self.server.as_mut().expect("server not started");

        for (_client_key, state) in self.clients.iter_mut() {
            let (client, world) = state.client_and_world_mut();
            Self::update_client_server_at(&now, client, server, world, &mut self.server_world);
        }
    }

    /// Get the current tick count
    pub fn global_tick(&self) -> usize {
        self.global_tick
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
        let mut events_data: Vec<(ClientKey, ClientWorldEvents<TestEntity>, ClientTickEvents)> =
            Vec::new();
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

    /// Split borrow fields needed for client spawn
    /// Returns disjoint references: client state (mutable), registry (mutable, can be reborrowed as immutable)
    pub(crate) fn split_for_client_mut(
        &'_ mut self,
        client_key: &ClientKey,
    ) -> Option<(&'_ mut ClientState, &'_ mut EntityRegistry)> {
        let state = self.clients.get_mut(client_key)?;
        let registry = &mut self.entity_registry;
        Some((state, registry))
    }

    /// Create a client socket from the transport hub
    /// Returns the socket along with handles to identity_token, rejection_code, and client_addr
    fn create_client_socket(
        &self,
    ) -> (
        ClientSocket,
        Arc<Mutex<Option<naia_shared::IdentityToken>>>,
        Arc<Mutex<Option<u16>>>,
        SocketAddr,
    ) {
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
            self.hub
                .configure_link_conditioner(client_addr, client_to_server, server_to_client)
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
            client.process_all_packets(client_world.proxy_mut(), now);
            client.send_all_packets(client_world.proxy_mut());
        } else {
            client.receive_all_packets();
            client.send_all_packets(client_world.proxy_mut());
        }

        // Server update
        server.receive_all_packets();
        server.process_all_packets(server_world.proxy_mut(), now);
        server.send_all_packets(server_world.proxy());
    }

    // ========================================================================
    // Event History Tracking API (for BDD ordering assertions)
    // ========================================================================

    /// Track a server-side event for ordering assertions.
    pub fn track_server_event(&mut self, event: TrackedServerEvent) {
        self.server_event_history.push(event);
    }

    /// Track a client-side event for ordering assertions.
    pub fn track_client_event(&mut self, client_key: ClientKey, event: TrackedClientEvent) {
        self.client_event_history
            .entry(client_key)
            .or_default()
            .push(event);
    }

    /// Get the server event history (immutable).
    pub fn server_event_history(&self) -> &[TrackedServerEvent] {
        &self.server_event_history
    }

    /// Get a specific client's event history (immutable).
    pub fn client_event_history(&self, client_key: ClientKey) -> &[TrackedClientEvent] {
        self.client_event_history
            .get(&client_key)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Check if server observed events in the expected order.
    /// Returns true if `earlier` appears before `later` in server event history.
    pub fn server_event_before(&self, earlier: TrackedServerEvent, later: TrackedServerEvent) -> bool {
        let earlier_idx = self.server_event_history.iter().position(|e| *e == earlier);
        let later_idx = self.server_event_history.iter().position(|e| *e == later);
        match (earlier_idx, later_idx) {
            (Some(e), Some(l)) => e < l,
            _ => false,
        }
    }

    /// Check if client observed events in the expected order.
    /// Returns true if `earlier` appears before `later` in client event history.
    pub fn client_event_before(
        &self,
        client_key: ClientKey,
        earlier: TrackedClientEvent,
        later: TrackedClientEvent
    ) -> bool {
        let history = self.client_event_history(client_key);
        let earlier_idx = history.iter().position(|e| *e == earlier);
        let later_idx = history.iter().position(|e| *e == later);
        match (earlier_idx, later_idx) {
            (Some(e), Some(l)) => e < l,
            _ => false,
        }
    }

    /// Check if client observed a specific event.
    pub fn client_observed(&self, client_key: ClientKey, event: TrackedClientEvent) -> bool {
        self.client_event_history(client_key).contains(&event)
    }

    /// Check if server observed a specific event.
    pub fn server_observed(&self, event: TrackedServerEvent) -> bool {
        self.server_event_history.contains(&event)
    }

    /// Clear event history (useful when testing multiple connection attempts).
    pub fn clear_event_history(&mut self) {
        self.server_event_history.clear();
        self.client_event_history.clear();
    }

    // ========================================================================
    // Convenience Getters for BDD Tests
    // ========================================================================

    /// Get the last client key that was started.
    /// Panics if no client has been started.
    pub fn last_client(&self) -> ClientKey {
        self.last_client_key.expect("No client has been started")
    }

    /// Get the last client key if one exists.
    pub fn last_client_opt(&self) -> Option<ClientKey> {
        self.last_client_key
    }

    /// Get the last room key that was created.
    /// Panics if no room has been created.
    pub fn last_room(&self) -> RoomKey {
        self.last_room_key.expect("No room has been created")
    }

    /// Set the last room key (call this after creating a room).
    pub fn set_last_room(&mut self, room_key: RoomKey) {
        self.last_room_key = Some(room_key);
    }

    /// Get all client keys in the scenario.
    pub fn client_keys(&self) -> impl Iterator<Item = ClientKey> + '_ {
        self.clients.keys().copied()
    }
}
