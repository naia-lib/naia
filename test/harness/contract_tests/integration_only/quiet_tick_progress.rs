//! Idle authoritative state must keep confirming through Data, without allowing
//! the client to confirm beyond the Data it has actually received.

use std::{sync::Arc, time::Duration};

use naia_client::{
    transport::local::{LocalAddrCell, LocalClientSocket, Socket as ClientSocket},
    Client, ClientConfig, JitterBufferType,
};
use naia_demo_world::{Entity, World};
use naia_server::{
    transport::local::{LocalServerSocket, LocalTransportHub, Socket as ServerSocket},
    AuthEvent, ReplicationConfig, Server, ServerConfig, ServerMode, UserKey,
};
use naia_shared::{sequence_greater_than, Instant, TestClock, WorldRefType};
use naia_test_harness::{protocol, Auth, Position};
use parking_lot::Mutex;

struct Link {
    server: Server<Entity>,
    server_world: World,
    client: Client<Entity>,
    client_world: World,
    user: Option<UserKey>,
}

impl Link {
    fn new(mode: ServerMode) -> Self {
        TestClock::init(0);
        let hub = LocalTransportHub::new("127.0.0.1:54502".parse().unwrap());
        let mut server = Server::new(mode, &ServerConfig::default(), protocol());
        server.listen(ServerSocket::new(LocalServerSocket::new(hub.clone()), None));
        let (address, auth_tx, auth_rx, data_tx, data_rx) = hub.register_client();
        let addr_cell = LocalAddrCell::new();
        addr_cell.set_sync(hub.server_addr());
        let socket = LocalClientSocket::new_with_tokens(
            address,
            hub.server_addr(),
            auth_tx,
            auth_rx,
            data_tx,
            data_rx,
            addr_cell,
            Arc::new(Mutex::new(None)),
            Arc::new(Mutex::new(None)),
        );
        let mut client = Client::new(
            ClientConfig {
                send_handshake_interval: Duration::ZERO,
                jitter_buffer: JitterBufferType::Bypass,
                ..ClientConfig::default()
            },
            protocol(),
        );
        client.auth(Auth::new("quiet-link", "test"));
        client.connect(ClientSocket::new(socket, None));
        let mut link = Self {
            server,
            server_world: World::default(),
            client,
            client_world: World::default(),
            user: None,
        };
        for _ in 0..100 {
            link.step(true);
            if link.client.connection_status().is_connected()
                && link
                    .user
                    .is_some_and(|key| link.server.user_connection_ready(&key))
            {
                return link;
            }
        }
        panic!("the production local handshake must finish");
    }

    fn step(&mut self, deliver: bool) {
        TestClock::advance(50);
        let now = Instant::now();
        self.server.receive_all_packets();
        self.server
            .process_all_packets(self.server_world.proxy_mut(), &now);
        let mut events = self.server.take_world_events();
        for (key, _) in events.read::<AuthEvent<Auth>>() {
            self.server.accept_connection(&key);
            self.user = Some(key);
        }
        self.server.take_tick_events(&now);
        self.server.send_all_packets(&self.server_world.proxy());
        if deliver {
            self.client.receive_all_packets();
        }
        self.client.take_tick_events(&now);
        self.client
            .process_all_packets(self.client_world.proxy_mut(), &now);
        self.client.take_world_events();
        self.client.send_all_packets(self.client_world.proxy());
    }

    fn seed_replication(&mut self) {
        let room = self.server.create_room().key();
        self.server.room_mut(&room).add_user(&self.user.unwrap());
        let entity = self
            .server
            .spawn_entity(self.server_world.proxy_mut())
            .configure_replication(ReplicationConfig::public())
            .insert_component(Position::new(7.0, 8.0))
            .enter_room(&room)
            .id();
        self.server
            .user_scope_mut(&self.user.unwrap())
            .include(&entity);
        for _ in 0..80 {
            self.step(true);
        }
        assert_eq!(self.client_world.proxy().entities().len(), 1);
    }
}

fn assert_quiet_progress_and_receive_ceiling(mode: ServerMode) {
    let mut link = Link::new(mode);
    link.seed_replication();
    let before = link.client.server_tick().unwrap();
    let data_before = link.client.last_received_server_tick().unwrap();
    for _ in 0..40 {
        link.step(true);
    }
    let after = link.client.server_tick().unwrap();
    assert!(
        sequence_greater_than(after, before.wrapping_add(20)),
        "unchanged replicated state must keep confirming: before={before}, after={after}"
    );
    assert!(sequence_greater_than(
        link.client.last_received_server_tick().unwrap(),
        data_before
    ));

    let ceiling = link.client.last_received_server_tick().unwrap();
    for _ in 0..20 {
        link.step(false);
        assert_eq!(link.client.last_received_server_tick(), Some(ceiling));
        assert!(
            !sequence_greater_than(link.client.server_tick().unwrap(), ceiling),
            "clock progress cannot confirm undelivered Data"
        );
    }
    for _ in 0..40 {
        link.step(true);
    }
    assert!(sequence_greater_than(
        link.client.server_tick().unwrap(),
        ceiling.wrapping_add(20)
    ));
    assert_eq!(link.client_world.proxy().entities().len(), 1);
}

#[test]
fn quiet_resident_link_confirms_without_crossing_undelivered_data() {
    assert_quiet_progress_and_receive_ceiling(ServerMode::Resident);
}

#[test]
fn quiet_pipelined_link_confirms_without_crossing_undelivered_data() {
    assert_quiet_progress_and_receive_ceiling(ServerMode::Pipelined);
}
