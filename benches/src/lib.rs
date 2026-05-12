pub mod bench_protocol;
pub mod serde_quat;

use std::{net::SocketAddr, sync::Arc};

use parking_lot::Mutex;

use naia_client::{
    transport::local::{LocalAddrCell, LocalClientSocket, Socket as ClientSocket},
    Client as NaiaClient, ClientConfig,
};
use naia_demo_world::World;
use naia_server::{
    transport::local::{LocalServerSocket, Socket as ServerSocket},
    ConnectEvent, RoomKey, Server as NaiaServer, ServerConfig, UserKey,
};
use naia_shared::{
    transport::local::{LocalTransportHub, FAKE_SERVER_ADDR},
    Instant, TestClock,
};

use crate::bench_protocol::{
    bench_protocol, BenchAuth, BenchComponent, BenchEntity, BenchImmutableComponent, BenchResource,
    HaloTile, HaloUnit, Position, PositionQ, PositionQState, Rotation, RotationQ, Velocity,
    VelocityQ, VelocityQState,
};
use crate::serde_quat::BenchQuat;
use naia_server::ReplicationConfig;
use naia_shared::{EntityAuthStatus, SignedVariableFloat};

/// Simulated tick duration. 16 ms = 62.5 Hz, a reasonable game-server
/// default. Load-bearing: don't change without auditing callers that
/// interpret tick counts as wall-time (e.g. bandwidth windows).
const TICK_MS: u64 = 16;

/// Max ticks to wait for K clients to connect during setup. 500 ticks
/// ≈ 8 s simulated; the local transport typically finishes in <10.
const SETUP_TIMEOUT: usize = 500;

/// Max ticks to wait for all entities to replicate to all clients. Sized
/// for worst-case 8-client × 10K-entity scenarios; Naia chunks spawns
/// across ticks so large counts genuinely need many ticks.
const REPLICATE_TIMEOUT: usize = 10_000;

// ─── bench!() macro ───────────────────────────────────────────────────────────

/// Produces a qualified benchmark name: `module::path::name`.
/// Mirrors Bevy's bench!() macro to avoid name collisions across groups.
#[macro_export]
macro_rules! bench {
    ($name:literal) => {
        concat!(module_path!(), "::", $name)
    };
}

// ─── Entity kind ──────────────────────────────────────────────────────────────

pub enum EntityKind {
    Mutable,
    Immutable,
}

/// Realistic-archetype categories for `wire/bandwidth_realistic`.
///
/// Each archetype is a fixed component composition that stands in for a
/// canonical netgame state shape. Sizes are intentionally typical (3×f32
/// position, 3×f32 velocity, 2×f32 camera rotation) — measured bytes
/// reflect Naia's framing on top of these shapes.
#[derive(Copy, Clone, Debug)]
pub enum Archetype {
    /// `Position` + `Velocity` + `Rotation` — first-class avatar.
    Player,
    /// `Position` + `Velocity` — physics-driven projectile, no aim.
    Projectile,
    /// `Position` + `Velocity` + `Rotation` — same shape as Player; kept
    /// distinct so benches can track per-class budgets independently.
    Vehicle,
}

// ─── BenchWorldBuilder ────────────────────────────────────────────────────────

/// Fluent builder for [`BenchWorld`]. Call from `iter_batched` setup closures —
/// this is never measured.
pub struct BenchWorldBuilder {
    user_count: usize,
    entity_count: usize,
    entity_kind: EntityKind,
    static_entity_count: usize,
    scoped: bool,
    uncapped_bandwidth: bool,
    tick_ms: u64,
    delegated: bool,
}

impl Default for BenchWorldBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl BenchWorldBuilder {
    pub fn new() -> Self {
        Self {
            user_count: 1,
            entity_count: 0,
            entity_kind: EntityKind::Mutable,
            static_entity_count: 0,
            scoped: true,
            uncapped_bandwidth: false,
            tick_ms: TICK_MS,
            delegated: false,
        }
    }

    /// Override the simulated tick clock. Default is 16 ms (62.5 Hz).
    /// Use `tick_rate_hz(25)` for a 25 Hz / 40 ms cyberlith-scale scenario.
    pub fn tick_rate_hz(mut self, hz: u16) -> Self {
        self.tick_ms = 1000 / hz as u64;
        self
    }

    /// Disable the per-connection bandwidth cap on the server (sets
    /// `target_bytes_per_sec = u32::MAX`). Required when the bench needs to
    /// measure *raw* bytes/tick of the dirty workload — the default 64 KB/s
    /// cap clips dense-update scenarios at ~1288 B/tick, masking compaction
    /// wins. The bandwidth_realistic_quantized bench uses this so the
    /// quantized-vs-naive comparison reflects the wire format, not the cap.
    pub fn uncapped_bandwidth(mut self) -> Self {
        self.uncapped_bandwidth = true;
        self
    }

