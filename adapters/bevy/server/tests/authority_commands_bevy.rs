//! Bevy-app integration tests for entity authority commands (P1.6 / P1.7).
//!
//! Verifies that `CommandsExt::give_authority` and `CommandsExt::take_authority`
//! work end-to-end through the Bevy adapter — covering the `give_authority`
//! path that was previously a `todo!()`.
//!
//! Coverage:
//! - **A1**: `give_authority` via EntityCommands propagates Granted to the client.
//! - **A2**: `take_authority` via EntityCommands propagates Denied to the client.

use std::{sync::Arc, time::Duration};

use bevy_app::{App, Startup, Update};
use bevy_ecs::{
    entity::Entity,
    message::Messages,
    resource::Resource,
    schedule::IntoScheduleConfigs,
    system::{Commands, ResMut},
};
use parking_lot::Mutex;

use naia_bevy_client::{
    events::{ConnectEvent as ClientConnectEvent, EntityAuthDeniedEvent, EntityAuthGrantedEvent},
    Client, ClientConfig, Plugin as ClientPlugin,
};
use naia_bevy_server::{
    events::{AuthEvents, ConnectEvent},
    CommandsExt, Plugin as ServerPlugin, Server, ServerConfig,
};
use naia_bevy_shared::Protocol as BevyProtocol;
use naia_client::transport::local::{
    LocalAddrCell, LocalClientSocket, Socket as ClientSocket,
};
use naia_server::transport::local::{LocalServerSocket, Socket as ServerSocket};
use naia_shared::{
    transport::local::LocalTransportHub, ChannelDirection, ChannelMode, ReliableSettings,
};
use naia_test_harness::test_protocol::{Auth, Position, ReliableChannel};

const SERVER_ADDR_A1: &str = "127.0.0.1:14195";
const SERVER_ADDR_A2: &str = "127.0.0.1:14196";

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Main;

fn protocol() -> BevyProtocol {
    let mut p = BevyProtocol::builder();
    p.add_message::<Auth>()
        .add_channel::<ReliableChannel>(
            ChannelDirection::Bidirectional,
            ChannelMode::UnorderedReliable(ReliableSettings::default()),
        )
        .add_component::<Position>();
    p.tick_interval(Duration::from_micros(100));
    p.build()
}

// ── Server state ─────────────────────────────────────────────────────────────

#[derive(Resource, Default)]
struct ServerState {
    room_key: Option<naia_server::RoomKey>,
    entity: Option<Entity>,
    // False until the first Update tick applies configure_replication(Delegated).
    delegation_configured: bool,
    connected_user: Option<naia_server::UserKey>,
    // true → give_authority on connect; false → take_authority on connect
    give_mode: bool,
    // Set on connect tick; authority is applied the NEXT tick so the entity
    // spawn packet goes out separately from the SetAuth packet.  If both were
    // sent in the same SendPackets burst the client would process SpawnEntity
    // (Position Added) and SetAuth(Granted) / HostOwned insert in the same
    // Bevy tick, keeping Added<Position>=true when on_component_added fires
    // and causing insert_component_worldless to double-register Position.
    pending_authority: bool,
}

// ── Client state ─────────────────────────────────────────────────────────────

#[derive(Resource, Default)]
struct ClientState {
    connected: bool,
    auth_grant_count: u32,
    auth_denied_count: u32,
}

// ── Server systems ────────────────────────────────────────────────────────────

/// Startup: listen, make a room, spawn Position entity with replication enabled.
/// Delegation is NOT configured here — naia's tracking system hasn't run yet so
/// the Property SyncMutators aren't ready; we defer to the first Update tick.
fn sys_server_startup(
    mut commands: Commands,
    mut server: Server,
    mut state: ResMut<ServerState>,
) {
    let room_key = server.make_room().key();
    state.room_key = Some(room_key);

    let entity = commands
        .spawn(Position::new(0.0, 0.0))
        .enable_replication(&mut server)
        .id();
    // Entity must be in a room so user_scope_has_entity returns true for
    // explicit include() targets (entity-scopes-09: roomless server-owned
    // non-resource entities are excluded even with explicit include).
    server.room_mut(&room_key).add_entity(&entity);
    state.entity = Some(entity);
}

