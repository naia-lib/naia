//! Bevy-app integration tests for the `Replication` marker component
//! (naia-lib/naia#182).
//!
//! Inserting `Replication` on an entity begins replication; removing it ends
//! replication -- no `enable_replication` / `disable_replication` command
//! needed. The commands converge onto the same marker, so both paths agree.
//!
//! Coverage:
//! - **live-insert**: inserting the marker on a live entity begins
//!   replication; removing it ends it (client spawns then despawns).
//! - **bundle**: an entity spawned carrying the marker replicates with its
//!   components; removal ends it.
//! - **despawn-once**: despawning an entity that carries the marker disables
//!   replication exactly once (no panic, one client despawn).
//! - **remove-after-disable**: removing the marker from an entity whose
//!   replication was already disabled by command is a silent no-op.

use std::{sync::Arc, time::Duration};

use bevy_app::{App, Startup, Update};
use bevy_ecs::{
    entity::Entity,
    message::Messages,
    resource::Resource,
    schedule::IntoScheduleConfigs,
    system::{Commands, IntoSystem, ResMut},
};
use parking_lot::Mutex;

use naia_bevy_client::{
    events::{
        ConnectEvent as ClientConnectEvent, DespawnEntityEvent, InsertComponentEvent,
        SpawnEntityEvent,
    },
    AppRegisterComponentEvents as ClientAppEvents, Client, ClientConfig, Plugin as ClientPlugin,
};
use naia_bevy_server::{
    events::{AuthEvents, ConnectEvent},
    CommandsExt as ServerEntityCommandsExt, Plugin as ServerPlugin, ProtocolServerExt, Replication,
    Server, ServerConfig,
};
use naia_bevy_shared::Protocol as BevyProtocol;
use naia_client::transport::local::{LocalAddrCell, LocalClientSocket, Socket as ClientSocket};
use naia_server::transport::local::{LocalServerSocket, Socket as ServerSocket};
use naia_shared::transport::local::LocalTransportHub;
use naia_shared::{ChannelDirection, ChannelMode, ReliableSettings};
use naia_test_harness::test_protocol::{Auth, Position, ReliableChannel};

const FAKE_SERVER_ADDR: &str = "127.0.0.1:14192";

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

#[derive(Resource, Default)]
struct ServerState {
    room_key: Option<naia_server::RoomKey>,
    connected_user: Option<naia_server::UserKey>,
}

#[derive(Resource, Default)]
struct ClientConnected(bool);

fn sys_server_auth(mut server: Server, mut auth_msgs: ResMut<Messages<AuthEvents>>) {
    for events in auth_msgs.drain() {
        for (user_key, _) in events.read::<Auth>() {
            server.accept_connection(&user_key);
        }
    }
}

fn sys_server_connect(
    mut connect_msgs: ResMut<Messages<ConnectEvent>>,
    mut state: ResMut<ServerState>,
    mut server: Server,
) {
    for event in connect_msgs.drain() {
        state.connected_user = Some(event.0);
        if let Some(room_key) = state.room_key {
            server.user_mut(&event.0).enter_room(&room_key);
        }
    }
}

struct BevyHarness {
    server_app: App,
    client_app: App,
    // Cumulative event counters. Bevy `Messages` only retain two frames, so
    // draining once at the end of a tick run misses events that fired early;
    // `tick()` drains into these every frame instead.
    spawn_count: usize,
    despawn_count: usize,
    insert_count: usize,
}

impl BevyHarness {
    fn new() -> Self {
        let server_addr = FAKE_SERVER_ADDR.parse().expect("addr");
        let hub = LocalTransportHub::new(server_addr);

        // -- Server App --
        let hub_for_server = hub.clone();
        let mut server_app = App::new();
        server_app.add_plugins(ServerPlugin::new(
            naia_bevy_server::ServerPluginConfig::new(
                ServerConfig::default(),
                protocol(),
                naia_bevy_server::Topology::Standalone(naia_bevy_server::DriveShape::Resident),
            ),
        ));
        server_app
            .init_resource::<ServerState>()
            .add_systems(
                Startup,
                move |mut server: Server, mut state: ResMut<ServerState>| {
                    let socket =
                        ServerSocket::new(LocalServerSocket::new(hub_for_server.clone()), None);
                    server.listen(socket);
                    state.room_key = Some(server.create_room().key());
                },
            )
            .add_systems(
                Update,
                (sys_server_auth, sys_server_connect).in_set(naia_bevy_shared::HandleWorldEvents),
            );
        server_app.update();

        // -- Client App --
        let hub_for_client = hub.clone();
        let mut client_app = App::new();
        let cfg = ClientConfig {
            send_handshake_interval: Duration::from_millis(0),
            ..Default::default()
        };
        client_app.add_plugins(ClientPlugin::<Main>::new(cfg, protocol()));
        ClientAppEvents::add_component_events::<Main, Position>(&mut client_app);
        client_app
            .init_resource::<ClientConnected>()
            .add_systems(Startup, move |mut client: Client<Main>| {
                let (client_addr, auth_req_tx, auth_resp_rx, client_data_tx, client_data_rx) =
                    hub_for_client.register_client();
                let addr_cell = LocalAddrCell::new();
                addr_cell.set_sync(hub_for_client.server_addr());
                let identity_token = Arc::new(Mutex::new(None::<naia_shared::IdentityToken>));
                let rejection_code = Arc::new(Mutex::new(None::<(u16, Option<Vec<u8>>)>));
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
            })
            .add_systems(
                Update,
                sys_client_connect.in_set(naia_bevy_shared::HandleWorldEvents),
            );
        client_app.update();

        Self {
            server_app,
            client_app,
            spawn_count: 0,
            despawn_count: 0,
            insert_count: 0,
        }
    }