    pub fn users(mut self, n: usize) -> Self {
        self.user_count = n;
        self
    }

    pub fn entities(mut self, n: usize) -> Self {
        self.entity_count = n;
        self
    }

    pub fn immutable(mut self) -> Self {
        self.entity_kind = EntityKind::Immutable;
        self
    }

    /// Skip adding entities to the room. They remain server-local with no
    /// dirty receivers attached — a baseline for the Win-3 push model,
    /// where the idle/mutation cost is purely framework overhead with no
    /// per-entity dispatch at all.
    pub fn unscoped(mut self) -> Self {
        self.scoped = false;
        self
    }

    /// Spawn N static [`HaloTile`] entities during setup (IDs from static pool;
    /// no diff-tracking after initial replication). Used to simulate the
    /// cyberlith tile layer and verify the split-pool bandwidth savings.
    pub fn static_entities(mut self, n: usize) -> Self {
        self.static_entity_count = n;
        self
    }

    /// Configure all spawned dynamic entities as `Delegated` — required for
    /// any bench that calls `give_authority_on_entity` or `take_authority_on_entity`.
    pub fn delegated(mut self) -> Self {
        self.delegated = true;
        self
    }

    /// Build to steady-state. Not measured — call from `iter_batched` setup.
    pub fn build(self) -> BenchWorld {
        BenchWorld::new(
            self.user_count,
            self.entity_count,
            self.entity_kind,
            self.static_entity_count,
            self.scoped,
            self.uncapped_bandwidth,
            self.tick_ms,
            self.delegated,
        )
    }
}

// ─── BenchWorld ───────────────────────────────────────────────────────────────

/// Per-phase wall-time breakdown of one tick. See `BenchWorld::tick_timed`.
#[derive(Debug, Clone, Copy)]
pub struct TickBreakdown {
    pub hub: std::time::Duration,
    pub clients: std::time::Duration,
    pub srv_rx: std::time::Duration,
    pub srv_tx: std::time::Duration,
    pub drain: std::time::Duration,
}

/// A Naia server + N clients in steady state, using local (in-memory) transport.
///
/// `tick()` is the measured operation for most benchmarks. Setup cost lives in
/// [`BenchWorldBuilder::build`] which is always called from `iter_batched`'s
/// setup closure.
pub struct BenchWorld {
    hub: LocalTransportHub,
    server: NaiaServer<BenchEntity>,
    server_world: World,
    clients: Vec<(NaiaClient<BenchEntity>, World)>,
    connected_user_keys: Vec<UserKey>,
    room_key: RoomKey,
    pub server_entities: Vec<BenchEntity>,
    tick_ms: u64,
}

