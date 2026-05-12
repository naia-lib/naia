use std::marker::PhantomData;
use std::sync::Arc;
use std::time::Duration;

use bevy_app::{App, Startup, Update};
use bevy_ecs::{
    entity::Entity,
    message::Messages,
    resource::Resource,
    schedule::IntoScheduleConfigs,
    system::{Commands, ResMut, RunSystemOnce},
};
use parking_lot::Mutex as ParkingMutex;

use naia_bevy_server::{
    ReplicationConfig, RoomKey,
    ServerCommandsExt,
    events::{AuthEvents, ConnectEvent, DisconnectEvent},
    Plugin as ServerPlugin, Server, ServerConfig, UserKey,
};
use naia_bevy_client::{
    ClientCommandsExt,
    EntityAuthStatus,
    events::{
        ConnectEvent as ClientConnectEvent,
        DisconnectEvent as ClientDisconnectEvent,
        DespawnEntityEvent,
        EntityAuthDeniedEvent,
        EntityAuthGrantedEvent,
        SpawnEntityEvent,
    },
    Plugin as ClientPlugin, Client, ClientConfig,
};
use naia_bevy_shared::Protocol as BevyProtocol;
use naia_server::transport::local::{LocalServerSocket, Socket as ServerSocket};
use naia_client::transport::local::{LocalAddrCell, LocalClientSocket, Socket as ClientSocket};
use naia_shared::{
    transport::local::{LocalTransportHub, FAKE_SERVER_ADDR},
    ChannelDirection, ChannelMode, ReliableSettings,
};
use naia_test_harness::{
    test_protocol::{Auth, Position, ReliableChannel, TestPlayerSelection, TestScore},
};

use namako_engine::codegen::AssertOutcome;

// ── Protocol ──────────────────────────────────────────────────────────────────

fn bevy_protocol() -> BevyProtocol {
    BevyProtocol::builder()
        .add_message::<Auth>()
        .add_channel::<ReliableChannel>(
            ChannelDirection::Bidirectional,
            ChannelMode::UnorderedReliable(ReliableSettings::default()),
        )
        .add_component::<Position>()
        .add_resource::<TestScore>()
        .add_resource::<TestPlayerSelection>()
        .enable_client_authoritative_entities()
        .tick_interval(Duration::from_micros(100))
        .build()
}

// ── Marker types ──────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ClientKey(pub u32);

pub struct ClientSingleton;

// ── Capture resources ─────────────────────────────────────────────────────────

#[derive(Resource, Default)]
pub struct ServerState {
    pub connected_user_keys: Vec<UserKey>,
    pub connect_count: u32,
    pub disconnect_count: u32,
    pub room_key: Option<RoomKey>,
    pub last_entity: Option<Entity>,
    pub player_selection_authority: Option<EntityAuthStatus>,
}

#[derive(Resource, Default)]
pub struct ClientState {
    pub connect_count: u32,
    pub disconnect_count: u32,
    pub is_connected: bool,
    // Entity tracking
    pub spawn_event_count: u32,
    pub despawn_event_count: u32,
    pub last_spawned_entity: Option<Entity>,
    pub authority_status: Option<EntityAuthStatus>,
    pub auth_granted_event_count: u32,
    pub auth_denied_event_count: u32,
    // Resource tracking — mirrors Bevy Resource<TestScore> each tick
    pub test_score: Option<(u32, u32)>,
}

// ── Server systems ────────────────────────────────────────────────────────────

fn sys_server_auth(mut server: Server, mut auth_msgs: ResMut<Messages<AuthEvents>>) {
    for events in auth_msgs.drain() {
        for (user_key, _auth) in events.read::<Auth>() {
            server.accept_connection(&user_key);
        }
    }
}

fn sys_server_connect(
    mut server: Server,
    mut connect_msgs: ResMut<Messages<ConnectEvent>>,
    mut state: ResMut<ServerState>,
) {
    for event in connect_msgs.drain() {
        state.connected_user_keys.push(event.0);
        state.connect_count += 1;
        // Add user to the default room so resource entities enter scope.
        if let Some(room_key) = state.room_key {
            server.user_mut(&event.0).enter_room(&room_key);
        }
    }
}

