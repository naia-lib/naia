pub mod bench_protocol;

use std::{net::SocketAddr, sync::Arc, time::Duration};

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
    bench_protocol, BenchAuth, BenchComponent, BenchEntity, BenchImmutableComponent,
};

const TICK_MS: u64 = 16;
const SETUP_TIMEOUT: usize = 500;
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

// ─── BenchWorldBuilder ────────────────────────────────────────────────────────

/// Fluent builder for [`BenchWorld`]. Call from `iter_batched` setup closures —
/// this is never measured.
pub struct BenchWorldBuilder {
    user_count: usize,
    entity_count: usize,
    entity_kind: EntityKind,
    bandwidth_enabled: bool,
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
            bandwidth_enabled: false,
        }
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

    /// Enable bandwidth monitoring. Required to call `server_outgoing_bytes_per_tick()`.
    pub fn with_bandwidth(mut self) -> Self {
        self.bandwidth_enabled = true;
        self
    }

    /// Build to steady-state. Not measured — call from `iter_batched` setup.
    pub fn build(self) -> BenchWorld {
        BenchWorld::new(self.user_count, self.entity_count, self.entity_kind, self.bandwidth_enabled)
    }
}

// ─── BenchWorld ───────────────────────────────────────────────────────────────

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
}

impl BenchWorld {
    fn new(user_count: usize, entity_count: usize, entity_kind: EntityKind, bandwidth_enabled: bool) -> Self {
        TestClock::init(0);

        let server_addr: SocketAddr = FAKE_SERVER_ADDR.parse().expect("invalid addr");
        let hub = LocalTransportHub::new(server_addr);

        let bw_duration = if bandwidth_enabled {
            Some(Duration::from_secs(1))
        } else {
            None
        };

        let protocol = bench_protocol();
        let mut server_config = ServerConfig::default();
        server_config.connection.bandwidth_measure_duration = bw_duration;
        let mut server: NaiaServer<BenchEntity> =
            NaiaServer::new(server_config, protocol.clone());
        server.listen(ServerSocket::new(
            LocalServerSocket::new(hub.clone()),
            None,
        ));

        let mut server_world = World::default();

        let mut client_config = ClientConfig::default();
        client_config.connection.bandwidth_measure_duration = bw_duration;

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
            advance_tick(&hub, &mut server, &mut server_world, &mut clients);

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
                    room_key = Some(server.make_room().key());
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

        // Spawn entities and add to room
        let mut server_entities: Vec<BenchEntity> = Vec::new();
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
            server.room_mut(&room_key).add_entity(&entity);
            server_entities.push(entity);
        }

        // Run until all entities replicated to all clients
        if entity_count > 0 && user_count > 0 {
            for _ in 0..REPLICATE_TIMEOUT {
                advance_tick(&hub, &mut server, &mut server_world, &mut clients);
                let mut events = server.take_world_events();
                let _ = server.take_tick_events(&Instant::now());
                let _ = events.take_auths();
                for (client, _) in &mut clients {
                    let _ = client.take_world_events();
                    let _ = client.take_tick_events(&Instant::now());
                }

                let all_visible = clients.iter().all(|(client, world)| {
                    client.entities(&world.proxy()).len() >= entity_count
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
        );
        // Drain events — O(new_events), which is 0 for idle ticks.
        let mut events = self.server.take_world_events();
        let _ = self.server.take_tick_events(&Instant::now());
        let _ = events.take_auths();
        for (client, _) in &mut self.clients {
            let _ = client.take_world_events();
            let _ = client.take_tick_events(&Instant::now());
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

    /// Outgoing bytes the server sent in the last bandwidth-measurement window,
    /// normalised to one tick. Requires `.with_bandwidth()` on the builder.
    ///
    /// Formula: kbps × TICK_MS / 8 = bytes/tick
    pub fn server_outgoing_bytes_per_tick(&self) -> u64 {
        let kbps = self.server.outgoing_bandwidth_total();
        (kbps * TICK_MS as f32 / 8.0).max(1.0) as u64
    }

    /// Grant authority on entity[entity_idx] to user[0].
    /// Used by authority benchmarks.
    pub fn give_authority_on_entity(&mut self, entity_idx: usize) {
        if let (Some(&entity), Some(&user_key)) = (
            self.server_entities.get(entity_idx),
            self.connected_user_keys.get(0),
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

// ─── advance_tick (free function) ─────────────────────────────────────────────

/// Advance one tick: clock, hub queues, then client+server I/O pairs.
///
/// Called from both `BenchWorld::tick()` (measured) and setup loops (not measured).
pub fn advance_tick(
    hub: &LocalTransportHub,
    server: &mut NaiaServer<BenchEntity>,
    server_world: &mut World,
    clients: &mut Vec<(NaiaClient<BenchEntity>, World)>,
) {
    TestClock::advance(TICK_MS);
    let now = Instant::now();
    hub.process_time_queues();

    for (client, client_world) in clients.iter_mut() {
        client.receive_all_packets();
        client.process_all_packets(client_world.proxy_mut(), &now);
        client.send_all_packets(client_world.proxy_mut());

        server.receive_all_packets();
        server.process_all_packets(server_world.proxy_mut(), &now);
        server.send_all_packets(server_world.proxy());
    }
}
