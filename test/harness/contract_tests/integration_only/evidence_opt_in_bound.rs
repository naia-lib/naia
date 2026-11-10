//! nydus-dm9 Ada fourth read (184483): evidence is opt-in (default off) and
//! bounded when on.
//!
//! - Off (default configs): a production server and client allocate nothing
//!   for evidence — no server stream exists (`transmit_coverage` and the
//!   transmit drain read `None`) and the client frontier stays `None` with an
//!   empty drain.
//! - On: over a long run with per-tick entries, both journals stay bounded
//!   (retained entries and gaps within the window behind the frontier), the
//!   lapse is named by the floor, and both sides drain through public
//!   calls (`take_transmit_causes` / `take_applied_causes`) leaving empty
//!   second drains while the frontier keeps advancing.

use std::{sync::Arc, time::Duration};

use naia_client::{
    transport::local::{LocalAddrCell, LocalClientSocket, Socket as ClientSocket},
    Client, ClientConfig, JitterBufferType,
};
use naia_demo_world::{Entity, World};
use naia_server::{
    transport::local::{LocalServerSocket, LocalTransportHub, Socket as ServerSocket},
    AuthEvent, ReplicationConfig, RoomKey, Server, ServerConfig, ServerMode, UserKey,
};
use naia_shared::{AppliedCause, CauseKind, ComponentKind, Instant, TestClock, WorldRefType};
use naia_test_harness::{protocol, Auth, Position, Velocity};
use parking_lot::Mutex;

/// Default evidence window both configs share: history older than this many
/// ticks behind the frontier is evicted, naming the lapse floor.
const WINDOW: u16 = 16;

struct Link {
    server: Server<Entity>,
    server_world: World,
    client: Client<Entity>,
    client_world: World,
    user: Option<UserKey>,
}