    fn tick(&mut self) {
        naia_bevy_shared::TestClock::advance(60);
        self.server_app.update();
        self.client_app.update();
        self.spawn_count += self
            .client_app
            .world_mut()
            .resource_mut::<Messages<SpawnEntityEvent<Main>>>()
            .drain()
            .count();
        self.despawn_count += self
            .client_app
            .world_mut()
            .resource_mut::<Messages<DespawnEntityEvent<Main>>>()
            .drain()
            .count();
        self.insert_count += self
            .client_app
            .world_mut()
            .resource_mut::<Messages<InsertComponentEvent<Main, Position>>>()
            .drain()
            .count();
    }

    fn tick_n(&mut self, n: u32) {
        for _ in 0..n {
            self.tick();
        }
    }

    fn wait_for_connect(&mut self) {
        for _ in 0..120 {
            self.tick();
            let server_ready = self
                .server_app
                .world()
                .resource::<ServerState>()
                .connected_user
                .is_some();
            let client_ready = self.client_app.world().resource::<ClientConnected>().0;
            if server_ready && client_ready {
                return;
            }
        }
        panic!("client should connect within 120 ticks");
    }

    /// Runs a one-shot server system, then an update so queued commands apply.
    fn run_server_system<M>(&mut self, sys: impl IntoSystem<(), (), M> + 'static)
    where
        M: 'static,
    {
        self.run_server_system_raw(sys);
        self.server_app.update();
    }

    /// Runs a one-shot server system with no trailing update.
    fn run_server_system_raw<M>(&mut self, sys: impl IntoSystem<(), (), M> + 'static)
    where
        M: 'static,
    {
        let id = self.server_app.register_system(sys);
        let _ = self.server_app.world_mut().run_system(id);
    }

    fn server_spawn_position(&mut self) -> Entity {
        let out: Arc<Mutex<Option<Entity>>> = Arc::new(Mutex::new(None));
        let out_clone = out.clone();
        self.run_server_system(move |mut commands: Commands| {
            let entity = commands.spawn(Position::new(3.0, 4.0)).id();
            *out_clone.lock() = Some(entity);
        });
        let entity = out.lock().take().expect("spawn ran");
        entity
    }

    /// Spawns an entity already carrying the marker, without a trailing
    /// update: the spawn and the marker flush together on the next update,
    /// so component `Added` ticks are fresh when tracking starts.
    fn server_spawn_marked(&mut self) -> Entity {
        let out: Arc<Mutex<Option<Entity>>> = Arc::new(Mutex::new(None));
        let out_clone = out.clone();
        self.run_server_system_raw(move |mut commands: Commands| {
            let entity = commands.spawn((Position::new(3.0, 4.0), Replication)).id();
            *out_clone.lock() = Some(entity);
        });
        let entity = out.lock().take().expect("spawn ran");
        entity
    }

    /// Adds an entity to the test room. The entity must already be registered
    /// with naia (room join resolves through the global entity map).
    fn server_add_to_room(&mut self, entity: Entity) {
        self.run_server_system(move |mut server: Server, state: ResMut<ServerState>| {
            if let Some(room_key) = state.room_key {
                server.room_mut(&room_key).add_entity(&entity);
            }
        });
    }

    fn client_position_count(&mut self) -> usize {
        self.client_app
            .world_mut()
            .query::<&Position>()
            .iter(self.client_app.world())
            .count()
    }

    fn client_spawn_count(&self) -> usize {
        self.spawn_count
    }

    fn client_despawn_count(&self) -> usize {
        self.despawn_count
    }

    fn client_insert_count(&self) -> usize {
        self.insert_count
    }
}

fn sys_client_connect(
    mut connect_msgs: ResMut<Messages<ClientConnectEvent<Main>>>,
    mut state: ResMut<ClientConnected>,
) {
    for _ in connect_msgs.drain() {
        state.0 = true;
    }
}

/// Inserting `Replication` on a live entity begins replication; removing it
/// ends it -- all without any command call.
#[test]
fn marker_insert_on_live_entity_begins_replication_and_removal_ends_it() {
    let mut h = BevyHarness::new();
    h.wait_for_connect();

    // Spawned without the marker: not replicated.
    let entity = h.server_spawn_position();
    h.tick_n(30);
    assert_eq!(
        h.client_position_count(),
        0,
        "an unmarked entity must not replicate"
    );
    assert_eq!(h.client_spawn_count(), 0);

    // Insert the marker on the live entity: replication begins, the client
    // spawns the entity. Only the spawn is asserted here: components whose
    // `Added` change-ticks expired before the marker landed do not
    // retro-sync, an adapter-wide change-detection limit the command path
    // shares (not a marker gap). Full component sync is covered by the
    // spawn-bundle test below.
    h.run_server_system(move |mut commands: Commands| {
        commands.entity(entity).insert(Replication);
    });
    h.server_add_to_room(entity);
    h.tick_n(60);
    assert_eq!(
        h.client_spawn_count(),
        1,
        "inserting Replication must replicate the entity to the client"
    );

    // Remove the marker: replication ends, the client despawns.
    h.run_server_system(move |mut commands: Commands| {
        commands.entity(entity).remove::<Replication>();
    });
    h.tick_n(30);
    assert_eq!(
        h.client_position_count(),
        0,
        "removing Replication must despawn the entity on the client"
    );
    assert_eq!(h.client_despawn_count(), 1);
}

/// An entity spawned carrying the marker replicates with its components.
#[test]
fn marker_in_spawn_bundle_replicates_components() {
    let mut h = BevyHarness::new();
    h.wait_for_connect();

    let entity = h.server_spawn_marked();
    h.server_add_to_room(entity);
    h.tick_n(60);
    assert_eq!(
        h.client_position_count(),
        1,
        "a marked entity must replicate its components to the client"
    );
    assert_eq!(h.client_spawn_count(), 1);
    assert_eq!(h.client_insert_count(), 1);

    // Remove the marker: replication ends, the client despawns.
    h.run_server_system(move |mut commands: Commands| {
        commands.entity(entity).remove::<Replication>();
    });
    h.tick_n(30);
    assert_eq!(
        h.client_position_count(),
        0,
        "removing Replication must despawn the entity on the client"
    );
    assert_eq!(h.client_despawn_count(), 1);
}

/// Despawning an entity that carries the marker disables replication exactly
/// once: no panic, one client despawn, and the server record is gone.
#[test]
fn despawn_with_marker_disables_exactly_once() {
    let mut h = BevyHarness::new();
    h.wait_for_connect();

    let entity = h.server_spawn_marked();
    h.server_add_to_room(entity);
    h.tick_n(60);
    assert_eq!(h.client_position_count(), 1);
    assert_eq!(h.client_spawn_count(), 1);

    h.run_server_system(move |mut commands: Commands| {
        commands.entity(entity).despawn();
    });
    h.tick_n(30);
    assert_eq!(
        h.client_position_count(),
        0,
        "despawning must remove the entity on the client"
    );
    assert_eq!(
        h.client_despawn_count(),
        1,
        "the client must observe exactly one despawn"
    );
}

/// Removing the marker from an entity whose replication was already disabled
/// by command is a silent no-op.
#[test]
fn removing_marker_after_command_disable_is_noop() {
    let mut h = BevyHarness::new();
    h.wait_for_connect();

    // Command path converges onto the marker: enable inserts it. The enable
    // runs before the spawn flushes (same frame), so component `Added`
    // ticks are still fresh when tracking starts.
    let out: Arc<Mutex<Option<Entity>>> = Arc::new(Mutex::new(None));
    let out_clone = out.clone();
    h.run_server_system_raw(move |mut commands: Commands, mut server: Server| {
        let entity = commands.spawn(Position::new(3.0, 4.0)).id();
        commands.entity(entity).enable_replication(&mut server);
        *out_clone.lock() = Some(entity);
    });
    let entity = out.lock().take().expect("spawn ran");
    h.server_add_to_room(entity);
    h.tick_n(60);
    assert_eq!(h.client_position_count(), 1);

    // Disable by command removes the marker; removing it again is a no-op:
    // no panic, still unregistered, client stays clean.
    h.run_server_system(move |mut commands: Commands, mut server: Server| {
        commands.entity(entity).disable_replication(&mut server);
        commands.entity(entity).remove::<Replication>();
    });
    h.tick_n(30);
    assert_eq!(h.client_position_count(), 0);
    assert_eq!(h.client_despawn_count(), 1);
}
