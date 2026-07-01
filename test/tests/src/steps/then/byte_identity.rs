//! Then-step bindings: byte-exact resident/oracle comparisons.

use std::{net::SocketAddr, sync::Arc, time::Duration};

use crate::steps::prelude::*;
use naia_client::{
    transport::local::{LocalAddrCell, LocalClientSocket, Socket as ClientSocket},
    Client, ClientConfig, JitterBufferType,
};
use naia_server::{
    transport::local::{LocalServerSocket, Socket as ServerSocket},
    ConnectEvent, ReplicationConfig, Server, ServerConfig, ServerMode,
};
use naia_shared::{transport::local::LocalTransportHub, Instant, TestClock, WorldMutType};
use naia_test_harness::{protocol, Auth, TestEntity, TestScore, TestWorld};
use parking_lot::Mutex;

fn client_config() -> ClientConfig {
    let mut config = ClientConfig::default();
    config.send_handshake_interval = Duration::from_millis(0);
    config.jitter_buffer = JitterBufferType::Bypass;
    config
}

fn trace_summary(packets: &[Vec<u8>]) -> String {
    packets
        .iter()
        .enumerate()
        .map(|(index, bytes)| {
            let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
            format!("[{index}] len={} {hex}", bytes.len())
        })
        .collect::<Vec<_>>()
        .join("; ")
}

struct DirectScopeRun {
    hub: LocalTransportHub,
    server: Server<TestEntity>,
    server_world: TestWorld,
    client: Client<TestEntity>,
    client_world: TestWorld,
}

impl DirectScopeRun {
    fn new(mode: ServerMode) -> Self {
        TestClock::init(0);
        let server_addr: SocketAddr = "127.0.0.1:54590".parse().unwrap();
        let hub = LocalTransportHub::new(server_addr);

        let mut server = Server::new(mode, ServerConfig::default(), protocol());
        let server_socket = ServerSocket::new(LocalServerSocket::new(hub.clone()), None);
        server.listen(server_socket);

        let (client_addr, auth_req_tx, auth_resp_rx, client_data_tx, client_data_rx) =
            hub.register_client();
        let addr_cell = LocalAddrCell::new();
        addr_cell.set_sync(hub.server_addr());
        let identity_token = Arc::new(Mutex::new(None));
        let rejection_code = Arc::new(Mutex::new(None));
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
        let mut client = Client::new(client_config(), protocol());
        client.auth(Auth::new("user", "password"));
        client.connect(socket);

        Self {
            hub,
            server,
            server_world: TestWorld::default(),
            client,
            client_world: TestWorld::default(),
        }
    }

    fn tick_classic(&mut self) {
        TestClock::advance(16);
        let now = Instant::now();
        self.hub.process_time_queues();

        self.client.receive_all_packets();
        self.client
            .process_all_packets(self.client_world.proxy_mut(), &now);
        self.client.send_all_packets(self.client_world.proxy_mut());

        self.server.receive_all_packets();
        self.server
            .process_all_packets(self.server_world.proxy_mut(), &now);
        self.server.send_all_packets(self.server_world.proxy());
    }

    fn tick_bracket(&mut self) {
        TestClock::advance(16);
        let now = Instant::now();
        self.hub.process_time_queues();

        self.client.receive_all_packets();
        self.client
            .process_all_packets(self.client_world.proxy_mut(), &now);
        self.client.send_all_packets(self.client_world.proxy_mut());

        self.server.receive(self.server_world.proxy_mut());
        self.server.send(self.server_world.proxy());
    }

    fn connect(&mut self) -> Result<naia_server::UserKey, String> {
        for _ in 0..64 {
            self.tick_classic();
            let mut events = self.server.take_world_events();
            let auths: Vec<_> = events
                .read::<naia_server::AuthEvent<Auth>>()
                .map(|(user_key, _auth)| user_key)
                .collect();
            for user_key in auths {
                self.server.accept_connection(&user_key);
            }
            if let Some(user_key) = events.read::<ConnectEvent>().next() {
                return Ok(user_key);
            }
        }
        Err("client did not connect".into())
    }