impl Link {
    fn new(mode: ServerMode, evidence: bool) -> Self {
        TestClock::init(0);
        let hub = LocalTransportHub::new("127.0.0.1:54503".parse().unwrap());
        let server_config = ServerConfig {
            cause_evidence: evidence,
            cause_evidence_window: WINDOW,
            ..ServerConfig::default()
        };
        let mut server = Server::new(mode, &server_config, protocol());
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
        let client_config = ClientConfig {
            send_handshake_interval: Duration::ZERO,
            jitter_buffer: JitterBufferType::Bypass,
            cause_evidence: evidence,
            cause_evidence_window: WINDOW,
            ..ClientConfig::default()
        };
        let mut client = Client::new(client_config, protocol());
        client.auth(Auth::new("evidence-link", "test"));
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

    fn seed_replication(&mut self) -> RoomKey {
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
        room
    }

    /// Spawn one fresh in-scope entity per step so every send tick serializes
    /// entries (spawn + insert + scope publish) on both sides.
    fn churn(&mut self, room: &RoomKey, steps: usize) {
        for _ in 0..steps {
            let entity = self
                .server
                .spawn_entity(self.server_world.proxy_mut())
                .configure_replication(ReplicationConfig::public())
                .insert_component(Position::new(1.0, 2.0))
                .enter_room(room)
                .id();
            self.server
                .user_scope_mut(&self.user.unwrap())
                .include(&entity);
            self.step(true);
        }
    }
}

fn evidence_off_records_nothing(mode: ServerMode) {
    let mut link = Link::new(mode, false);
    link.seed_replication();
    for _ in 0..40 {
        link.step(true);
    }
    let user = link.user.unwrap();
    // The server materialized no stream: both reads are None.
    assert!(
        link.server.transmit_coverage(&user).is_none(),
        "evidence off must leave no server stream"
    );
    assert!(
        link.server.take_transmit_causes(&user).is_none(),
        "evidence off must leave nothing to drain server-side"
    );
    // The client journal is dark: no frontier, empty drain.
    assert_eq!(
        link.client.applied_coverage().frontier,
        None,
        "evidence off must never mark the client frontier"
    );
    assert!(
        link.client.take_applied_causes().is_empty(),
        "evidence off must leave nothing to drain client-side"
    );
}

#[test]
fn evidence_off_records_nothing_resident() {
    evidence_off_records_nothing(ServerMode::Resident);
}

#[test]
fn evidence_off_records_nothing_pipelined() {
    evidence_off_records_nothing(ServerMode::Pipelined);
}

fn evidence_on_stays_bounded_and_drains(mode: ServerMode) {
    let mut link = Link::new(mode, true);
    let room = link.seed_replication();
    // Sixty entry-bearing ticks plus forty quiet ones: far past the window,
    // so any unbounded growth shows up as hundreds of retained entries.
    link.churn(&room, 60);
    for _ in 0..40 {
        link.step(true);
    }
    let user = link.user.unwrap();

    // Server side: bounded drain, declared lapse, frontier still advancing.
    let server_cov = link
        .server
        .transmit_coverage(&user)
        .expect("evidence on must hold a server stream");
    let frontier = server_cov.frontier.expect("send ticks were transmitted");
    let drained = link
        .server
        .take_transmit_causes(&user)
        .expect("evidence on must drain server-side");
    assert!(
        drained.len() <= 4 * (WINDOW as usize + 1),
        "server journal must stay bounded: {} entries for window {WINDOW}",
        drained.len()
    );
    assert!(
        drained
            .iter()
            .all(|e| frontier.wrapping_sub(e.identity().tick) <= WINDOW),
        "no drained server entry may be older than the window"
    );
    assert!(
        server_cov
            .gaps
            .iter()
            .all(|&g| frontier.wrapping_sub(g) <= WINDOW),
        "no server gap may be older than the window"
    );
    assert!(
        server_cov.lapsed_before.is_some(),
        "evicted history must name the lapse floor, never stay silent"
    );
    assert!(
        link.server
            .take_transmit_causes(&user)
            .expect("stream survives a drain")
            .is_empty(),
        "second server drain must be empty"
    );

    // Client side: same bound, same declared lapse, frontier on the applied
    // timeline with an empty second drain.
    let client_cov = link.client.applied_coverage();
    let applied = client_cov.frontier.expect("client applied ticks");
    let client_drained = link.client.take_applied_causes();
    assert!(
        client_drained.len() <= 4 * (WINDOW as usize + 1),
        "client journal must stay bounded: {} entries for window {WINDOW}",
        client_drained.len()
    );
    assert!(
        client_drained
            .iter()
            .all(|e| applied.wrapping_sub(e.identity().tick) <= WINDOW),
        "no drained client entry may be older than the window"
    );
    assert!(
        client_cov
            .gaps
            .iter()
            .all(|&g| applied.wrapping_sub(g) <= WINDOW),
        "no client gap may be older than the window"
    );
    assert!(
        client_cov.lapsed_before.is_some(),
        "evicted history must name the lapse floor, never stay silent"
    );
    assert!(
        link.client.take_applied_causes().is_empty(),
        "second client drain must be empty"
    );
}

#[test]
fn evidence_on_stays_bounded_and_drains_resident() {
    evidence_on_stays_bounded_and_drains(ServerMode::Resident);
}

#[test]
fn evidence_on_stays_bounded_and_drains_pipelined() {
    evidence_on_stays_bounded_and_drains(ServerMode::Pipelined);
}

/// nydus-cae: a same-window insert+remove must journal identically on both
/// sides. The insert nets to a wire Noop (collapse-to-final), so the server
/// journal must not hold a Remove the client never applies: the client only
/// journals removals it actually performs.
fn same_window_insert_remove_journals_agree(mode: ServerMode) {
    let mut link = Link::new(mode, true);
    let _room = link.seed_replication();
    let user = link.user.unwrap();
    // Clear the seed baseline from both journals: only the insert/remove
    // window below may contribute entries.
    let _ = link
        .server
        .take_transmit_causes(&user)
        .expect("evidence on must drain server-side");
    let _ = link.client.take_applied_causes();

    // Same-tick insert + remove: both ops land before the next send, so
    // replication for that tick nets to nothing for Velocity.
    let entity = link
        .server_world
        .proxy()
        .entities()
        .pop()
        .expect("one seeded entity");

    // The netted window: insert + remove within one tick. Waits stay far
    // under the evidence window (16) so entries cannot lapse before drain.
    link.server
        .entity_mut(link.server_world.proxy_mut(), &entity)
        .insert_component(Velocity::new(7.0, 8.0));
    let _ = link
        .server
        .entity_mut(link.server_world.proxy_mut(), &entity)
        .remove_component::<Velocity>();

    for _ in 0..5 {
        link.step(true);
    }

    // One entity ever exists, so every entry in the window is its own:
    // compare (kind, component, wire id) multisets across the two journals.
    let server_entries = project_entries(
        &link
            .server
            .take_transmit_causes(&user)
            .expect("evidence on must drain server-side"),
    );
    let client_entries = project_entries(&link.client.take_applied_causes());
    assert_eq!(
        server_entries, client_entries,
        "server and client journals disagree after a same-window insert+remove: server {server_entries:?} vs client {client_entries:?}",
    );
}

/// nydus-3nl: a retransmitted insert must not arm the netted marker.
/// Insert, delay the first copy (its ack arrives too late, so the insert
/// resends), remove before the resend, then deliver: the insert's resend
/// rides beside the genuine remove. The server must journal both, like the
/// client does.
/// Resident writes the insert for real on the first pass, so both sides hold
/// the pair. Pipelined stages the insert into D-slots, so the remove nets it
/// before any real write and both sides hold nothing. Either way the two
/// journals must agree.
fn resent_insert_beside_remove_journals_agree(mode: ServerMode, expect_insert: bool) {
    let mut link = Link::new(mode, true);
    let _room = link.seed_replication();
    let user = link.user.unwrap();
    // Drain the seed journals so only the insert/remove window is compared.
    let _ = link
        .server
        .take_transmit_causes(&user)
        .expect("evidence on must drain server-side");
    let _ = link.client.take_applied_causes();
    let entity = link
        .server_world
        .proxy()
        .entities()
        .pop()
        .expect("one seeded entity");
    let velocity_kind = ComponentKind::of::<Velocity>();

    link.server
        .entity_mut(link.server_world.proxy_mut(), &entity)
        .insert_component(Velocity::new(7.0, 8.0));
    // The first copy is delayed past the ack window, so the insert resends
    // in the next packet — before the delayed copy's ack can return.
    link.step(false);
    // Removed before the insert is ever acked.
    let _ = link
        .server
        .entity_mut(link.server_world.proxy_mut(), &entity)
        .remove_component::<Velocity>();

    // Accumulate per-step drains so a slow apply cannot lapse entries behind
    // the evidence window (16) before the comparison.
    let mut server_accumulated = Vec::new();
    let mut client_accumulated = Vec::new();
    for _ in 0..60 {
        link.step(true);
        server_accumulated.extend(
            link.server
                .take_transmit_causes(&user)
                .expect("evidence on must drain server-side")
                .into_iter()
                .filter(|entry| entry.identity().component_kind == Some(velocity_kind)),
        );
        client_accumulated.extend(
            link.client
                .take_applied_causes()
                .into_iter()
                .filter(|entry| entry.identity().component_kind == Some(velocity_kind)),
        );
        if client_accumulated
            .iter()
            .any(|entry: &AppliedCause<_>| entry.kind() == CauseKind::Insert)
            && client_accumulated
                .iter()
                .any(|entry: &AppliedCause<_>| entry.kind() == CauseKind::Remove)
        {
            break;
        }
    }

    let server_entries = project_entries(&server_accumulated);
    let client_entries = project_entries(&client_accumulated);
    assert_eq!(
        expect_insert,
        server_entries
            .iter()
            .any(|entry| entry.starts_with("Insert")),
        "server journal insert presence must match the engine shape: {server_entries:?}",
    );
    assert_eq!(
        server_entries, client_entries,
        "server and client journals disagree after a resend beside a genuine remove: server {server_entries:?} vs client {client_entries:?}",
    );
}

#[test]
fn resent_insert_beside_remove_journals_agree_resident() {
    resent_insert_beside_remove_journals_agree(ServerMode::Resident, true);
}

#[test]
fn resent_insert_beside_remove_journals_agree_pipelined() {
    resent_insert_beside_remove_journals_agree(ServerMode::Pipelined, false);
}

/// Compare (kind, component, wire id) multisets across the two journals.
fn project_entries<E: Copy>(entries: &[AppliedCause<E>]) -> Vec<String> {
    let mut projected: Vec<String> = entries
        .iter()
        .map(|entry| {
            let identity = entry.identity();
            format!(
                "{:?} {:?} {:?}",
                entry.kind(),
                identity.component_kind,
                identity.command_id
            )
        })
        .collect();
    projected.sort();
    projected
}

#[test]
fn same_window_insert_remove_journals_agree_resident() {
    same_window_insert_remove_journals_agree(ServerMode::Resident);
}

#[test]
fn same_window_insert_remove_journals_agree_pipelined() {
    same_window_insert_remove_journals_agree(ServerMode::Pipelined);
}
