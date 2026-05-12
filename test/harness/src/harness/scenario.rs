use std::{
    any::Any,
    collections::HashMap,
    net::SocketAddr,
    sync::Arc,
};

use parking_lot::Mutex as ParkingMutex;

use naia_client::{
    transport::local::{LocalAddrCell, LocalClientSocket, Socket as ClientSocket},
    Client as NaiaClient, ClientConfig, TickEvents as ClientTickEvents,
    Events as ClientWorldEvents,
};
use naia_demo_world::{WorldMut, WorldRef};
use naia_server::{
    transport::local::{LocalServerSocket, Socket as ServerSocket},
    RoomKey, Server as NaiaServer, ServerConfig, UserKey,
};
use naia_shared::{
    transport::local::{LocalTransportHub, FAKE_SERVER_ADDR},
    Instant, LinkConditionerConfig, LocalEntity, Protocol, ProtocolId, TestClock, WorldRefType,
};

use crate::harness::ClientEntityMut;
use crate::{
    harness::{
        client_events::{process_spawn_events, ClientEvents},
        client_state::ClientState,
        entity_registry::EntityRegistry,
        mutate_ctx::MutateCtx,
        server_events::ServerEvents,
        users::Users,
        ClientEntityRef, ClientKey, EntityKey, ExpectCtx,
    },
    Auth, TestEntity, TestWorld,
};

// ============================================================================
// AllocationSnapshot types (Phase 0.5)
// ============================================================================

/// Snapshot of diff-handler allocations at a point in time.
///
/// Returned by [`Scenario::diff_handler_snapshot`]. Use to assert that
/// component registrations are created/destroyed as expected.
pub struct DiffHandlerSnapshot {
    /// Total component registrations in the global diff handler.
    pub global_receivers: usize,
    /// Per-user component registration counts (keyed by [`ClientKey`]).
    pub user_receivers: HashMap<ClientKey, usize>,
    /// Global registration count broken down by component kind.
    pub per_component_kind: HashMap<naia_shared::ComponentKind, usize>,
}

// ============================================================================
// Wire trace capture types (Phase 0.5)
// ============================================================================

/// Direction of a captured wire packet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceDirection {
    ClientToServer,
    ServerToClient,
}

/// A single captured wire packet.
#[derive(Debug, Clone)]
pub struct TracePacket {
    pub direction: TraceDirection,
    pub bytes: Vec<u8>,
}

/// A captured sequence of wire packets for golden-trace regression testing.
///
/// Obtain via [`Scenario::take_trace`] after enabling capture with
/// [`Scenario::enable_trace_capture`].
#[derive(Debug, Clone, Default)]
pub struct Trace {
    pub packets: Vec<TracePacket>,
}

impl Trace {
    pub fn packet_count(&self) -> usize {
        self.packets.len()
    }

    pub fn client_to_server_count(&self) -> usize {
        self.packets
            .iter()
            .filter(|p| p.direction == TraceDirection::ClientToServer)
            .count()
    }

    pub fn server_to_client_count(&self) -> usize {
        self.packets
            .iter()
            .filter(|p| p.direction == TraceDirection::ServerToClient)
            .count()
    }
}

type Client = NaiaClient<TestEntity>;
type Server = NaiaServer<TestEntity>;

// Constants for simulation timing and retry behavior
const TICK_DURATION_MS: u64 = 16; // Default tick duration (~60 FPS)
const DEFAULT_MAX_EXPECT_TICKS: usize = 500; // Maximum ticks before expect() times out

/// A labeled trace event for deterministic ordering assertions.
///
/// Trace events are used to verify the order of operations in tests.
/// Unlike the typed `TrackedServerEvent` and `TrackedClientEvent`, these
/// are general-purpose string labels that can represent any operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceEvent {
    /// A label describing the operation (e.g., "scope_op_A", "command_B")
    pub label: String,
}

