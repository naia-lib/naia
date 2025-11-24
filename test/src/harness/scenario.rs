use naia_shared::Instant;
use naia_client::Client as NaiaClient;
use naia_server::{Server as NaiaServer, ServerConfig, RoomKey, UserKey, Events};

use crate::{
    TestWorld, Auth, TestEntity, Position, LocalTransportBuilder,
    create_client_socket, create_server_socket, default_client_config,
    complete_handshake_with_name,
};
use naia_shared::WorldRefType;
use crate::helpers::{update_client_server_at, update_all_at};

type Client = NaiaClient<TestEntity>;
type Server = NaiaServer<TestEntity>;

pub struct Scenario {
    now: Instant,
    builder: LocalTransportBuilder,
    server: Server,
    server_world: TestWorld,
    main_room: RoomKey,
    client_a: Option<(Client, TestWorld, UserKey)>,
    client_b: Option<(Client, TestWorld, UserKey)>,
    protocol: naia_shared::Protocol,
}

impl Scenario {
    pub fn new(protocol: naia_shared::Protocol) -> Self {
        let builder = LocalTransportBuilder::default();
        let mut server = Server::new(ServerConfig::default(), protocol.clone());
        let server_socket = create_server_socket(&builder);
        server.listen(server_socket);
        let main_room = server.make_room().key();

        Self {
            now: Instant::now(),
            builder,
            server,
            server_world: TestWorld::default(),
            main_room,
            client_a: None,
            client_b: None,
            protocol,
        }
    }

    pub fn main_room_key(&self) -> &RoomKey {
        &self.main_room
    }

    pub fn server(&mut self) -> (&mut Server, &mut TestWorld) {
        (&mut self.server, &mut self.server_world)
    }

    pub fn connect_client_a(&mut self, name: &str, auth: Auth) -> UserKey {
        let mut client = Client::new(default_client_config(), self.protocol.clone());
        let mut world = TestWorld::default();
        let socket = create_client_socket(&self.builder);
        client.auth(auth);
        client.connect(socket);

        let user_key = complete_handshake_with_name(
            &mut client,
            &mut self.server,
            &mut world,
            &mut self.server_world,
            &self.main_room,
            name,
        )
        .expect("client A should connect");

        self.client_a = Some((client, world, user_key));
        user_key
    }

    pub fn connect_b(&mut self, name: &str, auth: Auth) -> UserKey {
        let mut client = Client::new(default_client_config(), self.protocol.clone());
        let mut world = TestWorld::default();
        let socket = create_client_socket(&self.builder);
        client.auth(auth);
        client.connect(socket);

        let user_key = complete_handshake_with_name(
            &mut client,
            &mut self.server,
            &mut world,
            &mut self.server_world,
            &self.main_room,
            name,
        )
        .expect("client B should connect");

        self.client_b = Some((client, world, user_key));
        user_key
    }

    fn tick_once_1c(&mut self) {
        let now = self.now.clone();
        let (client, world, _) = self.client_a.as_mut().unwrap();
        update_client_server_at(now, client, &mut self.server, world, &mut self.server_world);
        self.now = Instant::now(); // simplest tonight; later you can advance deterministically
    }

    fn tick_once_2c(&mut self) {
        let now = self.now.clone();
        let (ca, wa, _) = self.client_a.as_mut().unwrap();
        let (cb, wb, _) = self.client_b.as_mut().unwrap();
        update_all_at(now, ca, cb, &mut self.server, wa, wb, &mut self.server_world);
        self.now = Instant::now();
    }

    pub fn tick(&mut self, n: usize) {
        for _ in 0..n {
            if self.client_b.is_some() {
                self.tick_once_2c();
            } else {
                self.tick_once_1c();
            }
        }
    }

    pub fn tick_until<F>(&mut self, max: usize, label: &str, mut f: F)
    where
        F: FnMut(&mut Scenario) -> bool,
    {
        for i in 0..max {
            self.tick(1);
            if f(self) {
                return;
            }
            if i == max - 1 {
                panic!("tick_until timeout: {}", label);
            }
        }
    }

    pub fn tick_until_map<T, F>(&mut self, max: usize, label: &str, mut f: F) -> Option<T>
    where
        F: FnMut(&mut Scenario) -> Option<T>,
    {
        for i in 0..max {
            self.tick(1);
            if let Some(result) = f(self) {
                return Some(result);
            }
            if i == max - 1 {
                panic!("tick_until_map timeout: {}", label);
            }
        }
        None
    }

    // Convenience accessors you need immediately:
    pub fn client_a(&mut self) -> (&mut Client, &mut TestWorld) {
        let (c, w, _) = self.client_a.as_mut().unwrap();
        (c, w)
    }

    pub fn client_b(&mut self) -> (&mut Client, &mut TestWorld) {
        let (c, w, _) = self.client_b.as_mut().unwrap();
        (c, w)
    }

    pub fn take_server_events(&mut self) -> Events<TestEntity> {
        self.server.take_world_events()
    }

    pub fn include_in_scope_b(&mut self, server_entity: &TestEntity, user_key_b: &UserKey) {
        self.server.user_scope_mut(user_key_b).include(server_entity);
        self.tick(1); // kick update_entity_scopes
    }

    /// Check if server has at least one entity
    pub fn server_has_entity(&self) -> bool {
        !self.server_world.proxy().entities().is_empty()
    }

    /// Get the first entity from the server world (panics if empty)
    pub fn server_first_entity(&self) -> TestEntity {
        let ents = self.server_world.proxy().entities();
        assert!(!ents.is_empty(), "Server should have at least one entity");
        ents[0]
    }

    /// Get the first entity from client B's world (returns None if empty)
    pub fn client_b_first_entity(&self) -> Option<TestEntity> {
        if let Some((_, world, _)) = &self.client_b {
            world.proxy().entities().get(0).cloned()
        } else {
            None
        }
    }

    /// Check if client B has a specific entity
    pub fn b_has_entity(&self, entity: &TestEntity) -> bool {
        if let Some((_, world, _)) = &self.client_b {
            world.proxy().has_entity(entity)
        } else {
            false
        }
    }

    /// Get the position component of an entity on client A
    pub fn a_entity_position(&self, entity: &TestEntity) -> Option<(f32, f32)> {
        if let Some((_client, world, _)) = &self.client_a {
            world
                .proxy()
                .component::<Position>(entity)
                .map(|p| (*p.x, *p.y))
        } else {
            None
        }
    }

}

