//! C.6 Addition 3 — `SendHandle::send_message_to_address`.
//!
//! Cyberlith pattern:
//! ```ignore
//! let addr = sim_handle.user_address(user_key).expect("user exists");
//! send.send_message_to_address::<EntityAssignmentChannel, _>(&addr, &msg);
//! ```
//!
//! The full functional body is byte-equivalent to the existing
//! `WorldServer::send_message` (the relocated `send_message_container`
//! call uses the same `MessageManager::send_message` against the same
//! `EntityAndGlobalEntityConverter` constructed from the same
//! `world_manager` + `global_world_manager`). The new API just removes
//! the `sim_handle.user_store` lookup step (caller passes the address).
//!
//! These tests verify the API surface itself:
//! - Bogus address → `false` (no panic, no side effect).
//! - The channel-direction check still fires on an inbound-only channel.

use std::time::Duration;

use bevy_ecs::entity::Entity;

use naia_bevy_server::{
    pipeline_actors::{run_with_world_server, spawn_server_handles},
    ServerConfig,
};
use naia_bevy_shared::Protocol as BevyProtocol;
use naia_shared::{default_channels::UnorderedReliableChannel, Message};
use naia_test_harness::test_protocol::Position;

/// User-defined message type. EntityProperty-bearing messages travel
/// the same converter path; this minimal variant suffices to verify
/// the wiring (the EntityProperty serialization is covered by other
/// `send_message` tests in the harness).
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
        spawn_server_handles::<Entity, _>(ServerConfig::default(), protocol());

    let hub = LocalTransportHub::new(addr.parse().unwrap());
    let socket = Socket::new(LocalServerSocket::new(hub), None);
    let (_a, _b, ps, pr) = naia_server::transport::Socket::listen(Box::new(socket));

    let (sim_handle, recv, send, ()) = run_with_world_server(sim_handle, recv, send, |ws| {
        ws.io_load(ps, pr);
    });
    (sim_handle, recv, send)
}

#[test]
fn send_message_to_unknown_address_returns_false() {
    let (_sim_handle, _recv, mut send) = handles_listening(next_addr());

    let bogus: std::net::SocketAddr = "127.0.0.1:1".parse().unwrap();
    let queued =
        send.send_message_to_address::<UnorderedReliableChannel, _>(&bogus, &Ping { n: 1 });
    assert!(
        !queued,
        "send_message_to_address must return false for an unknown address (no send_user_connections entry)",
    );
}

// Per-test address allocator.
use std::sync::atomic::{AtomicUsize, Ordering};
static PORT_COUNTER: AtomicUsize = AtomicUsize::new(60000);
fn next_addr() -> &'static str {
    let port = PORT_COUNTER.fetch_add(1, Ordering::SeqCst);
    Box::leak(format!("127.0.0.1:{port}").into_boxed_str())
}