    fn setup_scoped_entity(
        &mut self,
        user_key: &naia_server::UserKey,
    ) -> naia_test_harness::TestEntity {
        let room_key = self.server.create_room().key();
        self.server.room_mut(&room_key).add_user(user_key);
        let entity = self.server_world.proxy_mut().spawn_entity();
        self.server
            .entity_mut(self.server_world.proxy_mut(), &entity)
            .enable_replication()
            .configure_replication(ReplicationConfig::public())
            .enter_room(&room_key);

        for _ in 0..32 {
            self.tick_classic();
        }
        entity
    }

    fn scope_trace(mut self) -> Result<Vec<Vec<u8>>, String> {
        let user_key = self.connect()?;
        let entity = self.setup_scoped_entity(&user_key);
        self.hub.enable_packet_recording();

        self.server.user_scope_mut(&user_key).exclude(&entity);
        self.tick_bracket();
        self.tick_bracket();
        self.server.user_scope_mut(&user_key).include(&entity);
        self.tick_bracket();
        self.tick_bracket();

        Ok(self
            .hub
            .take_recorded_packets()
            .into_iter()
            .filter_map(|(server_to_client, bytes)| server_to_client.then_some(bytes))
            .collect())
    }

    fn resource_trace(mut self) -> Result<Vec<Vec<u8>>, String> {
        let _user_key = self.connect()?;
        self.hub.enable_packet_recording();

        self.server
            .insert_resource(self.server_world.proxy_mut(), TestScore::new(7, 3), false)
            .map_err(|error| format!("resource insert failed: {error:?}"))?;
        self.tick_bracket();
        self.tick_bracket();
        if !self
            .server
            .remove_resource::<_, TestScore>(self.server_world.proxy_mut())
        {
            return Err("resource remove returned false".into());
        }
        self.tick_bracket();
        self.tick_bracket();

        Ok(self
            .hub
            .take_recorded_packets()
            .into_iter()
            .filter_map(|(server_to_client, bytes)| server_to_client.then_some(bytes))
            .collect())
    }
}

/// Then pipelined D7 scope-ledger bytes match the resident oracle.
#[then("pipelined D7 scope-ledger bytes match the resident oracle")]
fn then_pipelined_d7_scope_ledger_bytes_match_resident_oracle(
    _ctx: &TestWorldRef,
) -> AssertOutcome<()> {
    let resident = match DirectScopeRun::new(ServerMode::Resident).scope_trace() {
        Ok(trace) => trace,
        Err(error) => return AssertOutcome::Failed(format!("resident setup failed: {error}")),
    };
    let pipelined = match DirectScopeRun::new(ServerMode::Pipelined).scope_trace() {
        Ok(trace) => trace,
        Err(error) => return AssertOutcome::Failed(format!("pipelined setup failed: {error}")),
    };

    if resident == pipelined {
        AssertOutcome::Passed(())
    } else {
        AssertOutcome::Failed(format!(
            "D7 scope-ledger byte trace diverged: resident={} pipelined={}",
            trace_summary(&resident),
            trace_summary(&pipelined)
        ))
    }
}

/// Then pipelined D2 resource bytes match the resident oracle.
#[then("pipelined D2 resource bytes match the resident oracle")]
fn then_pipelined_d2_resource_bytes_match_resident_oracle(
    _ctx: &TestWorldRef,
) -> AssertOutcome<()> {
    let resident = match DirectScopeRun::new(ServerMode::Resident).resource_trace() {
        Ok(trace) => trace,
        Err(error) => return AssertOutcome::Failed(format!("resident setup failed: {error}")),
    };
    let pipelined = match DirectScopeRun::new(ServerMode::Pipelined).resource_trace() {
        Ok(trace) => trace,
        Err(error) => return AssertOutcome::Failed(format!("pipelined setup failed: {error}")),
    };

    if resident == pipelined {
        AssertOutcome::Passed(())
    } else {
        AssertOutcome::Failed(format!(
            "D2 resource byte trace diverged: resident={} pipelined={}",
            trace_summary(&resident),
            trace_summary(&pipelined)
        ))
    }
}