impl TraceEvent {
    /// Create a new trace event with the given label.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
        }
    }
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
    /// Last operation outcome tracking (for common error/panic assertions)
    last_operation_result: Option<OperationResult>,
    /// Ordered trace events for deterministic ordering assertions.
    /// Used by same-tick scheduling tests to verify operation order.
    trace_events: Vec<TraceEvent>,
    /// Type-erased storage for request/response keys between BDD steps.
    /// Maps a string key (e.g., "response_receive_key") to a boxed Any value.
    bdd_storage: HashMap<String, Box<dyn Any + Send + Sync>>,
    /// Received messages for BDD assertions (type-erased: Vec<u32> for TestMessage values).
    received_messages: Vec<u32>,
    /// Whether wire trace capture is currently enabled on the hub.
    trace_capture_enabled: bool,
}

/// Tracks the outcome of the last operation for BDD assertions.
/// Used by common steps like "Then no panic occurs" and "Then the operation returns Err".
#[derive(Debug, Clone)]
pub struct OperationResult {
    /// Whether the operation returned Ok or Err
    pub is_ok: bool,
    /// Error message if the operation returned Err
    pub error_msg: Option<String>,
    /// Whether a panic occurred (captured via catch_unwind)
    pub panic_msg: Option<String>,
}

impl Default for Scenario {
    fn default() -> Self {
        Self::new()
    }
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
            global_tick: 0,
            server_event_history: Vec::new(),
            client_event_history: HashMap::new(),
            last_client_key: None,
            last_room_key: None,
            last_operation_result: None,
            trace_events: Vec::new(),
            bdd_storage: HashMap::new(),
            received_messages: Vec::new(),
            trace_capture_enabled: false,
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