fn sys_server_disconnect(
    mut disconnect_msgs: ResMut<Messages<DisconnectEvent>>,
    mut state: ResMut<ServerState>,
) {
    for event in disconnect_msgs.drain() {
        state.connected_user_keys.retain(|k| *k != event.0);
        state.disconnect_count += 1;
    }
}

fn sys_server_resource_authority(
    server: Server,
    mut state: ResMut<ServerState>,
) {
    state.player_selection_authority =
        server.resource_authority_status::<TestPlayerSelection>();
}

// ── Client systems ────────────────────────────────────────────────────────────

fn sys_client_connect(
    mut connect_msgs: ResMut<Messages<ClientConnectEvent<ClientSingleton>>>,
    mut state: ResMut<ClientState>,
) {
    for _ in connect_msgs.drain() {
        state.connect_count += 1;
        state.is_connected = true;
    }
}

fn sys_client_disconnect(
    mut disconnect_msgs: ResMut<Messages<ClientDisconnectEvent<ClientSingleton>>>,
    mut state: ResMut<ClientState>,
) {
    for _ in disconnect_msgs.drain() {
        state.disconnect_count += 1;
        state.is_connected = false;
    }
}

fn sys_client_spawn_entity(
    mut spawn_msgs: ResMut<Messages<SpawnEntityEvent<ClientSingleton>>>,
    mut state: ResMut<ClientState>,
) {
    for event in spawn_msgs.drain() {
        state.spawn_event_count += 1;
        state.last_spawned_entity = Some(event.entity);
    }
}

fn sys_client_despawn_entity(
    mut despawn_msgs: ResMut<Messages<DespawnEntityEvent<ClientSingleton>>>,
    mut state: ResMut<ClientState>,
) {
    for event in despawn_msgs.drain() {
        state.despawn_event_count += 1;
        if state.last_spawned_entity == Some(event.entity) {
            state.last_spawned_entity = None;
        }
    }
}

fn sys_client_auth_events(
    mut granted_msgs: ResMut<Messages<EntityAuthGrantedEvent<ClientSingleton>>>,
    mut denied_msgs: ResMut<Messages<EntityAuthDeniedEvent<ClientSingleton>>>,
    mut state: ResMut<ClientState>,
) {
    for _ in granted_msgs.drain() {
        state.auth_granted_event_count += 1;
    }
    for _ in denied_msgs.drain() {
        state.auth_denied_event_count += 1;
    }
}

fn sys_client_authority_status(
    mut commands: Commands,
    client: Client<ClientSingleton>,
    mut state: ResMut<ClientState>,
) {
    let entity = state.last_spawned_entity;
    if let Some(entity) = entity {
        let entity_cmds = commands.entity(entity);
        // Disambiguate: use client-side CommandsExt::authority (not server-side)
        state.authority_status =
            naia_bevy_client::CommandsExt::authority::<ClientSingleton>(&entity_cmds, &client);
    }
}

fn sys_client_score(
    score: Option<bevy_ecs::system::Res<TestScore>>,
    mut state: ResMut<ClientState>,
) {
    state.test_score = score.map(|s| (*s.home, *s.away));
}

// ── BevyTestHarness ───────────────────────────────────────────────────────────

pub struct BevyTestHarness {
    server_app: App,
    client_apps: Vec<(ClientKey, App)>,
    hub: LocalTransportHub,
    next_client_id: u32,
}