impl BenchWorld {
    #[allow(clippy::too_many_arguments)]
    fn new(
        user_count: usize,
        entity_count: usize,
        entity_kind: EntityKind,
        static_entity_count: usize,
        scoped: bool,
        uncapped_bandwidth: bool,
        tick_ms: u64,
        delegated: bool,
    ) -> Self {
        TestClock::init(0);

        let server_addr: SocketAddr = FAKE_SERVER_ADDR.parse().expect("invalid addr");
        let hub = LocalTransportHub::new(server_addr);

        let protocol = bench_protocol();
        let mut server_config = ServerConfig::default();
        if uncapped_bandwidth {
            server_config.connection.bandwidth.target_bytes_per_sec = u32::MAX;
        }
        let mut server: NaiaServer<BenchEntity> =
            NaiaServer::new(server_config, protocol.clone());
        server.listen(ServerSocket::new(
            LocalServerSocket::new(hub.clone()),
            None,
        ));

        let mut server_world = World::default();

        let client_config = ClientConfig::default();

        // Connect all clients
        let mut clients: Vec<(NaiaClient<BenchEntity>, World)> = Vec::new();
        for _ in 0..user_count {
            let mut client: NaiaClient<BenchEntity> =
                NaiaClient::new(client_config.clone(), protocol.clone());
            let (client_addr, auth_req_tx, auth_resp_rx, data_tx, data_rx) = hub.register_client();
            let addr_cell = LocalAddrCell::new();
            addr_cell.set_sync(hub.server_addr());
            let inner = LocalClientSocket::new_with_tokens(
                client_addr,
                hub.server_addr(),
                auth_req_tx,
                auth_resp_rx,
                data_tx,
                data_rx,
                addr_cell,
                Arc::new(Mutex::new(None)),
                Arc::new(Mutex::new(None)),
            );
            let socket = ClientSocket::new(inner, None);
            client.auth(BenchAuth);
            client.connect(socket);
            clients.push((client, World::default()));
        }

        // Run until all clients connected and room created
        let mut connected_user_keys: Vec<UserKey> = Vec::new();
        let mut room_key: Option<RoomKey> = None;

        for _ in 0..SETUP_TIMEOUT {
            advance_tick(&hub, &mut server, &mut server_world, &mut clients, tick_ms);

            let mut events = server.take_world_events();
            let _ = server.take_tick_events(&Instant::now());

            // Auto-accept all auth requests (type-erased)
            let auths = events.take_auths();
            for (_kind, user_auths) in auths {
                for (user_key, _) in user_auths {
                    server.accept_connection(&user_key);
                }
            }

            // Handle new connections
            for user_key in events.read::<ConnectEvent>() {
                connected_user_keys.push(user_key);
                if room_key.is_none() {
                    room_key = Some(server.create_room().key());
                }
                server
                    .room_mut(room_key.as_ref().unwrap())
                    .add_user(&user_key);
            }

            for (client, _) in &mut clients {
                let _ = client.take_world_events();
                let _ = client.take_tick_events(&Instant::now());
            }

            if connected_user_keys.len() >= user_count {
                break;
            }
        }

        assert!(
            connected_user_keys.len() >= user_count,
            "BenchWorld setup timed out waiting for {} connections (got {})",
            user_count,
            connected_user_keys.len()
        );

        let room_key = room_key.expect("no room created — connection failed");

        // Spawn static entities (IDs from static pool — no diff-tracking after initial replication)
        let mut server_entities: Vec<BenchEntity> = Vec::new();
        for _ in 0..static_entity_count {
            let entity = {
                let mut em = server.spawn_entity(server_world.proxy_mut());
                em.as_static();
                let entity = em.id();
                em.insert_component(HaloTile);
                entity
            };
            if scoped {
                server.room_mut(&room_key).add_entity(&entity);
            }
            server_entities.push(entity);
        }

        // Spawn dynamic entities (IDs from dynamic pool — diff-tracked each tick)
        for i in 0..entity_count {
            let entity = {
                let mut entity_mut = server.spawn_entity(server_world.proxy_mut());
                let entity = entity_mut.id();
                match entity_kind {
                    EntityKind::Mutable => {
                        entity_mut.insert_component(BenchComponent::new(i as u32));
                    }
                    EntityKind::Immutable => {
                        entity_mut.insert_component(BenchImmutableComponent);
                    }
                }
                entity
            };
            if delegated {
                let mut world_mut = server_world.proxy_mut();
                server.configure_entity_replication(
                    &mut world_mut,
                    &entity,
                    ReplicationConfig::delegated(),
                );
            }
            if scoped {
                server.room_mut(&room_key).add_entity(&entity);
            }
            server_entities.push(entity);
        }

        let total_entity_count = static_entity_count + entity_count;

        // Run until all entities replicated to all clients
        if scoped && total_entity_count > 0 && user_count > 0 {
            for _ in 0..REPLICATE_TIMEOUT {
                advance_tick(&hub, &mut server, &mut server_world, &mut clients, tick_ms);
                drain_all_events(&mut server, &mut clients);

                let all_visible = clients.iter().all(|(client, world)| {
                    client.entities(&world.proxy()).len() >= total_entity_count
                });
                if all_visible {
                    break;
                }
            }
        }

        Self {
            hub,
            server,
            server_world,
            clients,
            connected_user_keys,
            room_key,
            server_entities,
            tick_ms,
        }
    }

    /// Run one full tick (server + all clients). This is the measured operation.
    ///
    /// Events are drained after each tick to prevent unbounded accumulation.
    #[inline]
    pub fn tick(&mut self) {
        advance_tick(
            &self.hub,
            &mut self.server,
            &mut self.server_world,
            &mut self.clients,
            self.tick_ms,
        );
        drain_all_events(&mut self.server, &mut self.clients);
    }

    /// Returns the number of (room, user, entity) tuples via the pending path
    /// (after mark_all_scope_checks_pending). Used by the scope bench to
    /// measure the cost of a full re-evaluation cycle.
    pub fn scope_checks_all_tuple_count(&mut self) -> usize {
        self.server.mark_all_scope_checks_pending();
        let count = self.server.scope_checks_pending().len();
        self.server.mark_scope_checks_pending_handled();
        count
    }

    /// Returns the number of pending (room, user, entity) tuples. Zero in
    /// steady state — used to verify the fast-path is actually free.
    pub fn scope_checks_pending_tuple_count(&mut self) -> usize {
        let n = self.server.scope_checks_pending().len();
        self.server.mark_scope_checks_pending_handled();
        n
    }

