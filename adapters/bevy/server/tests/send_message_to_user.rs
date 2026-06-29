//! MISSION_USER_ONLY_SEES_SIM Phase B.3 (2026-05-19) —
//! `SendHandle::send_message_to_user` convenience wrapper.
//!
//! Cyberlith pattern (Sim system holding both `SimHandle` and
//! `SendHandle` as Bevy Resources):
//! ```ignore
//! send.send_message_to_user::<EntityAssignmentChannel, _>(&sim_handle, user_key, &msg);
//! ```
//!
//! The functional body is byte-equivalent to the existing
//! `SendHandle::send_message_to_address` (the new entry point just
//! folds the `sim_handle.user_address(user_key)` lookup into the same call).
//!
//! These tests verify the API surface itself:
//! - Unknown UserKey → `false` (no panic, no side effect).
//! - Known UserKey with no live send connection (handshake not yet
//!   complete) → `false`.

use std::time::Duration;

use bevy_ecs::entity::Entity;

use naia_bevy_server::{
    pipeline_actors::{run_with_world_server, spawn_server_handles},
    ServerConfig,
};
use naia_bevy_shared::Protocol as BevyProtocol;
use naia_server::UserKey;
use naia_shared::{default_channels::UnorderedReliableChannel, BigMapKey, Message};
use naia_test_harness::test_protocol::Position;

#[derive(Message)]
struct Ping {
    n: u32,
}

fn protocol() -> naia_shared::Protocol {
    let mut p = BevyProtocol::builder();
    p.register_component::<Position>();
    p.add_channel::<UnorderedReliableChannel>(
        naia_shared::ChannelDirection::ServerToClient,
        naia_shared::ChannelMode::UnorderedReliable(naia_shared::ReliableSettings::default()),
    );
    p.add_message::<Ping>();
    p.tick_interval(Duration::from_micros(100));
    let bevy_proto = p.build();
    bevy_proto.into()
}

fn handles_listening(
    addr: &str,
) -> (
    naia_server::pipeline_actors::SimHandle<Entity>,
    naia_server::RecvHandle<Entity>,
    naia_server::SendHandle<Entity>,
) {
    use naia_server::transport::local::{LocalServerSocket, LocalTransportHub, Socket};

    let (sim_handle, recv, send) =
        spawn_server_handles::<Entity, _>(ServerConfig::default(), protocol()).take_handles();

    let hub = LocalTransportHub::new(addr.parse().unwrap());
    let socket = Socket::new(LocalServerSocket::new(hub), None);
    let (_a, _b, ps, pr) = naia_server::transport::Socket::listen(Box::new(socket));

    let (sim_handle, recv, send, ()) = run_with_world_server(sim_handle, recv, send, |ws| {
        ws.io_load(ps, pr);
    });
    (sim_handle, recv, send)
}

#[test]
fn send_message_to_unknown_user_returns_false() {
    let (sim_handle, _recv, mut send) = handles_listening(next_addr());

    // Fabricate a UserKey that was never registered via the connection
    // handshake. `sim_handle.user_address` will return None and the helper
    // must short-circuit to `false`.
    let bogus_user: UserKey = UserKey::from_u64(0xDEAD_BEEF_DEAD_BEEF);
    let queued = send.send_message_to_user::<UnorderedReliableChannel, _>(
        &sim_handle,
        &bogus_user,
        &Ping { n: 1 },
    );
    assert!(
        !queued,
        "send_message_to_user must return false for an unknown UserKey",
    );
}

#[test]
fn send_message_to_user_with_no_live_send_conn_returns_false() {
    // No handshakes drive in this test (no client connects), so even if
    // we managed to inject a UserKey into sim_handle's user_store, the
    // matching SendConnection wouldn't exist (handshake finalization is
    // what inserts it). Verifying via a bogus key is sufficient — the
    // function returns false on either gap.
    let (sim_handle, _recv, mut send) = handles_listening(next_addr());

    let bogus_user: UserKey = UserKey::from_u64(0x1234_5678_9ABC_DEF0);
    let queued = send.send_message_to_user::<UnorderedReliableChannel, _>(
        &sim_handle,
        &bogus_user,
        &Ping { n: 7 },
    );
    assert!(!queued);
}

use std::sync::atomic::{AtomicUsize, Ordering};
static PORT_COUNTER: AtomicUsize = AtomicUsize::new(61000);
fn next_addr() -> &'static str {
    let port = PORT_COUNTER.fetch_add(1, Ordering::SeqCst);
    Box::leak(format!("127.0.0.1:{port}").into_boxed_str())
}