impl BevyTestHarness {
    pub fn new() -> Self {
        let server_addr = FAKE_SERVER_ADDR.parse().expect("invalid server addr");
        let hub = LocalTransportHub::new(server_addr);
        let hub_for_startup = hub.clone();

        let mut server_app = App::new();
        server_app
            .add_plugins(ServerPlugin::new(ServerConfig::default(), bevy_protocol()));
        naia_bevy_server::AppRegisterComponentEvents::add_resource_events::<TestScore>(&mut server_app);
        naia_bevy_server::AppRegisterComponentEvents::add_resource_events::<TestPlayerSelection>(&mut server_app);
        server_app
            .init_resource::<ServerState>()
            .add_systems(
                Startup,
                move |mut server: Server, mut state: ResMut<ServerState>| {
                    let socket = ServerSocket::new(
                        LocalServerSocket::new(hub_for_startup.clone()),
                        None,
                    );
                    server.listen(socket);
                    let room = server.create_room();
                    state.room_key = Some(room.key());
                },
            )
            .add_systems(
                Update,
                (
                    sys_server_auth,
                    sys_server_connect,
                    sys_server_disconnect,
                    sys_server_resource_authority,
                )
                    .in_set(naia_bevy_shared::HandleWorldEvents),
            );

        server_app.update(); // Run Startup

        Self {
            server_app,
            client_apps: Vec::new(),
            hub,
            next_client_id: 0,
        }
    }

    pub fn tick(&mut self) {
        naia_shared::TestClock::advance(60);
        self.server_app.update();
        for (_, app) in &mut self.client_apps {
            app.update();
        }
    }

    pub fn tick_until<F: FnMut(&Self) -> bool>(&mut self, mut pred: F, max: u32) -> bool {
        for _ in 0..max {
            self.tick();
            if pred(self) {
                return true;
            }
        }
        false
    }

    pub fn add_client(&mut self) -> ClientKey {
        let hub = self.hub.clone();
        let key = ClientKey(self.next_client_id);
        self.next_client_id += 1;

        let cfg = ClientConfig {
            send_handshake_interval: Duration::from_millis(0),
            ..Default::default()
        };

        let mut app = App::new();
        app.add_plugins(ClientPlugin::<ClientSingleton>::new(cfg, bevy_protocol()));
        naia_bevy_client::AppRegisterComponentEvents::add_resource_events::<ClientSingleton, TestScore>(&mut app);
        naia_bevy_client::AppRegisterComponentEvents::add_resource_events::<ClientSingleton, TestPlayerSelection>(&mut app);
        app.init_resource::<ClientState>()
            .add_systems(
                Startup,
                move |mut client: Client<ClientSingleton>| {
                    let (client_addr, auth_req_tx, auth_resp_rx, client_data_tx, client_data_rx) =
                        hub.register_client();
                    let addr_cell = LocalAddrCell::new();
                    addr_cell.set_sync(hub.server_addr());
                    let identity_token =
                        Arc::new(ParkingMutex::new(None::<naia_shared::IdentityToken>));
                    let rejection_code = Arc::new(ParkingMutex::new(None::<u16>));
                    let inner_socket = LocalClientSocket::new_with_tokens(
                        client_addr,
                        hub.server_addr(),
                        auth_req_tx,
                        auth_resp_rx,
                        client_data_tx,
                        client_data_rx,
                        addr_cell,
                        identity_token,
                        rejection_code,
                    );
                    let socket = ClientSocket::new(inner_socket, None);
                    client.auth(Auth::new("test_user", "password"));
                    client.connect(socket);
                },
            )
            .add_systems(
                Update,
                (
                    sys_client_connect,
                    sys_client_disconnect,
                    sys_client_spawn_entity,
                    sys_client_despawn_entity,
                    sys_client_auth_events,
                )
                    .in_set(naia_bevy_shared::HandleWorldEvents),
            )
            .add_systems(
                Update,
                (sys_client_authority_status, sys_client_score)
                    .after(naia_bevy_shared::HandleWorldEvents),
            );

        app.update(); // Run Startup

        self.client_apps.push((key, app));
        key
    }

    // ── State accessors — connection ──────────────────────────────────────────

    pub fn server_connected_count(&self) -> usize {
        self.server_app.world().resource::<ServerState>().connected_user_keys.len()
    }

    pub fn server_connect_count(&self) -> u32 {
        self.server_app.world().resource::<ServerState>().connect_count
    }

    pub fn server_disconnect_count(&self) -> u32 {
        self.server_app.world().resource::<ServerState>().disconnect_count
    }