    /// Diagnostic-only variant of `tick()` that reports per-phase wall time.
    /// Used by `examples/phase4_tick_internals.rs` to localize remaining
    /// per-tick cost inside the server idle path. Not exposed through the
    /// criterion benches — keeping `tick()` itself as the canonical surface.
    pub fn tick_timed(&mut self) -> TickBreakdown {
        use std::time::Instant as StdInstant;
        TestClock::advance(self.tick_ms);
        let now = Instant::now();

        let t = StdInstant::now();
        self.hub.process_time_queues();
        let hub = t.elapsed();

        let t = StdInstant::now();
        for (client, client_world) in self.clients.iter_mut() {
            client.receive_all_packets();
            client.process_all_packets(client_world.proxy_mut(), &now);
            client.send_all_packets(client_world.proxy_mut());
        }
        let clients = t.elapsed();

        let t = StdInstant::now();
        self.server.receive_all_packets();
        self.server
            .process_all_packets(self.server_world.proxy_mut(), &now);
        let srv_rx = t.elapsed();

        let t = StdInstant::now();
        self.server.send_all_packets(self.server_world.proxy());
        let srv_tx = t.elapsed();

        let t = StdInstant::now();
        drain_all_events(&mut self.server, &mut self.clients);
        let drain = t.elapsed();

        TickBreakdown {
            hub,
            clients,
            srv_rx,
            srv_tx,
            drain,
        }
    }

    /// Mutate entities in `range` (by index in `server_entities`). Skips
    /// entries that don't have a `BenchComponent`. Used by benches that place
    /// tiles at the start of the entity list and units at the tail.
    #[inline]
    pub fn mutate_entity_range(&mut self, range: std::ops::Range<usize>) {
        for idx in range {
            if idx >= self.server_entities.len() {
                continue;
            }
            let entity = self.server_entities[idx];
            if let Some(mut comp) = self
                .server
                .entity_mut(self.server_world.proxy_mut(), &entity)
                .component::<BenchComponent>()
            {
                *comp.value = (*comp.value).wrapping_add(1);
            }
        }
    }

    /// Mutate the first `count` entities' mutable component values.
    /// Call before `tick()` to benchmark active workload.
    #[inline]
    pub fn mutate_entities(&mut self, count: usize) {
        let count = count.min(self.server_entities.len());
        for i in 0..count {
            let entity = self.server_entities[i];
            if let Some(mut comp) = self
                .server
                .entity_mut(self.server_world.proxy_mut(), &entity)
                .component::<BenchComponent>()
            {
                *comp.value = (*comp.value).wrapping_add(1);
            }
        }
    }

    /// Spawn `tile_count` immutable [`HaloTile`] entities and `unit_count` mutable
    /// [`HaloUnit`] entities, add all to the room, then drive ticks until every
    /// entity is replicated to every client.
    ///
    /// **Not measured** — call from `iter_custom` setup or `iter_batched` setup.
    pub fn spawn_halo_scene(&mut self, tile_count: usize, unit_count: usize) {
        // Tiles are static entities: IDs from the static pool, no diff-tracking after scope-entry.
        for _ in 0..tile_count {
            let entity = {
                let mut em = self.server.spawn_entity(self.server_world.proxy_mut());
                em.as_static();
                let id = em.id();
                em.insert_component(HaloTile);
                id
            };
            self.server.room_mut(&self.room_key).add_entity(&entity);
            self.server_entities.push(entity);
        }
        for i in 0..unit_count {
            let entity = {
                let mut em = self.server.spawn_entity(self.server_world.proxy_mut());
                let id = em.id();
                em.insert_component(HaloUnit::new(i as i16, 0, 0));
                id
            };
            self.server.room_mut(&self.room_key).add_entity(&entity);
            self.server_entities.push(entity);
        }
        // Drive ticks until all entities reach all clients.
        let target_per_client = tile_count + unit_count;
        if target_per_client > 0 && !self.clients.is_empty() {
            let t0 = std::time::Instant::now();
            let mut last_reported = 0usize;
            for tick_n in 0..REPLICATE_TIMEOUT {
                self.tick();
                let min_visible = self.clients.iter()
                    .map(|(client, world)| client.entities(&world.proxy()).len())
                    .min()
                    .unwrap_or(0);
                // Print progress every 1 000 entities replicated (to stderr — not captured by criterion).
                if min_visible / 1_000 > last_reported / 1_000 {
                    last_reported = min_visible;
                    eprintln!(
                        "[spawn_halo_scene] tick {tick_n}: {min_visible}/{target_per_client} entities replicated ({:.1}s)",
                        t0.elapsed().as_secs_f64()
                    );
                }
                if min_visible >= target_per_client {
                    eprintln!(
                        "[spawn_halo_scene] complete: {target_per_client} entities in {tick_n} ticks ({:.1}s)",
                        t0.elapsed().as_secs_f64()
                    );
                    break;
                }
            }
            assert!(
                self.clients.iter().all(|(client, world)| {
                    client.entities(&world.proxy()).len() >= target_per_client
                }),
                "spawn_halo_scene: {target_per_client} entities did not replicate within \
                 {REPLICATE_TIMEOUT} ticks"
            );
        }
    }