/// Update: configure delegation exactly once, AFTER naia tracking has
/// set up the entity's Property SyncMutators (first Update tick).
fn sys_server_configure_delegation(
    mut commands: Commands,
    mut state: ResMut<ServerState>,
) {
    if state.delegation_configured {
        return;
    }
    let Some(entity) = state.entity else {
        return;
    };
    commands
        .entity(entity)
        .configure_replication(naia_bevy_server::ReplicationConfig::delegated());
    state.delegation_configured = true;
}

fn sys_server_auth(mut server: Server, mut auth_msgs: ResMut<Messages<AuthEvents>>) {
    for events in auth_msgs.drain() {
        for (user_key, _) in events.read::<Auth>() {
            server.accept_connection(&user_key);
        }
    }
}

/// On connect: put the user in the room so the entity enters scope.
/// Authority is NOT applied here — see sys_server_apply_authority below.
/// Reason: giving authority in the same tick as scope-entry causes the server
/// to send SpawnEntity and SetAuth in the same SendPackets burst.  The client
/// then processes both in the same Bevy tick, keeping Added<Position>=true
/// when HostOwned is inserted, which triggers on_component_added::<Position>
/// to fire and double-registers Position in the global entity record.
fn sys_server_connect(
    mut server: Server,
    mut connect_msgs: ResMut<Messages<ConnectEvent>>,
    mut state: ResMut<ServerState>,
) {
    for event in connect_msgs.drain() {
        let user_key = event.0;
        state.connected_user = Some(user_key);
        let Some(room_key) = state.room_key else {
            continue;
        };
        server.user_mut(&user_key).enter_room(&room_key);
        state.pending_authority = true;
    }
}

/// One tick after the client enters scope, apply give/take authority so that
/// the spawn packet and the SetAuth packet go out in separate SendPackets bursts.
fn sys_server_apply_authority(
    mut commands: Commands,
    mut server: Server,
    mut state: ResMut<ServerState>,
) {
    if !state.pending_authority {
        return;
    }
    let (Some(user_key), Some(entity)) = (state.connected_user, state.entity) else {
        return;
    };
    state.pending_authority = false;
    if state.give_mode {
        commands
            .entity(entity)
            .give_authority(&mut server, &user_key);
    } else {
        commands.entity(entity).take_authority(&mut server);
    }
}

// ── Client systems ────────────────────────────────────────────────────────────

fn sys_client_connect(
    mut connect_msgs: ResMut<Messages<ClientConnectEvent<Main>>>,
    mut state: ResMut<ClientState>,
) {
    for _ in connect_msgs.drain() {
        state.connected = true;
    }
}

fn sys_client_count_auth_events(
    mut granted: ResMut<Messages<EntityAuthGrantedEvent<Main>>>,
    mut denied: ResMut<Messages<EntityAuthDeniedEvent<Main>>>,
    mut state: ResMut<ClientState>,
) {
    state.auth_grant_count += granted.drain().count() as u32;
    state.auth_denied_count += denied.drain().count() as u32;
}

// ── Harness ───────────────────────────────────────────────────────────────────

struct BevyHarness {
    server_app: App,
    client_app: App,
}

impl BevyHarness {
    fn new_give(server_addr_str: &str) -> Self {
        Self::new_inner(server_addr_str, true)
    }

    fn new_take(server_addr_str: &str) -> Self {
        Self::new_inner(server_addr_str, false)
    }