    pub fn client_is_connected(&self, key: ClientKey) -> bool {
        self.client_app(key)
            .map(|app| app.world().resource::<ClientState>().is_connected)
            .unwrap_or(false)
    }

    pub fn client_connect_count(&self, key: ClientKey) -> u32 {
        self.client_app(key)
            .map(|app| app.world().resource::<ClientState>().connect_count)
            .unwrap_or(0)
    }

    pub fn client_disconnect_count(&self, key: ClientKey) -> u32 {
        self.client_app(key)
            .map(|app| app.world().resource::<ClientState>().disconnect_count)
            .unwrap_or(0)
    }

    pub fn last_client_key(&self) -> Option<ClientKey> {
        self.client_apps.last().map(|(k, _)| *k)
    }

    pub fn disconnect_last_user(&mut self) {
        let user_key = {
            self.server_app
                .world()
                .resource::<ServerState>()
                .connected_user_keys
                .last()
                .copied()
                .expect("no connected users to disconnect")
        };
        let _ = self.server_app.world_mut().run_system_once(
            move |mut server: Server| { server.user_mut(&user_key).disconnect(); },
        );
    }

    // ── State accessors — entity ──────────────────────────────────────────────

    pub fn client_spawn_event_count(&self, key: ClientKey) -> u32 {
        self.client_app(key)
            .map(|app| app.world().resource::<ClientState>().spawn_event_count)
            .unwrap_or(0)
    }

    pub fn client_despawn_event_count(&self, key: ClientKey) -> u32 {
        self.client_app(key)
            .map(|app| app.world().resource::<ClientState>().despawn_event_count)
            .unwrap_or(0)
    }

    pub fn client_has_entity(&self, key: ClientKey) -> bool {
        self.client_app(key)
            .map(|app| app.world().resource::<ClientState>().last_spawned_entity.is_some())
            .unwrap_or(false)
    }

    pub fn client_entity_position(&self, key: ClientKey) -> Option<(f32, f32)> {
        let app = self.client_app(key)?;
        let entity = app.world().resource::<ClientState>().last_spawned_entity?;
        let pos = app.world().get::<Position>(entity)?;
        Some((*pos.x, *pos.y))
    }

    pub fn client_authority_status(&self, key: ClientKey) -> Option<EntityAuthStatus> {
        self.client_app(key)
            .and_then(|app| app.world().resource::<ClientState>().authority_status)
    }

    pub fn client_auth_granted_event_count(&self, key: ClientKey) -> u32 {
        self.client_app(key)
            .map(|app| app.world().resource::<ClientState>().auth_granted_event_count)
            .unwrap_or(0)
    }

    pub fn client_auth_denied_event_count(&self, key: ClientKey) -> u32 {
        self.client_app(key)
            .map(|app| app.world().resource::<ClientState>().auth_denied_event_count)
            .unwrap_or(0)
    }

    // ── State accessors — resource ────────────────────────────────────────────

    pub fn client_score(&self, key: ClientKey) -> Option<(u32, u32)> {
        self.client_app(key)
            .and_then(|app| app.world().resource::<ClientState>().test_score)
    }

    pub fn server_player_selection_authority(&self) -> Option<EntityAuthStatus> {
        self.server_app.world().resource::<ServerState>().player_selection_authority
    }

    // ── Imperative server operations ──────────────────────────────────────────

    pub fn server_spawn_entity(&mut self) -> Entity {
        let entity = self.server_app.world_mut().spawn(Position::new(0.0, 0.0)).id();
        let _ = self.server_app.world_mut().run_system_once(
            move |mut commands: Commands, mut server: Server, mut state: ResMut<ServerState>| {
                // Use server-side CommandsExt::enable_replication explicitly
                naia_bevy_server::CommandsExt::enable_replication(
                    &mut commands.entity(entity),
                    &mut server,
                );
                state.last_entity = Some(entity);
            },
        );
        naia_shared::TestClock::advance(60);
        self.server_app.update(); // flush HostOwned insert + fire tick
        entity
    }

