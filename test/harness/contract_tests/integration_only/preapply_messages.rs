//! The client decode/apply boundary, driven through production local transport.
//! An early typed message drain must not advance replicated state or consume
//! unrelated messages, and normal callers must keep their existing delivery.

use std::{sync::Arc, time::Duration};

use naia_client::{
    transport::local::{LocalAddrCell, LocalClientSocket, Socket as ClientSocket},
    Client, ClientConfig, ConnectEvent, InsertComponentEvent, JitterBufferType, MessageEvent,
    SpawnEntityEvent,
};
use naia_demo_world::{Entity, World};
use naia_server::{
    transport::local::{LocalServerSocket, LocalTransportHub, Socket as ServerSocket},
    AuthEvent, ReplicationConfig, Server, ServerConfig, ServerMode, UserKey,
};
use naia_shared::{Instant, TestClock, WorldRefType};
use naia_test_harness::{
    protocol,
    test_protocol::{OrderedChannel, ReliableChannel, TestMessage},
    Auth, Position,
};
use parking_lot::Mutex;

struct Link {
    server: Server<Entity>,
    server_world: World,
    client: Client<Entity>,
    client_world: World,
    user: UserKey,
}

impl Link {
    fn new() -> Self {
        TestClock::init(0);
        let hub = LocalTransportHub::new("127.0.0.1:54491".parse().unwrap());
        let mut server = Server::new(ServerMode::Resident, &ServerConfig::default(), protocol());
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
        client.auth(Auth::new("preapply", "test"));
        client.connect(ClientSocket::new(socket, None));
        let mut server_world = World::default();
        let mut client_world = World::default();
        let mut user = None;
        let mut connected = false;
        for _ in 0..100 {
            TestClock::advance(16);
            let now = Instant::now();
            client.receive_all_packets();
            client.take_tick_events(&now);
            client.process_all_packets(client_world.proxy_mut(), &now);
            connected |= client
                .take_world_events()
                .read::<ConnectEvent>()
                .next()
                .is_some();
            client.send_all_packets(client_world.proxy());
            server.receive_all_packets();
            server.process_all_packets(server_world.proxy_mut(), &now);
            let mut events = server.take_world_events();
            for (key, _) in events.read::<AuthEvent<Auth>>() {
                server.accept_connection(&key);
                user = Some(key);
            }
            server.take_tick_events(&now);
            server.send_all_packets(&server_world.proxy());
            if connected && user.is_some_and(|key| server.user_connection_ready(&key)) {
                break;
            }
        }
        assert!(connected, "the production handshake must finish");
        Self {
            server,
            server_world,
            client,
            client_world,
            user: user.expect("the server must have authenticated the client"),
        }
    }

    fn spawn(&mut self) {
        let room = self.server.create_room().key();
        self.server.room_mut(&room).add_user(&self.user);
        let entity = self
            .server
            .spawn_entity(self.server_world.proxy_mut())
            .configure_replication(ReplicationConfig::public())
            .insert_component(Position::new(7.0, 8.0))
            .enter_room(&room)
            .id();
        self.server.user_scope_mut(&self.user).include(&entity);
    }

    fn send(&mut self) {
        TestClock::advance(16);
        self.server.take_tick_events(&Instant::now());
        self.server.send_all_packets(&self.server_world.proxy());
    }

    fn decode(&mut self) {
        self.client.receive_all_packets();
        self.client.take_tick_events(&Instant::now());
    }

    fn apply(&mut self) -> naia_client::Events<Entity> {
        self.client
            .process_all_packets(self.client_world.proxy_mut(), &Instant::now());
        self.client.take_world_events()
    }

    fn early(&mut self) -> Vec<u32> {
        self.client
            .take_received_messages_of_kind::<OrderedChannel, TestMessage>()
            .into_iter()
            .map(|message| message.value)
            .collect()
    }

    fn queue(&mut self, value: u32) {
        self.server
            .send_message::<OrderedChannel, _>(&self.user, &TestMessage::new(value));
    }
}

#[test]
fn early_drain_is_ordered_once_and_leaves_world_and_unrelated_events_for_apply() {
    let mut link = Link::new();
    link.spawn();
    link.queue(1);
    link.server
        .send_message::<OrderedChannel, _>(&link.user, &Auth::new("unrelated", "message"));
    link.queue(2);
    link.server
        .send_message::<ReliableChannel, _>(&link.user, &TestMessage::new(99));
    link.queue(3);
    link.send();
    assert!(link.early().is_empty(), "socket receipt is not decode");
    link.decode();
    assert_eq!(link.early(), vec![1, 2, 3]);
    assert!(link.early().is_empty());
    assert!(link.client_world.proxy().entities().is_empty());

    let mut events = link.apply();
    assert_eq!(events.read::<SpawnEntityEvent>().count(), 1);
    assert_eq!(events.read::<InsertComponentEvent<Position>>().count(), 1);
    assert_eq!(link.client_world.proxy().entities().len(), 1);
    assert_eq!(
        events
            .read::<MessageEvent<OrderedChannel, TestMessage>>()
            .count(),
        0
    );
    let other: Vec<_> = events
        .read::<MessageEvent<OrderedChannel, Auth>>()
        .collect();
    assert_eq!(other.len(), 1);
    assert_eq!(other[0].username, "unrelated");
    let other_channel: Vec<_> = events
        .read::<MessageEvent<ReliableChannel, TestMessage>>()
        .map(|message| message.value)
        .collect();
    assert_eq!(other_channel, vec![99]);
    assert!(link.early().is_empty());
}

#[test]
fn normal_callers_still_receive_messages_and_world_events_together() {
    let mut link = Link::new();
    link.spawn();
    link.queue(10);
    link.queue(11);
    link.send();
    link.decode();
    assert!(link.client_world.proxy().entities().is_empty());
    let mut events = link.apply();
    let values: Vec<_> = events
        .read::<MessageEvent<OrderedChannel, TestMessage>>()
        .map(|message| message.value)
        .collect();
    assert_eq!(values, vec![10, 11]);
    assert_eq!(events.read::<SpawnEntityEvent>().count(), 1);
    assert_eq!(events.read::<InsertComponentEvent<Position>>().count(), 1);
    assert!(link.early().is_empty());
}

#[test]
fn repeated_early_drains_preserve_unrelated_kind_order_across_packets() {
    let mut link = Link::new();
    for value in 1..=3 {
        link.queue(value);
        link.server.send_message::<OrderedChannel, _>(
            &link.user,
            &Auth::new(&format!("other-{value}"), "message"),
        );
        link.send();
        link.decode();
        assert_eq!(link.early(), vec![value]);
        assert!(link.early().is_empty());
    }
    let mut events = link.apply();
    let names: Vec<_> = events
        .read::<MessageEvent<OrderedChannel, Auth>>()
        .map(|message| message.username)
        .collect();
    assert_eq!(names, vec!["other-1", "other-2", "other-3"]);
    assert_eq!(
        events
            .read::<MessageEvent<OrderedChannel, TestMessage>>()
            .count(),
        0
    );
}