    pub fn server_start_with_protocol_id(
        &mut self,
        server_config: ServerConfig,
        protocol: Protocol,
        protocol_id: ProtocolId,
    ) {
        if self.server.is_some() {
            panic!("server_start() called multiple times");
        }

        let mut server = Server::new_with_protocol_id(server_config, protocol, protocol_id);
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

    pub fn client_start_with_protocol_id(
        &mut self,
        _display_name: &str,
        auth: Auth,
        client_config: ClientConfig,
        protocol: Protocol,
        protocol_id: ProtocolId,
    ) -> ClientKey {
        // Allow this to be called after either mutate() or expect()
        // This is a setup operation, not a mutate or expect, so it should be flexible
        self.allow_flexible_next();

        if self.server.is_none() {
            panic!("server_start() must be called before client_start()");
        }

        let client_key = ClientKey::new(self.next_client_key);
        self.next_client_key += 1;

        let mut client = Client::new_with_protocol_id(client_config, protocol, protocol_id);
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
        // if self.last_operation == LastOperation::Mutate {
        //     panic!("Scenario::mutate() called immediately after another mutate() call. Tests MUST alternate between mutate() and expect() calls.");
        // }

        let mut ctx = MutateCtx::new(self);
        let result = f(&mut ctx);
        // Update network to propagate immediate effects without draining events
        self.tick();
        result
    }

    /// Create a context for expectations with a custom tick timeout.
    ///
    /// # Example
    /// ```text
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

    /// Single-tick expectation check. Does NOT loop.
    ///
    /// Ticks once, collects events, creates ExpectCtx, calls closure.
    /// Returns the closure's result for the runner to handle (pass/retry/fail).
    ///
    /// This is the primitive used by Then step wrappers in the runner.
    /// Unlike `expect()`, this method does NOT loop - it performs exactly one tick
    /// and one evaluation, returning control to the caller.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use naia_test_harness::ExpectResult;
    ///
    /// for _tick in 0..100 {
    ///     let result = scenario.expect_once(|ctx| {
    ///         if ctx.server(|s| s.has_event::<SomeEvent>()) {
    ///             ExpectResult::Passed(())
    ///         } else {
    ///             ExpectResult::NotYet
    ///         }
    ///     });
    ///     match result {
    ///         ExpectResult::Passed(()) => break,
    ///         ExpectResult::NotYet => continue,
    ///         ExpectResult::Failed(msg) => panic!("{}", msg),
    ///     }
    /// }
    /// ```
    pub fn expect_once<T>(
        &mut self,
        f: impl FnOnce(&mut ExpectCtx<'_>) -> crate::harness::ExpectResult<T>,
    ) -> crate::harness::ExpectResult<T> {
        // Tick to advance simulation
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

        // Create ExpectCtx for this tick and call closure
        let mut ctx = ExpectCtx::new(self, server_events, client_events_map);
        f(&mut ctx)
    }

    /// Inject a raw packet from a client to the server (for testing malformed data).
    ///
    /// Call via `MutateCtx::inject_client_packet` — do not call directly on `Scenario`.
    pub(crate) fn inject_client_packet(&mut self, client_key: &ClientKey, data: Vec<u8>) -> bool {
        if let Some(addr) = self.client_to_addr_map.get(client_key) {
            return self.hub.inject_client_packet(addr, data);
        }
        false
    }

    /// Inject a raw packet from the server to a client (for testing malformed/oversized data).
    ///
    /// Call via `MutateCtx::inject_server_packet` — do not call directly on `Scenario`.
    pub(crate) fn inject_server_packet(&mut self, client_key: &ClientKey, data: Vec<u8>) -> bool {
        if let Some(addr) = self.client_to_addr_map.get(client_key) {
            return self.hub.inject_server_packet(addr, data);
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

        match result {
            Some(value) => value,
            None => {
                self.dump_all_state_for_timeout("expect timeout");
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

        match result {
            Some(value) => value,
            None => {
                self.dump_all_state_for_timeout(msg);
                panic!(
                    "Scenario::expect timed out after {} ticks: {}",
                    max_ticks, msg
                );
            }
        }
    }

    /// Auto-dump server + per-client identity state for every registered
    /// entity, called from the timeout panic path of
    /// `expect_with_ticks_internal{,_msg}`. Gated on `e2e_debug` — zero cost
    /// in release builds.
    fn dump_all_state_for_timeout(&self, label: &str) {
        #[cfg(feature = "e2e_debug")]
        {
            let client_keys: Vec<ClientKey> = self.clients.keys().copied().collect();
            let entity_keys: Vec<EntityKey> =
                self.entity_registry.all_entity_keys().collect();
            if entity_keys.is_empty() {
                eprintln!(
                    "[expect-timeout auto-dump] no registered entities to dump (label: {})",
                    label
                );
                return;
            }
            for ek in &entity_keys {
                self.debug_dump_identity_state(
                    &format!("{} :: timeout auto-dump", label),
                    ek,
                    &client_keys,
                );
            }
        }
        #[cfg(not(feature = "e2e_debug"))]
        {
            // Help fresh agents discover the feature flag without paying any cost.
            let _ = label;
            eprintln!(
                "[expect-timeout] re-run with --features e2e_debug for an auto-dump of identity state"
            );
        }
    }

    /// No-op. Previously reset the alternation-enforcement state machine; that
    /// enforcement is removed, but call sites are preserved to avoid churn.
    pub fn allow_flexible_next(&mut self) {}

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

    /// Register user_key → client_key by matching socket address.
    /// Used when `require_auth = false` so no ServerAuthEvent fires to establish the mapping.
    pub(crate) fn map_connect_event_by_addr(&mut self, user_key: &UserKey) -> Option<ClientKey> {
        let user_addr = self.server.as_ref()?.user_address(user_key)?;
        let client_key = self.client_to_addr_map
            .iter()
            .find(|(_, addr)| **addr == user_addr)
            .map(|(k, _)| *k)?;
        self.user_to_client_map.insert(*user_key, client_key);
        if let Some(state) = self.clients.get_mut(&client_key) {
            state.set_user_key(*user_key);
        }
        self.pending_auths.remove(&client_key);
        Some(client_key)
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
        self.clients.get(client_key)?.user_key()
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
        self.clients.get_mut(client_key).expect("client not found")
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
        let user_key = state.user_key()?;
        let local_entity = self.local_entity_for(key, &user_key)?;
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
        server_ref.local_entity(user_key)
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
    ///
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
    #[allow(clippy::type_complexity)]
    fn create_client_socket(
        &self,
    ) -> (
        ClientSocket,
        Arc<ParkingMutex<Option<naia_shared::IdentityToken>>>,
        Arc<ParkingMutex<Option<u16>>>,
        SocketAddr,
    ) {
        let (client_addr, auth_req_tx, auth_resp_rx, client_data_tx, client_data_rx) =
            self.hub.register_client();

        let addr_cell = LocalAddrCell::new();
        // For local transport, we know the server address immediately
        addr_cell.set_sync(self.hub.server_addr());

        // Each client gets its own identity token storage (not shared!)
        let identity_token = Arc::new(ParkingMutex::new(None));
        let rejection_code = Arc::new(ParkingMutex::new(None));

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
        // Client update — process_all_packets must also run during Disconnecting state,
        // because that is where disconnect_with_events() fires the ClientDisconnectEvent.
        let status = client.connection_status();
        if status.is_connected() || status.is_disconnecting() {
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
    pub fn server_event_before(
        &self,
        earlier: TrackedServerEvent,
        later: TrackedServerEvent,
    ) -> bool {
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
        later: TrackedClientEvent,
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

    // ========================================================================
    // Operation Result Tracking API (for common error/panic assertions)
    // ========================================================================

    /// Set the last operation result.
    pub fn set_operation_result(&mut self, result: OperationResult) {
        self.last_operation_result = Some(result);
    }

    /// Get the last operation result.
    pub fn last_operation_result(&self) -> Option<&OperationResult> {
        self.last_operation_result.as_ref()
    }

    /// Clear the last operation result.
    pub fn clear_operation_result(&mut self) {
        self.last_operation_result = None;
    }

    /// Record a successful operation.
    pub fn record_ok(&mut self) {
        self.set_operation_result(OperationResult {
            is_ok: true,
            error_msg: None,
            panic_msg: None,
        });
    }

    /// Record a failed operation with an error message.
    pub fn record_err(&mut self, msg: impl Into<String>) {
        self.set_operation_result(OperationResult {
            is_ok: false,
            error_msg: Some(msg.into()),
            panic_msg: None,
        });
    }

    /// Record a panic with its message.
    pub fn record_panic(&mut self, msg: impl Into<String>) {
        self.set_operation_result(OperationResult {
            is_ok: false,
            error_msg: None,
            panic_msg: Some(msg.into()),
        });
    }

    // ========================================================================
    // AllocationSnapshot API (Phase 0.5)
    // ========================================================================

    /// Snapshot the current diff-handler allocation state.
    ///
    /// Returns counts of component registrations in the global and per-user
    /// diff handlers. Useful for asserting that registrations appear/disappear
    /// at the right points in a scenario.
    pub fn diff_handler_snapshot(&self) -> DiffHandlerSnapshot {
        let server = self.server.as_ref().expect("server not started");
        let global_receivers = server.diff_handler_global_count();
        let per_component_kind = server.diff_handler_global_count_by_kind();
        let user_key_counts = server.diff_handler_user_counts();
        let user_receivers = user_key_counts
            .into_iter()
            .filter_map(|(user_key, count)| {
                self.user_to_client_key(&user_key).map(|ck| (ck, count))
            })
            .collect();
        DiffHandlerSnapshot {
            global_receivers,
            user_receivers,
            per_component_kind,
        }
    }

    /// Returns the current depth of the server's scope-change queue.
    ///
    /// Returns 0 when the `v2_push_pipeline` feature is not enabled (the legacy
    /// full-scan path has no queue).  Under `v2_push_pipeline`, returns the
    /// number of unprocessed entries after the last tick.
    pub fn scope_change_queue_len(&self) -> usize {
        let server = self.server.as_ref().expect("server not started");
        server.scope_change_queue_len()
    }

    /// Returns the total dirty update candidate count across all server connections.
    ///
    /// Returns 0 on the legacy full-scan path (no dirty set exists).
    /// Returns 0 after a Phase 3 tick that drained the dirty set cleanly.
    pub fn total_dirty_update_count(&self) -> usize {
        let server = self.server.as_ref().expect("server not started");
        server.total_dirty_update_count()
    }

    // ========================================================================
    // Wire Trace Capture API (Phase 0.5)
    // ========================================================================

    /// Enable wire-level packet recording on the local transport hub.
    ///
    /// After calling this, every packet sent or received through the hub will
    /// be appended to an internal buffer. Call [`take_trace`] to consume the
    /// buffer and obtain a [`Trace`].
    pub fn enable_trace_capture(&mut self) {
        self.hub.enable_packet_recording();
        self.trace_capture_enabled = true;
    }

    /// Consume the recorded wire trace and return it as a [`Trace`].
    ///
    /// The internal buffer is cleared. Capture remains enabled; call
    /// [`enable_trace_capture`] again if you need to restart from scratch.
    ///
    /// # Panics
    ///
    /// Panics if trace capture was never enabled.
    pub fn take_trace(&mut self) -> Trace {
        assert!(
            self.trace_capture_enabled,
            "take_trace() called without first calling enable_trace_capture()"
        );
        let raw = self.hub.take_recorded_packets();
        let packets = raw
            .into_iter()
            .map(|(server_to_client, bytes)| TracePacket {
                direction: if server_to_client {
                    TraceDirection::ServerToClient
                } else {
                    TraceDirection::ClientToServer
                },
                bytes,
            })
            .collect();
        Trace { packets }
    }

    // ========================================================================
    // Trace Sink API (for deterministic ordering assertions)
    // ========================================================================

    /// Push a labeled trace event.
    ///
    /// Events are appended in order and can be queried to verify
    /// the order of operations during a tick or across ticks.
    ///
    /// # Example
    ///
    /// ```ignore
    /// scenario.mutate(|ctx| {
    ///     ctx.scenario_mut().trace_push("scope_op_A");
    ///     ctx.scenario_mut().trace_push("scope_op_B");
    /// });
    /// scenario.expect(|ctx| {
    ///     let labels: Vec<_> = ctx.scenario().trace_labels().collect();
    ///     assert_eq!(labels, vec!["scope_op_A", "scope_op_B"]);
    ///     Some(())
    /// });
    /// ```
    pub fn trace_push(&mut self, label: impl Into<String>) {
        self.trace_events.push(TraceEvent::new(label));
    }

    /// Get all trace events in order.
    pub fn trace_all(&self) -> &[TraceEvent] {
        &self.trace_events
    }

    /// Get an iterator over trace event labels.
    pub fn trace_labels(&self) -> impl Iterator<Item = &str> {
        self.trace_events.iter().map(|e| e.label.as_str())
    }

    /// Clear all trace events.
    ///
    /// Useful when testing multiple phases or when resetting between scenarios.
    pub fn trace_clear(&mut self) {
        self.trace_events.clear();
    }

    /// Check if the trace contains a subsequence of labels in order.
    ///
    /// Returns true if all labels in `expected` appear in the trace
    /// in the same relative order (not necessarily contiguous).
    ///
    /// # Example
    ///
    /// ```ignore
    /// scenario.trace_push("A");
    /// scenario.trace_push("B");
    /// scenario.trace_push("C");
    ///
    /// assert!(scenario.trace_contains_subsequence(&["A", "C"])); // true
    /// assert!(scenario.trace_contains_subsequence(&["A", "B", "C"])); // true
    /// assert!(!scenario.trace_contains_subsequence(&["C", "A"])); // false
    /// ```
    pub fn trace_contains_subsequence(&self, expected: &[&str]) -> bool {
        if expected.is_empty() {
            return true;
        }
        let mut expected_iter = expected.iter();
        let mut looking_for = expected_iter.next();
        for event in &self.trace_events {
            if let Some(expected_label) = looking_for {
                if event.label == *expected_label {
                    looking_for = expected_iter.next();
                    if looking_for.is_none() {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Get the number of trace events.
    pub fn trace_len(&self) -> usize {
        self.trace_events.len()
    }

    /// Check if the trace is empty.
    pub fn trace_is_empty(&self) -> bool {
        self.trace_events.is_empty()
    }

    // ========================================================================
    // BDD Storage API (for request/response keys between steps)
    // ========================================================================

    /// Store a value in BDD storage for later retrieval.
    pub fn bdd_store<T: Any + Send + Sync>(&mut self, key: &str, value: T) {
        self.bdd_storage.insert(key.to_string(), Box::new(value));
    }

    /// Retrieve a value from BDD storage.
    pub fn bdd_get<T: Any + Clone>(&self, key: &str) -> Option<T> {
        self.bdd_storage
            .get(key)
            .and_then(|v| v.downcast_ref::<T>().cloned())
    }

    /// Take (remove and return) a value from BDD storage.
    pub fn bdd_take<T: Any>(&mut self, key: &str) -> Option<T> {
        self.bdd_storage
            .remove(key)
            .and_then(|v| v.downcast::<T>().ok().map(|b| *b))
    }

    // ========================================================================
    // Received Messages API (for message ordering assertions)
    // ========================================================================

    /// Push a received message value.
    pub fn push_received_message(&mut self, value: u32) {
        self.received_messages.push(value);
    }

    /// Get all received messages.
    pub fn received_messages(&self) -> &[u32] {
        &self.received_messages
    }

    /// Clear received messages.
    pub fn clear_received_messages(&mut self) {
        self.received_messages.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tests that trace events are appended in order.
    #[test]
    fn trace_appends_in_order() {
        let mut scenario = Scenario::new();

        scenario.trace_push("first");
        scenario.trace_push("second");
        scenario.trace_push("third");

        let labels: Vec<_> = scenario.trace_labels().collect();
        assert_eq!(labels, vec!["first", "second", "third"]);
    }

    /// Tests that trace_clear empties the trace.
    #[test]
    fn trace_clear_empties() {
        let mut scenario = Scenario::new();

        scenario.trace_push("A");
        scenario.trace_push("B");
        assert!(!scenario.trace_is_empty());
        assert_eq!(scenario.trace_len(), 2);

        scenario.trace_clear();

        assert!(scenario.trace_is_empty());
        assert_eq!(scenario.trace_len(), 0);
        assert_eq!(scenario.trace_all().len(), 0);
    }

    /// Tests trace_contains_subsequence with various patterns.
    #[test]
    fn trace_contains_subsequence_patterns() {
        let mut scenario = Scenario::new();

        scenario.trace_push("A");
        scenario.trace_push("B");
        scenario.trace_push("C");
        scenario.trace_push("D");

        // Full sequence matches
        assert!(scenario.trace_contains_subsequence(&["A", "B", "C", "D"]));

        // Partial subsequence matches
        assert!(scenario.trace_contains_subsequence(&["A", "C"]));
        assert!(scenario.trace_contains_subsequence(&["A", "D"]));
        assert!(scenario.trace_contains_subsequence(&["B", "D"]));

        // Single element matches
        assert!(scenario.trace_contains_subsequence(&["A"]));
        assert!(scenario.trace_contains_subsequence(&["D"]));

        // Empty subsequence matches
        assert!(scenario.trace_contains_subsequence(&[]));

        // Wrong order does not match
        assert!(!scenario.trace_contains_subsequence(&["C", "A"]));
        assert!(!scenario.trace_contains_subsequence(&["D", "B"]));

        // Non-existent element does not match
        assert!(!scenario.trace_contains_subsequence(&["X"]));
        assert!(!scenario.trace_contains_subsequence(&["A", "X"]));
    }

    // =========================================================================
    // Phase 0.5: AllocationSnapshot API smoke tests
    // =========================================================================

    /// Verifies that diff_handler_snapshot() returns zero counts before server start.
    ///
    /// This test ensures the snapshot API is wired end-to-end and returns
    /// consistent zero values when no entities have been registered.
    #[test]
    fn diff_handler_snapshot_zero_before_connect() {
        use crate::test_protocol::protocol;
        use naia_server::ServerConfig;

        let mut scenario = Scenario::new();
        scenario.server_start(ServerConfig::default(), protocol());

        let snap = scenario.diff_handler_snapshot();
        assert_eq!(snap.global_receivers, 0, "no components registered yet");
        assert!(
            snap.user_receivers.is_empty(),
            "no users connected yet"
        );
        assert!(
            snap.per_component_kind.is_empty(),
            "no component kinds registered yet"
        );
    }

    // =========================================================================
    // Phase 0.5: Wire trace capture smoke tests
    // =========================================================================

    /// Verifies that enable_trace_capture() + take_trace() compiles and the
    /// trace is empty before any ticks are run.
    #[test]
    fn trace_capture_empty_before_traffic() {
        let mut scenario = Scenario::new();
        scenario.enable_trace_capture();
        let trace = scenario.take_trace();
        assert_eq!(trace.packet_count(), 0);
        assert_eq!(trace.client_to_server_count(), 0);
        assert_eq!(trace.server_to_client_count(), 0);
    }

    /// Verifies direction counts are consistent.
    #[test]
    fn trace_direction_counts_consistent() {
        let trace = Trace {
            packets: vec![
                TracePacket {
                    direction: TraceDirection::ClientToServer,
                    bytes: vec![1, 2, 3],
                },
                TracePacket {
                    direction: TraceDirection::ServerToClient,
                    bytes: vec![4, 5],
                },
                TracePacket {
                    direction: TraceDirection::ServerToClient,
                    bytes: vec![6],
                },
            ],
        };
        assert_eq!(trace.packet_count(), 3);
        assert_eq!(trace.client_to_server_count(), 1);
        assert_eq!(trace.server_to_client_count(), 2);
    }

    // =========================================================================
    // Phase 5 spike: immutable component zero-allocation gate
    // =========================================================================

    /// Verifies that #[replicate(immutable)] components are NOT registered in
    /// GlobalDiffHandler, while regular mutable components are.
    ///
    /// This is the Phase 5 spike gate: "verify toy component roundtrips with
    /// zero GlobalDiffHandler entries".
    #[test]
    fn immutable_component_has_no_global_diff_handler_entry() {
        use crate::test_protocol::{protocol, ImmutableLabel, Position};
        use naia_server::ServerConfig;
        use naia_shared::ComponentKind;

        let mut scenario = Scenario::new();
        scenario.server_start(ServerConfig::default(), protocol());

        {
            let (server, world, _registry, _users) = scenario.split_for_server_mut();
            let world_proxy = world.proxy_mut();
            let mut entity_mut = server.spawn_entity(world_proxy);
            entity_mut.insert_component(Position::new(1.0, 2.0));
            entity_mut.insert_component(ImmutableLabel);
        }

        let snap = scenario.diff_handler_snapshot();

        let pos_kind = ComponentKind::of::<Position>();
        let imm_kind = ComponentKind::of::<ImmutableLabel>();

        assert_eq!(
            snap.per_component_kind.get(&pos_kind).copied().unwrap_or(0),
            1,
            "Position (mutable) must have 1 GlobalDiffHandler entry"
        );
        assert!(
            !snap.per_component_kind.contains_key(&imm_kind),
            "ImmutableLabel must NOT appear in GlobalDiffHandler"
        );
        assert_eq!(
            snap.global_receivers, 1,
            "only the mutable Position component should be registered globally"
        );
    }
}