    pub fn server_spawn_entity_with_position(&mut self, x: f32, y: f32) -> Entity {
        let entity = self.server_app.world_mut().spawn(Position::new(x, y)).id();
        let _ = self.server_app.world_mut().run_system_once(
            move |mut commands: Commands, mut server: Server, mut state: ResMut<ServerState>| {
                naia_bevy_server::CommandsExt::enable_replication(
                    &mut commands.entity(entity),
                    &mut server,
                );
                state.last_entity = Some(entity);
            },
        );
        naia_shared::TestClock::advance(60);
        self.server_app.update();
        entity
    }

    pub fn server_scope_entity_for_all_clients(&mut self, entity: Entity) {
        let room_key = self.server_app
            .world()
            .resource::<ServerState>()
            .room_key
            .expect("no room created");
        let user_keys: Vec<UserKey> = self.server_app
            .world()
            .resource::<ServerState>()
            .connected_user_keys
            .clone();
        let _ = self.server_app.world_mut().run_system_once(
            move |mut server: Server| {
                server.room_mut(&room_key).add_entity(&entity);
                for user_key in &user_keys {
                    server.room_mut(&room_key).add_user(user_key);
                }
            },
        );
    }

    pub fn server_configure_delegated(&mut self) {
        let entity = self.server_app
            .world()
            .resource::<ServerState>()
            .last_entity
            .expect("no entity spawned");
        let _ = self.server_app.world_mut().run_system_once(
            move |mut commands: Commands| {
                // Use server-side CommandsExt::configure_replication explicitly
                naia_bevy_server::CommandsExt::configure_replication(
                    &mut commands.entity(entity),
                    ReplicationConfig::delegated(),
                );
            },
        );
        naia_shared::TestClock::advance(60);
        self.server_app.update(); // flush WorldOpCommand + fire tick to send delegation to client
        // Let clients receive the delegation notification.
        for (_, app) in &mut self.client_apps {
            app.update();
        }
    }

    pub fn server_give_authority_to_client(&mut self, client_key: ClientKey) {
        let entity = self.server_app
            .world()
            .resource::<ServerState>()
            .last_entity
            .expect("no entity spawned");
        let user_key = self.server_app
            .world()
            .resource::<ServerState>()
            .connected_user_keys
            .get(client_key.0 as usize)
            .copied()
            .unwrap_or_else(|| panic!("no user for ClientKey({})", client_key.0));
        let _ = self.server_app.world_mut().run_system_once(
            move |mut commands: Commands, mut server: Server| {
                naia_bevy_server::CommandsExt::give_authority(
                    &mut commands.entity(entity),
                    &mut server,
                    &user_key,
                );
            },
        );
        naia_shared::TestClock::advance(60);
        self.server_app.update();
    }

    pub fn server_disable_replication(&mut self) {
        let entity = self.server_app
            .world()
            .resource::<ServerState>()
            .last_entity
            .expect("no entity spawned");
        let _ = self.server_app.world_mut().run_system_once(
            move |mut commands: Commands, mut server: Server| {
                naia_bevy_server::CommandsExt::disable_replication(
                    &mut commands.entity(entity),
                    &mut server,
                );
            },
        );
        naia_shared::TestClock::advance(60);
        self.server_app.update();
    }

    pub fn server_update_position(&mut self, x: f32, y: f32) {
        let entity = self.server_app
            .world()
            .resource::<ServerState>()
            .last_entity
            .expect("no entity spawned");
        let mut pos = self.server_app
            .world_mut()
            .get_mut::<Position>(entity)
            .expect("entity has no Position");
        *pos.x = x;
        *pos.y = y;
    }

    pub fn server_insert_score(&mut self, home: u32, away: u32) {
        let _ = self.server_app.world_mut().run_system_once(
            move |mut commands: Commands| {
                commands.replicate_resource(TestScore::new(home, away));
            },
        );
        naia_shared::TestClock::advance(60);
        self.server_app.update();
    }