    /// Mutate the first `count` [`HaloUnit`] entities (increment facing by 1).
    /// Call before `tick()` to drive an active-workload scenario.
    #[inline]
    pub fn mutate_halo_units(&mut self, count: usize) {
        let count = count.min(self.server_entities.len());
        for i in 0..count {
            let entity = self.server_entities[i];
            if let Some(mut unit) = self
                .server
                .entity_mut(self.server_world.proxy_mut(), &entity)
                .component::<HaloUnit>()
            {
                *unit.facing = unit.facing.wrapping_add(1);
            }
        }
    }

    /// Run a complete server tick, then time **one** client's receive path in
    /// isolation. All other clients are drained without timing.
    ///
    /// Use as the measured operation for client-side capacity benches. The
    /// returned [`std::time::Duration`] is the cost a single game client pays
    /// to consume one server tick worth of updates.
    pub fn tick_server_then_measure_one_client(
        &mut self,
        client_idx: usize,
    ) -> std::time::Duration {
        use std::time::Instant as StdInstant;

        let tick_ms = self.tick_ms;
        TestClock::advance(tick_ms);
        let now = Instant::now();

        self.hub.process_time_queues();

        // Full server step (produces outgoing packets for all clients).
        self.server.receive_all_packets();
        self.server.process_all_packets(self.server_world.proxy_mut(), &now);
        self.server.send_all_packets(self.server_world.proxy());

        // Time only the target client's receive path.
        let (client, world) = &mut self.clients[client_idx];
        let t = StdInstant::now();
        client.receive_all_packets();
        client.process_all_packets(world.proxy_mut(), &now);
        let elapsed = t.elapsed();

        // Drain remaining clients (no timing), then send all.
        for (i, (c, w)) in self.clients.iter_mut().enumerate() {
            if i == client_idx {
                c.send_all_packets(w.proxy_mut());
                continue;
            }
            c.receive_all_packets();
            c.process_all_packets(w.proxy_mut(), &now);
            c.send_all_packets(w.proxy_mut());
        }
        drain_all_events(&mut self.server, &mut self.clients);

        elapsed
    }

    pub fn room_key(&self) -> &RoomKey {
        &self.room_key
    }

    pub fn connected_user_keys(&self) -> &[UserKey] {
        &self.connected_user_keys
    }

    /// Return the number of entities the first client has received.
    pub fn client_entity_count(&self) -> usize {
        if let Some((client, world)) = self.clients.first() {
            client.entities(&world.proxy()).len()
        } else {
            0
        }
    }

    /// Access the server directly for advanced benchmark setup.
    /// Spawn one new mutable entity and add it to the room.
    /// Does NOT wait for replication — call tick() after to process.
    pub fn spawn_one_entity(&mut self) {
        let entity = {
            let mut entity_mut = self.server.spawn_entity(self.server_world.proxy_mut());
            let entity = entity_mut.id();
            entity_mut.insert_component(BenchComponent::new(0));
            entity
        };
        self.server.room_mut(&self.room_key).add_entity(&entity);
        self.server_entities.push(entity);
    }

    /// Spawn `count` entities of the given archetype, add to the room, and
    /// drive ticks until all clients have received them. Used by
    /// `wire/bandwidth_realistic`.
    ///
    /// Component composition by archetype:
    /// - `Player`     → `Position` + `Velocity` + `Rotation`
    /// - `Projectile` → `Position` + `Velocity`
    /// - `Vehicle`    → `Position` + `Velocity` + `Rotation`
    ///
    /// Returns the index range in `server_entities` that was just appended.
    pub fn spawn_archetype(
        &mut self,
        archetype: Archetype,
        count: usize,
    ) -> std::ops::Range<usize> {
        let start = self.server_entities.len();
        for i in 0..count {
            let f = i as f32;
            let entity = {
                let mut em = self.server.spawn_entity(self.server_world.proxy_mut());
                let id = em.id();
                em.insert_component(Position::new(f, f, f));
                em.insert_component(Velocity::new(f, f, f));
                if matches!(archetype, Archetype::Player | Archetype::Vehicle) {
                    em.insert_component(Rotation::new(f, f));
                }
                id
            };
            self.server.room_mut(&self.room_key).add_entity(&entity);
            self.server_entities.push(entity);
        }
        start..self.server_entities.len()
    }

    /// Spawn `count` entities with cyberlith-shape **quantized** components for
    /// `wire/bandwidth_realistic_quantized`.
    ///
    /// Composition by archetype:
    /// - `Player`     → `PositionQ` + `VelocityQ` + `RotationQ`
    /// - `Projectile` → `PositionQ` + `VelocityQ`
    /// - `Vehicle`    → `PositionQ` + `VelocityQ` + `RotationQ`
    ///
    /// Returns the index range in `server_entities` that was just appended.
    pub fn spawn_archetype_quantized(
        &mut self,
        archetype: Archetype,
        count: usize,
    ) -> std::ops::Range<usize> {
        let start = self.server_entities.len();
        for i in 0..count {
            let f = i as f32;
            let entity = {
                let mut em = self.server.spawn_entity(self.server_world.proxy_mut());
                let id = em.id();
                em.insert_component(PositionQ::new(f, f, f));
                em.insert_component(VelocityQ::new(f, f, f));
                if matches!(archetype, Archetype::Player | Archetype::Vehicle) {
                    em.insert_component(RotationQ::new(0.0, 0.0, 0.0, 1.0));
                }
                id
            };
            self.server.room_mut(&self.room_key).add_entity(&entity);
            self.server_entities.push(entity);
        }
        start..self.server_entities.len()
    }

