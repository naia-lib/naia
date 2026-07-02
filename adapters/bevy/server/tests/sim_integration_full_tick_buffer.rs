//! MISSION_PIPELINE_API_BOUNDARY §2f — PlayerCommand tick-buffer drain through
//! `Topology::WorldProxied(DriveShape::Pipelined(_))`.
//!
//! # The mechanism (under test)
//!
//! A Sim system reaches the pipeline's `RecvHandle` to call
//! `RecvHandle::receive_tick_buffer_messages(tick)` — the per-tick call cyberlith
//! makes to drain naia's per-user tick-buffered `PlayerCommands`. Under §2f the
//! pipeline lives inside the unified `WorldServer` resource; the consumer:
//!   1. parks the workers (`Server::pipeline_park`),
//!   2. reaches the pipeline via `Server::world_only_resource_scope` and takes
//!      the `RecvHandle` from its shared slot (`WorldServer::recv_slot`),
//!   3. calls `receive_tick_buffer_messages(tick)` per tick,
//!   4. returns the handle and unparks.
//! The park barrier makes the take/return race-free (D6 / Phase-G discipline).
//!
//! # Why injection rather than a live client handshake
//!
//! A real client cannot complete a handshake against the pipelined plugin today,
//! so we manufacture a recv connection and inject tick-buffered messages directly
//! via the `test_utils`-gated `RecvHandle::inject_tick_buffer_message_for_test` —
//! mirroring the per-user tick-buffer state a real handshake +
//! `process_recv_packets` decode would leave behind. The borrow path the Sim
//! system exercises (park → take from the recv slot → drain → return → unpark)
//! is the real, un-mocked mechanism.

use std::{thread, time::Duration};

use bevy_app::App;
use bevy_ecs::entity::Entity;

use naia_bevy_server::{
    transport, PipelineConfig, Plugin as ServerPlugin, RecvHandle, Server, ServerConfig,
};
use naia_bevy_shared::Protocol as BevyProtocol;
use naia_server::transport::local::{LocalServerSocket, LocalTransportHub, Socket};
use naia_server::{shared::BigMapKey, UserKey};

use naia_test_harness::test_protocol::{Position, TestMessage, TickBufferedChannel};

fn protocol() -> BevyProtocol {
    let mut p = BevyProtocol::builder();
    p.add_message::<TestMessage>()
        .add_channel::<TickBufferedChannel>(
            naia_shared::ChannelDirection::ClientToServer,
            naia_shared::ChannelMode::TickBuffered(naia_shared::TickBufferSettings::default()),
        )
        .register_component::<Position>();
    p.tick_interval(Duration::from_micros(100));
    p.build()
}

fn build_app() -> App {
    let mut app = App::new();
    app.add_plugins(ServerPlugin::new(
        naia_bevy_server::ServerPluginConfig::new(
            ServerConfig::default(),
            protocol(),
            naia_bevy_server::Topology::WorldProxied(naia_bevy_server::DriveShape::Pipelined(
                PipelineConfig::default(),
            )),
        ),
    ));
    app
}

fn local_socket(addr: &str) -> Box<dyn transport::Socket> {
    let hub = LocalTransportHub::new(addr.parse().unwrap());
    Box::new(Socket::new(LocalServerSocket::new(hub), None))
}

fn listen(app: &mut App, addr: &str) {
    Server::pipeline_listen(app.world_mut(), local_socket(addr));
    Server::pipeline_start(app.world_mut());
    app.update();
    thread::sleep(Duration::from_millis(15));
}

/// Clone the pipeline's recv-handle slot (the same `Arc` the recv worker borrows
/// from in active builds). The workers must be parked before taking the handle.
fn recv_slot(app: &mut App) -> std::sync::Arc<parking_lot::Mutex<Option<RecvHandle<Entity>>>> {
    Server::world_only_resource_scope(app.world_mut(), |_world, ws| ws.recv_slot())
}

#[test]
fn tick_buffered_messages_drain_in_per_tick_order_via_park_window() {
    let mut app = build_app();
    listen(&mut app, "127.0.0.1:23040");

    let addr: std::net::SocketAddr = "127.0.0.1:54400".parse().unwrap();
    let user_key = UserKey::from_u64(7);

    let ticks: Vec<u16> = vec![12, 13, 14, 15, 16];
    let mut expected: Vec<(u16, u32)> = Vec::new();

    Server::pipeline_park(app.world());
    let recv_slot = recv_slot(&mut app);

    // ── Inject phase (parked) ──────────────────────────────────────────
    {
        let mut recv = recv_slot
            .lock()
            .take()
            .expect("RecvHandle in slot while workers parked");
        for (i, message_tick) in ticks.iter().copied().enumerate() {
            let host_tick = message_tick.wrapping_sub(2);
            let value = i as u32 + 100;
            let accepted = recv
                .inject_tick_buffer_message_for_test::<TickBufferedChannel, TestMessage>(
                    addr,
                    &user_key,
                    &host_tick,
                    &message_tick,
                    TestMessage::new(value),
                );
            assert!(
                accepted,
                "tick-buffer inject should be accepted for tick {message_tick}"
            );
            expected.push((message_tick, value));
        }
        *recv_slot.lock() = Some(recv);
    }

    // ── Drain phase (still parked — simulates the Sim per-tick drain) ───
    let mut drained: Vec<(u16, u32)> = Vec::new();
    {
        let mut recv = recv_slot.lock().take().expect("RecvHandle still in slot");
        for message_tick in ticks.iter().copied() {
            let mut msgs = recv.receive_tick_buffer_messages(&message_tick);
            for (uk, msg) in msgs.read::<TickBufferedChannel, TestMessage>() {
                assert_eq!(uk, user_key, "message attributed to the injected user");
                drained.push((message_tick, msg.value));
            }
        }
        *recv_slot.lock() = Some(recv);
    }

    Server::pipeline_unpark(app.world());

    // Workers (if any) must still be alive + making progress after the borrow.
    thread::sleep(Duration::from_millis(10));
    Server::pipeline_propagate_panics(app.world());

    // Every injected command was drained, NONE dropped, and per-tick order
    // preserved (values strictly increasing in arrival order).
    assert_eq!(
        drained, expected,
        "all tick-buffered commands drained in per-tick order; got {drained:?}, expected {expected:?}",
    );

    drop(app);
}

#[test]
fn park_window_recv_borrow_is_race_free_under_repeated_cycles() {
    // Hammer the park → take RecvHandle → return → unpark cycle. No client is
    // connected, so the drain returns nothing — the point is that the handle is
    // always present in the slot once parked, and (in active builds) the workers
    // always re-claim it on unpark and keep running.
    let mut app = build_app();
    listen(&mut app, "127.0.0.1:23041");

    for cycle in 0..25 {
        Server::pipeline_park(app.world());

        let recv_slot = recv_slot(&mut app);
        let recv = recv_slot.lock().take();
        assert!(
            recv.is_some(),
            "RecvHandle must be in the slot once workers are parked (cycle {cycle})",
        );
        let mut recv = recv.unwrap();
        let mut msgs = recv.receive_tick_buffer_messages(&(cycle as u16));
        let drained = msgs.read::<TickBufferedChannel, TestMessage>();
        assert!(drained.is_empty(), "no client → no tick-buffer messages");
        *recv_slot.lock() = Some(recv);

        Server::pipeline_unpark(app.world());
        thread::sleep(Duration::from_millis(2));
        Server::pipeline_propagate_panics(app.world());
    }

    drop(app);
}
