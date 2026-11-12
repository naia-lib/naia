//! nydus-dm9 (Ada ninth read, Low): when the first copy of a command is
//! lost on the wire, the client applies the resend, and both sides' journals
//! record the command exactly once under the same wire id.
//!
//! The server journals the despawn on its first write; the resend after the
//! client's silence carries the same id and journals nothing new. The client
//! journals the resent despawn under the id it rode in on. Comparing by
//! (kind, id) — never the tick — the two journals must agree: one Despawn
//! each, same id. A double-journaled resend on either side fails the count.

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
use naia_shared::{CauseKind, Instant, TestClock, WorldRefType};
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
        let hub = LocalTransportHub::new("127.0.0.1:54504".parse().unwrap());
        // This link asserts journals, so it enables them on both sides.
        let server_config = ServerConfig {
            cause_evidence: true,
            ..ServerConfig::default()
        };
        let mut server = Server::new(mode, &server_config, protocol());
        server
            .listen(ServerSocket::new(LocalServerSocket::new(hub.clone()), None))
            .expect("server listener must bind");
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
        let client_config = ClientConfig {
            send_handshake_interval: Duration::ZERO,
            jitter_buffer: JitterBufferType::Bypass,
            cause_evidence: true,
            ..ClientConfig::default()
        };
        let mut client = Client::new(client_config, protocol());
        client.auth(Auth::new("resend-link", "test"));
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
        // The bracket form: resident is byte-identical to send_all_packets,
        // and pipelined drains its staged lifecycle ops (the despawn rides
        // D3) through the same call.
        self.server.send(&self.server_world.proxy());
        if deliver {
            self.client.receive_all_packets();
        }
        self.client.take_tick_events(&now);
        self.client
            .process_all_packets(self.client_world.proxy_mut(), &now);
        self.client.take_world_events();
        self.client.send_all_packets(self.client_world.proxy());
    }

    fn seed_replication(&mut self) -> Entity {
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
        entity
    }

    /// Despawn the entity, drop the first copies on the floor, then deliver
    /// until the client applies the resent despawn. Returns the Despawn
    /// entries each side journaled, accumulated across per-step drains so a
    /// slow apply cannot evict them behind the bound before the comparison.
    /// A never-sent despawn reads as an empty server vec, not a lapse.
    fn despawn_losing_first_copies(
        &mut self,
        entity: &Entity,
    ) -> (
        Vec<naia_shared::AppliedCause<naia_shared::GlobalEntity>>,
        Vec<naia_shared::AppliedCause<naia_demo_world::Entity>>,
    ) {
        use naia_shared::GlobalEntity;
        let user = self.user.unwrap();
        self.server
            .entity_mut(self.server_world.proxy_mut(), entity)
            .despawn();
        // The first writes never reach the client; the server still journals
        // the first one and skips the second (same id).
        self.step(false);
        self.step(false);
        // The unacked despawn rides every pass until the client applies it.
        let mut server_despawns: Vec<naia_shared::AppliedCause<GlobalEntity>> = Vec::new();
        let mut client_despawns = Vec::new();
        for _ in 0..101 {
            self.step(true);
            server_despawns.extend(
                self.server
                    .take_transmit_causes(&user)
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|entry| entry.kind() == CauseKind::Despawn),
            );
            client_despawns.extend(
                self.client
                    .take_applied_causes()
                    .into_iter()
                    .filter(|entry| entry.kind() == CauseKind::Despawn),
            );
            if self.client_world.proxy().entities().is_empty() {
                return (server_despawns, client_despawns);
            }
        }
        panic!(
            "the client must apply the resent despawn: server despawns {:?}, client despawns {:?}",
            server_despawns
                .iter()
                .map(|entry| entry.identity().command_id)
                .collect::<Vec<_>>(),
            client_despawns
                .iter()
                .map(|entry| entry.identity().command_id)
                .collect::<Vec<_>>(),
        );
    }
}

fn lost_first_copy_matches_resend_by_id(mode: ServerMode) {
    let mut link = Link::new(mode);
    let entity = link.seed_replication();
    // Drain the seed journals so only the despawn is compared below.
    let user = link.user.unwrap();
    let _ = link.server.take_transmit_causes(&user);
    let _ = link.client.take_applied_causes();
    let (server_despawns, client_despawns) = link.despawn_losing_first_copies(&entity);
    assert_eq!(
        server_despawns.len(),
        1,
        "the server journals the despawn once: the resend is the same cause, not a new entry: {:?}",
        server_despawns
            .iter()
            .map(|entry| (entry.identity().tick, entry.identity().command_id))
            .collect::<Vec<_>>(),
    );
    assert_eq!(
        client_despawns.len(),
        1,
        "the client applies the resent despawn exactly once: {:?}",
        client_despawns
            .iter()
            .map(|entry| (entry.identity().tick, entry.identity().command_id))
            .collect::<Vec<_>>(),
    );
    let server_id = server_despawns[0].identity().command_id;
    let client_id = client_despawns[0].identity().command_id;
    assert!(
        server_id.is_some(),
        "the server journals the despawn under its wire id"
    );
    assert_eq!(
        server_id, client_id,
        "both sides journal the same command under the same wire id"
    );
}

#[test]
fn resident_lost_first_copy_matches_resend_by_id() {
    lost_first_copy_matches_resend_by_id(ServerMode::Resident);
}

#[test]
fn pipelined_lost_first_copy_matches_resend_by_id() {
    lost_first_copy_matches_resend_by_id(ServerMode::Pipelined);
}