    /// Mutate the quantized state on every entity in `range`. Mirrors
    /// `mutate_archetype_range` but writes the whole `Property<State>`
    /// per component (matches cyberlith's per-component dirty pattern).
    pub fn mutate_archetype_range_quantized(&mut self, range: std::ops::Range<usize>) {
        for idx in range {
            if idx >= self.server_entities.len() {
                continue;
            }
            let entity = self.server_entities[idx];
            let f = idx as f32;
            if let Some(mut p) = self
                .server
                .entity_mut(self.server_world.proxy_mut(), &entity)
                .component::<PositionQ>()
            {
                *p.state = PositionQState {
                    tile_x: (f as i16).wrapping_add(1),
                    tile_y: (f as i16).wrapping_add(1),
                    tile_z: (f as i16).wrapping_add(1),
                    dx: SignedVariableFloat::new(0.5),
                    dy: SignedVariableFloat::new(0.5),
                    dz: SignedVariableFloat::new(0.5),
                };
            }
            if let Some(mut v) = self
                .server
                .entity_mut(self.server_world.proxy_mut(), &entity)
                .component::<VelocityQ>()
            {
                *v.state = VelocityQState {
                    vx: SignedVariableFloat::new(f + 1.0),
                    vy: SignedVariableFloat::new(f + 1.0),
                    vz: SignedVariableFloat::new(f + 1.0),
                };
            }
            if let Some(mut r) = self
                .server
                .entity_mut(self.server_world.proxy_mut(), &entity)
                .component::<RotationQ>()
            {
                *r.state = BenchQuat::new(0.0, 0.0, 0.0, 1.0);
            }
        }
    }

    /// Drive ticks until all clients have caught up to `target_count` entities.
    /// Used by archetype benches after `spawn_archetype` and *before* the
    /// measured loop, so the steady-state bytes/tick reflects mutations only.
    pub fn replicate_until_caught_up(&mut self, target_count: usize) {
        for _ in 0..REPLICATE_TIMEOUT {
            self.tick();
            let all_visible = self
                .clients
                .iter()
                .all(|(client, world)| client.entities(&world.proxy()).len() >= target_count);
            if all_visible {
                break;
            }
        }
    }

    /// Mutate Position + Velocity (+ Rotation if present) on every entity in
    /// `range`. Each property gets `+= 1.0` to dirty all axes — simulates
    /// continuously-moving units. Mirrors `mutate_entities` for archetype
    /// shapes.
    pub fn mutate_archetype_range(&mut self, range: std::ops::Range<usize>) {
        for idx in range {
            if idx >= self.server_entities.len() {
                continue;
            }
            let entity = self.server_entities[idx];
            if let Some(mut p) = self
                .server
                .entity_mut(self.server_world.proxy_mut(), &entity)
                .component::<Position>()
            {
                *p.x += 1.0;
                *p.y += 1.0;
                *p.z += 1.0;
            }
            if let Some(mut v) = self
                .server
                .entity_mut(self.server_world.proxy_mut(), &entity)
                .component::<Velocity>()
            {
                *v.x += 1.0;
                *v.y += 1.0;
                *v.z += 1.0;
            }
            if let Some(mut r) = self
                .server
                .entity_mut(self.server_world.proxy_mut(), &entity)
                .component::<Rotation>()
            {
                *r.yaw += 1.0;
                *r.pitch += 1.0;
            }
        }
    }

    /// PaintRect-style burst: spawn `n` entities with `components_per_entity`
    /// components each (`Mutable` + optionally `Immutable`) and add them all
    /// to the room — within a single tick boundary, with NO ticks in between.
    /// Used by Phase 6's coalescing audit. The audit assertion is that each
    /// resulting send carries one `SpawnWithComponents` per entity (with all
    /// components inlined), not `Spawn + N×InsertComponent`.
    ///
    /// `components_per_entity` is clamped to `[1, 2]` — the bench protocol
    /// has exactly two component kinds, `BenchComponent` (mutable) and
    /// `BenchImmutableComponent` (immutable). 1 → mutable only; 2 → both.
    pub fn paint_rect_spawn_burst(&mut self, n: usize, components_per_entity: usize) {
        let k = components_per_entity.clamp(1, 2);
        for i in 0..n {
            let entity = {
                let mut entity_mut = self.server.spawn_entity(self.server_world.proxy_mut());
                let entity = entity_mut.id();
                entity_mut.insert_component(BenchComponent::new(i as u32));
                if k >= 2 {
                    entity_mut.insert_component(BenchImmutableComponent);
                }
                entity
            };
            self.server.room_mut(&self.room_key).add_entity(&entity);
            self.server_entities.push(entity);
        }
    }