    pub fn server_mutate_score(&mut self, home: u32, away: u32) {
        let mut score = self.server_app.world_mut().resource_mut::<TestScore>();
        *score.home = home;
        *score.away = away;
    }

    pub fn server_insert_player_selection_delegated(&mut self, selected_id: u16) {
        let _ = self.server_app.world_mut().run_system_once(
            move |mut commands: Commands| {
                commands.replicate_resource(TestPlayerSelection::new(selected_id));
                commands.configure_replicated_resource::<TestPlayerSelection>(
                    ReplicationConfig::delegated(),
                );
            },
        );
        naia_shared::TestClock::advance(60);
        self.server_app.update();
    }

    // ── Imperative client operations ──────────────────────────────────────────

    pub fn client_request_entity_authority(&mut self, key: ClientKey) {
        let pos = self.client_apps.iter().position(|(k, _)| *k == key)
            .expect("client not found");
        let entity = self.client_apps[pos].1
            .world()
            .resource::<ClientState>()
            .last_spawned_entity
            .expect("client has no spawned entity");
        let _ = self.client_apps[pos].1.world_mut().run_system_once(
            move |mut commands: Commands, mut client: Client<ClientSingleton>| {
                // Use client-side CommandsExt::request_authority explicitly
                naia_bevy_client::CommandsExt::request_authority::<ClientSingleton>(
                    &mut commands.entity(entity),
                    &mut client,
                );
            },
        );
        naia_shared::TestClock::advance(60);
        self.client_apps[pos].1.update(); // fire tick + send packet
        // Flush the request through the server so sequential calls from different
        // clients are processed in order (prevents non-deterministic grant/deny).
        naia_shared::TestClock::advance(60);
        self.server_app.update();
        for (_, app) in &mut self.client_apps {
            app.update();
        }
    }

    pub fn client_request_player_selection_authority(&mut self, key: ClientKey) {
        let pos = self.client_apps.iter().position(|(k, _)| *k == key)
            .expect("client not found");
        let _ = self.client_apps[pos].1.world_mut().run_system_once(
            |mut commands: Commands| {
                commands.request_resource_authority::<ClientSingleton, TestPlayerSelection>();
            },
        );
        naia_shared::TestClock::advance(60);
        self.client_apps[pos].1.update(); // fire tick + send packet
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn client_app(&self, key: ClientKey) -> Option<&App> {
        self.client_apps.iter().find(|(k, _)| *k == key).map(|(_, app)| app)
    }
}

// ── BevyTestWorld ─────────────────────────────────────────────────────────────

#[derive(Default)]
pub struct BevyTestWorld(Option<BevyTestHarness>);

impl BevyTestWorld {
    pub fn harness_mut(&mut self) -> &mut BevyTestHarness {
        self.0.as_mut().expect("harness not initialized")
    }

    pub fn init(&mut self) -> &mut BevyTestHarness {
        self.0.insert(BevyTestHarness::new())
    }
}

impl namako_engine::World for BevyTestWorld {
    type Error = std::convert::Infallible;
    type MutCtx<'a> = BevyMutCtx<'a>;
    type RefCtx<'a> = BevyRefCtx<'a>;

    async fn new() -> Result<Self, Self::Error> {
        Ok(Self::default())
    }

    fn ctx_mut(&mut self) -> BevyMutCtx<'_> {
        BevyMutCtx(self)
    }

    fn ctx_ref(&mut self) -> BevyRefCtx<'_> {
        panic!("ctx_ref() should not be called directly — use assert_then()")
    }

    fn assert_then<T, F>(&mut self, mut f: F) -> T
    where
        F: FnMut(&BevyRefCtx<'_>) -> AssertOutcome<T>,
    {
        for _ in 0..500 {
            {
                let h = self.harness_mut();
                naia_shared::TestClock::advance(60);
                h.server_app.update();
                for (_, app) in &mut h.client_apps {
                    app.update();
                }
            }
            let ctx = BevyRefCtx(self.0.as_ref().unwrap() as *const _, PhantomData);
            match f(&ctx) {
                AssertOutcome::Passed(v) => return v,
                AssertOutcome::Pending => {}
                AssertOutcome::Failed(msg) => panic!("Assertion failed: {}", msg),
            }
        }
        panic!("assert_then: timed out after 500 ticks")
    }
}

