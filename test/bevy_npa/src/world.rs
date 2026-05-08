use std::marker::PhantomData;
use std::sync::Arc;
use std::time::Duration;

use bevy_app::{App, Startup, Update};
use bevy_ecs::{
    message::Messages,
    resource::Resource,
    schedule::IntoScheduleConfigs,
    system::{ResMut, RunSystemOnce},
};
use parking_lot::Mutex as ParkingMutex;

use naia_bevy_server::{
    events::{AuthEvents, ConnectEvent, DisconnectEvent},
    Plugin as ServerPlugin, Server, ServerConfig,
};
use naia_bevy_client::{
    events::{ConnectEvent as ClientConnectEvent, DisconnectEvent as ClientDisconnectEvent},
    Plugin as ClientPlugin, Client, ClientConfig,
};
use naia_bevy_shared::Protocol as BevyProtocol;
use naia_server::transport::local::{LocalServerSocket, Socket as ServerSocket};
use naia_client::transport::local::{LocalAddrCell, LocalClientSocket, Socket as ClientSocket};
use naia_shared::{transport::local::{LocalTransportHub, FAKE_SERVER_ADDR}, ChannelDirection, ChannelMode, ReliableSettings};
use naia_test_harness::{test_protocol::{Auth, ReliableChannel}};

use namako_engine::codegen::AssertOutcome;

fn bevy_protocol() -> BevyProtocol {
    BevyProtocol::builder()
        .add_message::<Auth>()
        .add_channel::<ReliableChannel>(
            ChannelDirection::Bidirectional,
            ChannelMode::UnorderedReliable(ReliableSettings::default()),
        )
        .build()
}

// ── Marker types ─────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ClientKey(pub u32);

pub struct ClientSingleton;

// ── Capture resources ─────────────────────────────────────────────────────────

#[derive(Resource, Default)]
pub struct ServerState {
    pub connected_user_keys: Vec<naia_server::UserKey>,
    pub connect_count: u32,
    pub disconnect_count: u32,
}

#[derive(Resource, Default)]
pub struct ClientState {
    pub connect_count: u32,
    pub disconnect_count: u32,
    pub is_connected: bool,
}

// ── Capture systems ────────────────────────────────────────────────────────────

fn sys_server_auth(mut server: Server, mut auth_msgs: ResMut<Messages<AuthEvents>>) {
    for events in auth_msgs.drain() {
        for (user_key, _auth) in events.read::<Auth>() {
            server.accept_connection(&user_key);
        }
    }
}

fn sys_server_connect(
    mut connect_msgs: ResMut<Messages<ConnectEvent>>,
    mut state: ResMut<ServerState>,
) {
    for event in connect_msgs.drain() {
        state.connected_user_keys.push(event.0);
        state.connect_count += 1;
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

// ── BevyTestHarness ────────────────────────────────────────────────────────────

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
            .add_plugins(ServerPlugin::new(ServerConfig::default(), bevy_protocol()))
            .init_resource::<ServerState>()
            .add_systems(Startup, move |mut server: Server| {
                let socket = ServerSocket::new(
                    LocalServerSocket::new(hub_for_startup.clone()),
                    None,
                );
                server.listen(socket);
                server.create_room();
            })
            .add_systems(
                Update,
                (sys_server_auth, sys_server_connect, sys_server_disconnect)
                    .in_set(naia_bevy_shared::HandleWorldEvents),
            );

        server_app.update(); // Run Startup

        Self { server_app, client_apps: Vec::new(), hub, next_client_id: 0 }
    }

    pub fn tick(&mut self) {
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

        let mut cfg = ClientConfig::default();
        cfg.send_handshake_interval = Duration::from_millis(0);

        let mut app = App::new();
        app.add_plugins(ClientPlugin::<ClientSingleton>::new(cfg, bevy_protocol()))
            .init_resource::<ClientState>()
            .add_systems(Startup, move |mut client: Client<ClientSingleton>| {
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
            })
            .add_systems(
                Update,
                (sys_client_connect, sys_client_disconnect)
                    .in_set(naia_bevy_shared::HandleWorldEvents),
            );

        app.update(); // Run Startup

        self.client_apps.push((key, app));
        key
    }

    // ── State accessors ───────────────────────────────────────────────────────

    pub fn server_connected_count(&self) -> usize {
        self.server_app
            .world()
            .resource::<ServerState>()
            .connected_user_keys
            .len()
    }

    pub fn server_connect_count(&self) -> u32 {
        self.server_app.world().resource::<ServerState>().connect_count
    }

    pub fn server_disconnect_count(&self) -> u32 {
        self.server_app.world().resource::<ServerState>().disconnect_count
    }

    pub fn client_is_connected(&self, key: ClientKey) -> bool {
        self.client_apps
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, app)| app.world().resource::<ClientState>().is_connected)
            .unwrap_or(false)
    }

    pub fn client_connect_count(&self, key: ClientKey) -> u32 {
        self.client_apps
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, app)| app.world().resource::<ClientState>().connect_count)
            .unwrap_or(0)
    }

    pub fn client_disconnect_count(&self, key: ClientKey) -> u32 {
        self.client_apps
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, app)| app.world().resource::<ClientState>().disconnect_count)
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
        let _ = self.server_app.world_mut().run_system_once(move |mut server: Server| {
            server.user_mut(&user_key).disconnect();
        });
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

    fn new() -> impl std::future::Future<Output = Result<Self, Self::Error>> {
        async { Ok(Self::default()) }
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

    pub fn server_connected_count(&self) -> usize {
        self.h().server_connected_count()
    }

    pub fn server_connect_count(&self) -> u32 {
        self.h().server_connect_count()
    }

    pub fn server_disconnect_count(&self) -> u32 {
        self.h().server_disconnect_count()
    }

    pub fn client_is_connected(&self, key: ClientKey) -> bool {
        self.h().client_is_connected(key)
    }

    pub fn client_connect_count(&self, key: ClientKey) -> u32 {
        self.h().client_connect_count(key)
    }

    pub fn client_disconnect_count(&self, key: ClientKey) -> u32 {
        self.h().client_disconnect_count(key)
    }

    pub fn last_client_key(&self) -> Option<ClientKey> {
        self.h().last_client_key()
    }
}

impl<'a> namako_engine::codegen::StepContext for BevyRefCtx<'a> {
    type World = BevyTestWorld;
}

// ── WorldInventory boilerplate ─────────────────────────────────────────────────

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