    /// Have ALL connected clients request authority on server_entities[entity_idx].
    /// Used by authority contention benchmarks to simulate simultaneous requests.
    pub fn request_authority_all_clients(&mut self, entity_idx: usize) {
        if let Some(&entity) = self.server_entities.get(entity_idx) {
            for (client, world) in &mut self.clients {
                let _ = client
                    .entity_mut(world.proxy_mut(), &entity)
                    .request_authority();
            }
        }
    }

    /// Outgoing bytes the server sent during the most recent tick. Precise
    /// (not a rolling average) — reads `Server::outgoing_bytes_last_tick`,
    /// which is reset at the start of each `send_all_packets` and
    /// incremented per sent packet.
    ///
    /// Does NOT require `.with_bandwidth()` on the builder; the counter
    /// is always tracked.
    pub fn server_outgoing_bytes_per_tick(&self) -> u64 {
        self.server.outgoing_bytes_last_tick().max(1)
    }

    /// Number of entities the server currently tracks. Used by archetype
    /// benches to anchor the dynamic-entity index range after spawn.
    pub fn server_entities_len(&self) -> usize {
        self.server_entities.len()
    }

    /// Grant authority on entity[entity_idx] to user[0].
    /// Used by authority benchmarks.
    pub fn give_authority_on_entity(&mut self, entity_idx: usize) {
        if let (Some(&entity), Some(&user_key)) = (
            self.server_entities.get(entity_idx),
            self.connected_user_keys.first(),
        ) {
            let _ = self
                .server
                .entity_mut(self.server_world.proxy_mut(), &entity)
                .give_authority(&user_key);
        }
    }

    /// Remove user[user_idx] from the room. Used for scope-exit benchmarks.
    pub fn remove_user_from_room(&mut self, user_idx: usize) {
        if let Some(&user_key) = self.connected_user_keys.get(user_idx) {
            self.server.room_mut(&self.room_key).remove_user(&user_key);
        }
    }

    // ─── Round-trip helpers ───────────────────────────────────────────────────

    /// Drive ticks until client 0 has a `BenchComponent` with value ≥
    /// `min_value`. Returns `true` if confirmed within `timeout` ticks.
    ///
    /// With local (in-memory) transport the update propagates in exactly one
    /// tick, so this always completes on the first iteration. The measured
    /// latency is the wall time of that one tick including the client's receive
    /// path — the true end-to-end round-trip cost.
    pub fn tick_until_client_entity_updated(&mut self, min_value: u32, timeout: usize) -> bool {
        for _ in 0..timeout {
            advance_tick(
                &self.hub,
                &mut self.server,
                &mut self.server_world,
                &mut self.clients,
                self.tick_ms,
            );
            drain_all_events(&mut self.server, &mut self.clients);
            if let Some((client, world)) = self.clients.first() {
                let confirmed = client.entities(&world.proxy()).iter().any(|entity| {
                    client
                        .entity(world.proxy(), entity)
                        .component::<BenchComponent>()
                        .map(|c| *c.value >= min_value)
                        .unwrap_or(false)
                });
                if confirmed {
                    return true;
                }
            }
        }
        false
    }

    // ─── Resource helpers ─────────────────────────────────────────────────────

    /// Insert a delta-tracked `BenchResource` with initial value 0.
    /// Panics if a resource of this type is already present.
    pub fn insert_resource(&mut self) {
        self.server
            .insert_resource(self.server_world.proxy_mut(), BenchResource::new(0), false)
            .expect("BenchResource already inserted");
    }

    /// Mutate the `BenchResource` value (wrapping add 1).
    /// No-op if the resource is not currently inserted.
    pub fn mutate_resource(&mut self) {
        let Some(entity) = self.server.resource_entity::<BenchResource>() else {
            return;
        };
        if let Some(mut res) = self
            .server
            .entity_mut(self.server_world.proxy_mut(), &entity)
            .component::<BenchResource>()
        {
            *res.value = res.value.wrapping_add(1);
        }
    }

    /// Drive ticks until client 0 has `BenchResource`. Returns `true` if
    /// replicated within `timeout` ticks.
    pub fn tick_until_client_has_resource(&mut self, timeout: usize) -> bool {
        for _ in 0..timeout {
            advance_tick(
                &self.hub,
                &mut self.server,
                &mut self.server_world,
                &mut self.clients,
                self.tick_ms,
            );
            drain_all_events(&mut self.server, &mut self.clients);
            if let Some((client, _)) = self.clients.first() {
                if client.has_resource::<BenchResource>() {
                    return true;
                }
            }
        }
        false
    }