impl std::fmt::Debug for BevyTestWorld {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "BevyTestWorld")
    }
}

// ── Context types ─────────────────────────────────────────────────────────────

pub struct BevyMutCtx<'a>(&'a mut BevyTestWorld);

impl<'a> BevyMutCtx<'a> {
    pub fn init(&mut self) -> &mut BevyTestHarness {
        self.0.init()
    }

    pub fn harness_mut(&mut self) -> &mut BevyTestHarness {
        self.0.harness_mut()
    }
}

impl namako_engine::codegen::StepContext for BevyMutCtx<'_> {
    type World = BevyTestWorld;
}

pub struct BevyRefCtx<'a>(*const BevyTestHarness, PhantomData<&'a ()>);

impl<'a> BevyRefCtx<'a> {
    fn h(&self) -> &BevyTestHarness {
        // Safety: single-threaded, pointer valid for the assert_then loop lifetime
        unsafe { &*self.0 }
    }

    // ── Connection accessors ──────────────────────────────────────────────────

    pub fn server_connected_count(&self) -> usize { self.h().server_connected_count() }
    pub fn server_connect_count(&self) -> u32 { self.h().server_connect_count() }
    pub fn server_disconnect_count(&self) -> u32 { self.h().server_disconnect_count() }
    pub fn client_is_connected(&self, key: ClientKey) -> bool { self.h().client_is_connected(key) }
    pub fn client_connect_count(&self, key: ClientKey) -> u32 { self.h().client_connect_count(key) }
    pub fn client_disconnect_count(&self, key: ClientKey) -> u32 { self.h().client_disconnect_count(key) }
    pub fn last_client_key(&self) -> Option<ClientKey> { self.h().last_client_key() }

    // ── Entity accessors ──────────────────────────────────────────────────────

    pub fn client_spawn_event_count(&self, key: ClientKey) -> u32 {
        self.h().client_spawn_event_count(key)
    }
    pub fn client_despawn_event_count(&self, key: ClientKey) -> u32 {
        self.h().client_despawn_event_count(key)
    }
    pub fn client_has_entity(&self, key: ClientKey) -> bool {
        self.h().client_has_entity(key)
    }
    pub fn client_entity_position(&self, key: ClientKey) -> Option<(f32, f32)> {
        self.h().client_entity_position(key)
    }
    pub fn client_authority_status(&self, key: ClientKey) -> Option<EntityAuthStatus> {
        self.h().client_authority_status(key)
    }
    pub fn client_auth_granted_event_count(&self, key: ClientKey) -> u32 {
        self.h().client_auth_granted_event_count(key)
    }
    pub fn client_auth_denied_event_count(&self, key: ClientKey) -> u32 {
        self.h().client_auth_denied_event_count(key)
    }

    // ── Resource accessors ────────────────────────────────────────────────────

    pub fn client_score(&self, key: ClientKey) -> Option<(u32, u32)> {
        self.h().client_score(key)
    }
    pub fn server_player_selection_authority(&self) -> Option<EntityAuthStatus> {
        self.h().server_player_selection_authority()
    }
}

impl<'a> namako_engine::codegen::StepContext for BevyRefCtx<'a> {
    type World = BevyTestWorld;
}

// ── WorldInventory boilerplate ────────────────────────────────────────────────

#[doc(hidden)]
pub struct NamakoGivenBevyTestWorld {
    pub loc: namako_engine::step::Location,
    pub binding_id: &'static str,
    pub expression: &'static str,
    pub kind: &'static str,
    pub impl_hash: &'static str,
    pub captures_arity: u32,
    pub accepts_docstring: bool,
    pub accepts_datatable: bool,
    pub source_symbol: &'static str,
    pub regex: namako_engine::codegen::LazyRegex,
    pub func: namako_engine::Step<BevyTestWorld>,
}

