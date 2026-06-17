//! MISSION_USER_ONLY_SEES_SIM Phase D.3a — PlayerCommand tick-buffer
//! drain through `Plugin::sim_integration_full`.
//!
//! # The gap (pre-D.3a)
//!
//! The `sim_integration_full` recv worker permanently owned the
//! `RecvHandle` and only ran `recv.receive()`. There was no
//! `RecvHandleRes`, so a Sim system could not reach the handle to call
//! `RecvHandle::receive_tick_buffer_messages(tick)` — the per-tick call
//! cyberlith makes to drain naia's per-user tick-buffered
//! `PlayerCommands`. Adopting the plugin as-is would silently drop ALL
//! player input. This was the sole blocker for cyberlith Phase E.6.
//!
//! # The D.3a mechanism (under test)
//!
//! The recv (and send) worker now borrow their handle from a shared
//! park-window slot (`Arc<Mutex<Option<_>>>`) per loop iteration,
//! depositing it at the park checkpoint. `RecvHandleRes` / `SendHandleRes`
//! wrap those slots. A Sim system:
//!   1. parks the workers (`PluginInternalState::park_workers`),
//!   2. takes the `RecvHandle` from the slot,
//!   3. calls `receive_tick_buffer_messages(tick)` for each tick,
//!   4. returns the handle and unparks.
//! The park barrier makes the take/return race-free (D6 / Phase-G
//! discipline) — see the borrow-contract docs on `RecvHandleRes`.
//!
//! # Why injection rather than a live client handshake
//!
//! The pipeline's auth / `receive_user` connection-lifecycle layer is a
//! separate (D.3b) concern not yet wired into `sim_integration_full`, so
//! a real client cannot complete a handshake against it today. To
//! exercise the tick-buffer drain we manufacture a recv connection and
//! inject tick-buffered messages directly via the `test_utils`-gated
//! `RecvHandle::inject_tick_buffer_message_for_test` — mirroring exactly
//! the per-user tick-buffer state a real handshake + `process_recv_packets`
//! decode would leave behind. The borrow path the Sim system exercises
//! (park → take from `RecvHandleRes` slot → drain → return → unpark) is
//! the real, un-mocked mechanism.

use std::{thread, time::Duration};

use bevy_app::App;

use naia_bevy_server::{
    transport, Plugin as ServerPlugin, PluginInternalState, PluginSimConfig, RecvHandleRes,
    ServerConfig,
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
    app.add_plugins(ServerPlugin::sim_integration_full(
        ServerConfig::default(),
        protocol(),
        PluginSimConfig::default(),
    ));
    app
}

fn local_socket(addr: &str) -> Box<dyn transport::Socket> {
    let hub = LocalTransportHub::new(addr.parse().unwrap());
    Box::new(Socket::new(LocalServerSocket::new(hub), None))
}

fn listen(app: &mut App, addr: &str) {
    let socket = local_socket(addr);
    // Listen + run one update so the armed sim_handle drains and the workers
    // start spinning.
    app.world().resource::<PluginInternalState>().listen(socket);
    app.update();
    thread::sleep(Duration::from_millis(15));
}

#[test]
fn tick_buffered_messages_drain_in_per_tick_order_via_park_window() {
    let mut app = build_app();
    listen(&mut app, "127.0.0.1:23040");

    let addr: std::net::SocketAddr = "127.0.0.1:54400".parse().unwrap();
    let user_key = UserKey::from_u64(7);

    // Inject one tick-buffered TestMessage per tick across several ticks.
    // Per the TickBufferReceiver contract the message_tick must be in the
    // future of host_tick; we inject at host=H, message=H+2, then drain
    // at the message tick. Values increase monotonically so we can assert
    // per-tick FIFO order is preserved end-to-end.
    let ticks: Vec<u16> = vec![12, 13, 14, 15, 16];
    let mut expected: Vec<(u16, u32)> = Vec::new();

    let state = app.world().resource::<PluginInternalState>();
    state.park_workers();
    let recv_slot = {
        let res = app.world().resource::<RecvHandleRes>();
        res.0.clone()
    };

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

    app.world()
        .resource::<PluginInternalState>()
        .unpark_workers();

    // Workers must still be alive + making progress after the borrow.
    thread::sleep(Duration::from_millis(10));
    app.world()
        .resource::<PluginInternalState>()
        .propagate_panic_if_any();

    // Every injected command was drained, NONE dropped, and per-tick
    // order preserved (values strictly increasing in arrival order).
    assert_eq!(
        drained, expected,
        "all tick-buffered commands drained in per-tick order; got {drained:?}, expected {expected:?}",
    );

    drop(app);
}

#[test]
fn park_window_recv_borrow_is_race_free_under_repeated_cycles() {
    // Hammer the park → take RecvHandle → return → unpark cycle while the
    // workers are actively spinning, to surface any race in the
    // deposit/claim handoff. No client is connected, so the drain returns
    // nothing — the point is that the handle is always present in the slot
    // once parked, and the workers always re-claim it on unpark and keep
    // running (no panic, no lost handle, no deadlock).
    let mut app = build_app();
    listen(&mut app, "127.0.0.1:23041");

    for cycle in 0..25 {
        let state = app.world().resource::<PluginInternalState>();
        state.park_workers();

        let recv_slot = app.world().resource::<RecvHandleRes>().0.clone();
        let recv = recv_slot.lock().take();
        assert!(
            recv.is_some(),
            "RecvHandle must be in the slot once workers are parked (cycle {cycle})",
        );
        // Touch the handle (drain an arbitrary tick — returns nothing).
        let mut recv = recv.unwrap();
        let mut msgs = recv.receive_tick_buffer_messages(&(cycle as u16));
        let drained = msgs.read::<TickBufferedChannel, TestMessage>();
        assert!(drained.is_empty(), "no client → no tick-buffer messages");
        *recv_slot.lock() = Some(recv);

        app.world()
            .resource::<PluginInternalState>()
            .unpark_workers();
        // Let the worker re-claim + spin briefly.
        thread::sleep(Duration::from_millis(2));
        app.world()
            .resource::<PluginInternalState>()
            .propagate_panic_if_any();
    }

    drop(app);
}
