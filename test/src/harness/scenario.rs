use std::collections::HashMap;

use naia_shared::{TestClock, Instant};
use naia_client::Client as NaiaClient;
use naia_server::{Server as NaiaServer, ServerConfig, RoomKey, UserKey, Events};

use crate::{
    TestWorld, Auth, TestEntity, LocalTransportBuilder,
    create_client_socket, create_server_socket, default_client_config,
    complete_handshake_with_name,
};
use crate::helpers::{update_client_server_at, update_all_at};

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

        client_key
    }

    pub fn main_room_key(&self) -> Option<&RoomKey> {
        self.main_room.as_ref()
    }

    // Internal helper methods for context types
    pub(crate) fn server(&self) -> &Server {
        self.server.as_ref().expect("server not started")
    }

    pub(crate) fn server_mut(&mut self) -> &mut Server {
        self.server.as_mut().expect("server not started")
    }

    pub(crate) fn server_world(&self) -> &TestWorld {
        &self.server_world
    }

    pub(crate) fn server_world_mut(&mut self) -> &mut TestWorld {
        &mut self.server_world
    }

    pub(crate) fn client_state_mut(&mut self, client_key: ClientKey) -> &mut ClientState {
        self.clients.get_mut(&client_key).expect("client not found")
    }

    pub(crate) fn entity_registry_mut(&mut self) -> &mut EntityRegistry {
        &mut self.entity_registry
    }

    /// Get server entity ID for a LocalEntity and UserKey
    pub(crate) fn server_entity_for_local(&self, user_key: UserKey, local_entity: &naia_shared::LocalEntity) -> Option<TestEntity> {
        let server = self.server.as_ref()?;
        let server_ref = server.local_entity(self.server_world.proxy(), &user_key, local_entity);
        Some(server_ref.id())
    }

    /// Configure entity replication config (helper to avoid borrow conflicts)
    pub(crate) fn configure_entity_replication(
        &mut self,
        entity: &TestEntity,
        config: naia_server::ReplicationConfig,
    ) {
        let server = self.server.as_mut().expect("server not started");
        let current_config = server.entity_replication_config(entity);
        if current_config != Some(config) {
            let world_mut = &mut self.server_world;
            let mut proxy = world_mut.proxy_mut();
            server.configure_entity_replication(&mut proxy, entity, config);
        }
    }

    pub(crate) fn entity_registry(&self) -> &EntityRegistry {
        &self.entity_registry
    }

    /// Tick the simulation once - updates all clients and server
    pub(crate) fn tick_once(&mut self) {
        // Advance simulated clock by 16ms (default tick duration for ~60 FPS)
        TestClock::advance(16);
        
        // Use current time for this tick (we update self.now at the end)
        let now = Instant::now();
        let server = self.server.as_mut().expect("server not started");
        let client_count = self.clients.len();

        // Handle different client counts explicitly
        match client_count {
            0 => {
                // Just update server
                server.receive_all_packets();
                server.take_tick_events(&now);
                server.process_all_packets(self.server_world.proxy_mut(), &now);
                server.send_all_packets(self.server_world.proxy());
            }
            1 => {
                let state = self.clients.values_mut().next().unwrap();
                update_client_server_at(
                    now,
                    &mut state.client,
                    server,
                    &mut state.world,
                    &mut self.server_world,
                );
            }
            2 => {
                let mut iter = self.clients.values_mut();
                let state_a = iter.next().unwrap();
                let state_b = iter.next().unwrap();
                update_all_at(
                    now,
                    &mut state_a.client,
                    &mut state_b.client,
                    server,
                    &mut state_a.world,
                    &mut state_b.world,
                    &mut self.server_world,
                );
            }
            _ => {
                // For 3+ clients, update each client-server pair sequentially
                // This is not ideal but works for now
                // Note: We use Instant::now() for each iteration since now is moved
                for state in self.clients.values_mut() {
                    let iter_now = Instant::now();
                    update_client_server_at(
                        iter_now,
                        &mut state.client,
                        server,
                        &mut state.world,
                        &mut self.server_world,
                    );
                }
            }
        }

        self.now = Instant::now();
    }

    pub fn tick(&mut self, n: usize) {
        for _ in 0..n {
            self.tick_once();
        }
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
    pub(crate) fn local_entity_for(&self, entity_key: EntityKey, user_key: UserKey) -> Option<naia_shared::LocalEntity> {
        let host_entity = self.entity_registry.host_world(entity_key)?;
        let server = self.server.as_ref()?;
        let host_ref = server.entity(self.server_world.proxy(), &host_entity);
        Some(host_ref.local_entity(&user_key))
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
}