impl namako_engine::codegen::StepConstructor<BevyTestWorld> for NamakoGivenBevyTestWorld {
    fn inner(
        &self,
    ) -> (
        namako_engine::step::Location,
        namako_engine::codegen::LazyRegex,
        namako_engine::Step<BevyTestWorld>,
    ) {
        (self.loc, self.regex, self.func)
    }
    fn npap_metadata(&self) -> namako_engine::codegen::NpapBindingMetadata {
        namako_engine::codegen::NpapBindingMetadata {
            binding_id: self.binding_id,
            expression: self.expression,
            kind: self.kind,
            impl_hash: self.impl_hash,
            captures_arity: self.captures_arity,
            accepts_docstring: self.accepts_docstring,
            accepts_datatable: self.accepts_datatable,
            source_symbol: self.source_symbol,
        }
    }
}
namako_engine::codegen::collect!(NamakoGivenBevyTestWorld);

#[doc(hidden)]
pub struct NamakoWhenBevyTestWorld {
    pub loc: namako_engine::step::Location,
    pub binding_id: &'static str,
    pub expression: &'static str,
    pub kind: &'static str,
    pub impl_hash: &'static str,
    pub captures_arity: u32,
    pub accepts_docstring: bool,
    pub accepts_datatable: bool,
    pub source_symbol: &'static str,
    pub regex: namako_engine::codegen::LazyRegex,
    pub func: namako_engine::Step<BevyTestWorld>,
}

impl namako_engine::codegen::StepConstructor<BevyTestWorld> for NamakoWhenBevyTestWorld {
    fn inner(
        &self,
    ) -> (
        namako_engine::step::Location,
        namako_engine::codegen::LazyRegex,
        namako_engine::Step<BevyTestWorld>,
    ) {
        (self.loc, self.regex, self.func)
    }
    fn npap_metadata(&self) -> namako_engine::codegen::NpapBindingMetadata {
        namako_engine::codegen::NpapBindingMetadata {
            binding_id: self.binding_id,
            expression: self.expression,
            kind: self.kind,
            impl_hash: self.impl_hash,
            captures_arity: self.captures_arity,
            accepts_docstring: self.accepts_docstring,
            accepts_datatable: self.accepts_datatable,
            source_symbol: self.source_symbol,
        }
    }
}
namako_engine::codegen::collect!(NamakoWhenBevyTestWorld);

#[doc(hidden)]
pub struct NamakoThenBevyTestWorld {
    pub loc: namako_engine::step::Location,
    pub binding_id: &'static str,
    pub expression: &'static str,
    pub kind: &'static str,
    pub impl_hash: &'static str,
    pub captures_arity: u32,
    pub accepts_docstring: bool,
    pub accepts_datatable: bool,
    pub source_symbol: &'static str,
    pub regex: namako_engine::codegen::LazyRegex,
    pub func: namako_engine::Step<BevyTestWorld>,
}

impl namako_engine::codegen::StepConstructor<BevyTestWorld> for NamakoThenBevyTestWorld {
    fn inner(
        &self,
    ) -> (
        namako_engine::step::Location,
        namako_engine::codegen::LazyRegex,
        namako_engine::Step<BevyTestWorld>,
    ) {
        (self.loc, self.regex, self.func)
    }
    fn npap_metadata(&self) -> namako_engine::codegen::NpapBindingMetadata {
        namako_engine::codegen::NpapBindingMetadata {
            binding_id: self.binding_id,
            expression: self.expression,
            kind: self.kind,
            impl_hash: self.impl_hash,
            captures_arity: self.captures_arity,
            accepts_docstring: self.accepts_docstring,
            accepts_datatable: self.accepts_datatable,
            source_symbol: self.source_symbol,
        }
    }
}
namako_engine::codegen::collect!(NamakoThenBevyTestWorld);

impl namako_engine::codegen::WorldInventory for BevyTestWorld {
    type Given = NamakoGivenBevyTestWorld;
    type When = NamakoWhenBevyTestWorld;
    type Then = NamakoThenBevyTestWorld;
}