    fn new_inner(server_addr_str: &str, give_mode: bool) -> Self {
        let server_addr = server_addr_str.parse().expect("addr");
        let hub = LocalTransportHub::new(server_addr);

        // ── Server App ────────────────────────────────────────────────────
        let hub_for_server = hub.clone();
        let mut server_app = App::new();
        server_app.add_plugins(ServerPlugin::new(ServerConfig::default(), protocol()));
        server_app
            .insert_resource(ServerState {
                give_mode,
                ..Default::default()
            })
            .add_systems(
                Startup,
                (
                    move |mut server: Server| {
                        let socket = ServerSocket::new(
                            LocalServerSocket::new(hub_for_server.clone()),
                            None,
                        );
                        server.listen(socket);
                    },
                    sys_server_startup,
                )
                    .chain(),
            )
            // configure_delegation must queue its WorldOpCommand AFTER WorldToHostSync
            // (which sets Property SyncMutators via world_to_host_sync).
            // Bevy 0.18 auto_insert_apply_deferred flushes commands at set
            // boundaries, so queuing in HostSyncChangeTracking causes the
            // WorldOpCommand to apply before SyncMutators are ready → panic.
            // SendPackets runs after WorldToHostSync, so by the time the
            // auto-deferred flush applies the command, SyncMutators are set.
            .add_systems(
                Update,
                sys_server_configure_delegation.in_set(naia_bevy_shared::SendPackets),
            )
            // apply_authority runs BEFORE connect so the pending flag is only
            // consumed in the tick AFTER it was set by connect.  This ensures
            // the entity spawn packet (sent tick N) and the SetAuth packet
            // (sent tick N+1) are in separate SendPackets bursts and reach the
            // client in separate ticks, preventing Added<Position> from being
            // true when HostOwned is inserted.
            .add_systems(
                Update,
                (sys_server_auth, sys_server_apply_authority, sys_server_connect)
                    .chain()
                    .in_set(naia_bevy_shared::HandleWorldEvents),
            );
        server_app.update();

        // ── Client App ────────────────────────────────────────────────────
        let hub_for_client = hub.clone();
        let mut client_app = App::new();
        let mut cfg = ClientConfig::default();
        cfg.send_handshake_interval = Duration::from_millis(0);
        client_app.add_plugins(ClientPlugin::<Main>::new(cfg, protocol()));
        client_app
            .init_resource::<ClientState>()
            .add_systems(
                Startup,
                move |mut client: Client<Main>| {
                    let (
                        client_addr,
                        auth_req_tx,
                        auth_resp_rx,
                        client_data_tx,
                        client_data_rx,
                    ) = hub_for_client.register_client();
                    let addr_cell = LocalAddrCell::new();
                    addr_cell.set_sync(hub_for_client.server_addr());
                    let identity_token = Arc::new(Mutex::new(None::<naia_shared::IdentityToken>));
                    let rejection_code = Arc::new(Mutex::new(None::<u16>));
                    let inner = LocalClientSocket::new_with_tokens(
                        client_addr,
                        hub_for_client.server_addr(),
                        auth_req_tx,
                        auth_resp_rx,
                        client_data_tx,
                        client_data_rx,
                        addr_cell,
                        identity_token,
                        rejection_code,
                    );
                    let socket = ClientSocket::new(inner, None);
                    client.auth(Auth::new("alice", "pw"));
                    client.connect(socket);
                },
            )
            .add_systems(
                Update,
                (sys_client_connect, sys_client_count_auth_events)
                    .in_set(naia_bevy_shared::HandleWorldEvents),
            );
        client_app.update();

        Self {
            server_app,
            client_app,
        }
    }

    fn tick(&mut self) {
        naia_bevy_shared::TestClock::advance(60);
        self.server_app.update();
        self.client_app.update();
    }

    fn tick_n(&mut self, n: u32) {
        for _ in 0..n {
            self.tick();
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[test]
fn a1_give_authority_propagates_granted_to_client() {
    let mut h = BevyHarness::new_give(SERVER_ADDR_A1);
    h.tick_n(80);

    let state = h.client_app.world().resource::<ClientState>();
    assert!(state.connected, "client should connect within 80 ticks");
    assert!(
        state.auth_grant_count >= 1,
        "client should receive EntityAuthGrantedEvent via give_authority; got {}",
        state.auth_grant_count,
    );
}

#[test]
fn a2_take_authority_propagates_denied_to_client() {
    let mut h = BevyHarness::new_take(SERVER_ADDR_A2);
    h.tick_n(80);

    let state = h.client_app.world().resource::<ClientState>();
    assert!(state.connected, "client should connect within 80 ticks");
    assert!(
        state.auth_denied_count >= 1,
        "client should receive EntityAuthDeniedEvent via take_authority; got {}",
        state.auth_denied_count,
    );
}