    // ─── Authority cycle helpers ──────────────────────────────────────────────

    /// Server takes authority back on entity[entity_idx].
    /// Used by `authority/cycle` benchmarks.
    pub fn take_authority_on_entity(&mut self, entity_idx: usize) {
        if let Some(&entity) = self.server_entities.get(entity_idx) {
            let _ = self
                .server
                .entity_mut(self.server_world.proxy_mut(), &entity)
                .take_authority();
        }
    }

    /// Drive ticks until client 0 has at least one entity with
    /// `EntityAuthStatus::Granted`. Returns `true` within `timeout` ticks.
    pub fn tick_until_client_auth_granted(&mut self, timeout: usize) -> bool {
        for _ in 0..timeout {
            advance_tick(
                &self.hub,
                &mut self.server,
                &mut self.server_world,
                &mut self.clients,
                self.tick_ms,
            );
            drain_all_events(&mut self.server, &mut self.clients);
            if let Some((client, world)) = self.clients.first() {
                let granted = client.entities(&world.proxy()).iter().any(|entity| {
                    client.entity_authority_status(entity) == Some(EntityAuthStatus::Granted)
                });
                if granted {
                    return true;
                }
            }
        }
        false
    }

    /// Drive ticks until client 0 has NO entity with `EntityAuthStatus::Granted`.
    /// Used to detect that a `take_authority` revocation has propagated.
    pub fn tick_until_client_auth_not_granted(&mut self, timeout: usize) -> bool {
        for _ in 0..timeout {
            advance_tick(
                &self.hub,
                &mut self.server,
                &mut self.server_world,
                &mut self.clients,
                self.tick_ms,
            );
            drain_all_events(&mut self.server, &mut self.clients);
            if let Some((client, world)) = self.clients.first() {
                let none_granted = !client.entities(&world.proxy()).iter().any(|entity| {
                    client.entity_authority_status(entity) == Some(EntityAuthStatus::Granted)
                });
                if none_granted {
                    return true;
                }
            }
        }
        false
    }

    pub fn server_mut(&mut self) -> &mut NaiaServer<BenchEntity> {
        &mut self.server
    }

    pub fn server_world_mut(&mut self) -> &mut World {
        &mut self.server_world
    }

    pub fn hub(&self) -> &LocalTransportHub {
        &self.hub
    }
}

// ─── drain_all_events ─────────────────────────────────────────────────────────

/// Drain server + client events after a tick.
///
/// Benchmarks don't consume events, but Naia accumulates them indefinitely
/// if never read — so we drain every tick to keep memory bounded and keep
/// steady-state costs steady. Cost is O(new_events) (0 for idle ticks).
///
/// Used by `BenchWorld::tick()` and the replication-wait setup loop.
/// The connection-wait setup loop does its own bespoke event consumption
/// (ConnectEvent reads + auth accept) and doesn't call this.
#[inline]
pub fn drain_all_events(
    server: &mut NaiaServer<BenchEntity>,
    clients: &mut Vec<(NaiaClient<BenchEntity>, World)>,
) {
    let mut events = server.take_world_events();
    let _ = server.take_tick_events(&Instant::now());
    let _ = events.take_auths();
    for (client, _) in clients {
        let _ = client.take_world_events();
        let _ = client.take_tick_events(&Instant::now());
    }
}

// ─── advance_tick (free function) ─────────────────────────────────────────────

/// Advance one tick: clock, hub queues, all-clients I/O, then ONE server step.
///
/// Ordering per tick:
///   1. Advance TestClock + flush any time-delayed hub packets.
///   2. Each client: receive → process → send.
///   3. Server: receive → process → send (ONCE, regardless of client count).
///
/// The pre-fix version ran server I/O inside the per-client loop, which
/// executed `update_entity_scopes` and per-connection send K times per tick
/// for K clients — inflating multi-user measurements. Server I/O is global
/// state; one call per tick is the correct semantics.
///
/// Called from `BenchWorld::tick()` (measured) and setup loops (not measured).
pub fn advance_tick(
    hub: &LocalTransportHub,
    server: &mut NaiaServer<BenchEntity>,
    server_world: &mut World,
    clients: &mut [(NaiaClient<BenchEntity>, World)],
    tick_ms: u64,
) {
    TestClock::advance(tick_ms);
    let now = Instant::now();
    hub.process_time_queues();

    for (client, client_world) in clients.iter_mut() {
        client.receive_all_packets();
        client.process_all_packets(client_world.proxy_mut(), &now);
        client.send_all_packets(client_world.proxy_mut());
    }

    server.receive_all_packets();
    server.process_all_packets(server_world.proxy_mut(), &now);
    server.send_all_packets(server_world.proxy());
}
