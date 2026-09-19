//! naia-lib/naia#207 follow-up (OriginMap eviction, Usher decision 12121):
//! the fan-in origin map must drain when clients leave, and eviction must
//! never run ahead of the disconnect packets.
//!
//! The server here listens through a single-inner `MultiSocket` (one hub
//! means one inner — two server sockets on the same hub would each see
//! every client). A single inner still exercises the full origin-map path
//! (record on receive, route on send, evict on forget); multi-inner fan-in
//! itself is covered in `naia-server`'s transport tests.
//!
//! Origin-drain is observed behaviorally through a `test_utils` clone of
//! the loaded packet sender: while an address's origin entry is live,
//! `send` routes into the local hub and returns `Ok`; once evicted, `send`
//! refuses with `Err` before touching any inner.
//!
//! Probing subtlety: a client that consumes its disconnect/reject burst
//! tears down and drops its hub channel, after which a refused raw send
//! reports the hub, not the origin map. The drain tests therefore stage
//! the kick/reject with `stage_without_ticking` (no flush tick) and then
//! drive only the server with `tick_server_only`: the full real teardown
//! — burst attempt, world delete, main delete — runs, but no client ever
//! updates, so every hub channel stays live and `Ok` vs `Err` reports ONLY
//! the origin map. Completion is observed tick-free through `users_count`
//! (the main-side count, which reaches zero exactly when the evicting
//! delete runs). Delivery itself is proved by the unpaused ordering test.
//! All raw-send assertions run with clients still frozen, so no client
//! ever ticks on the probe bytes.

#![allow(
    unused_imports,
    unused_variables,
    unused_must_use,
    unused_mut,
    dead_code,
    for_loops_over_fallibles
)]

use std::net::SocketAddr;

use naia_client::ClientConfig;
use naia_server::{transport::PacketSender, ServerConfig};
use naia_shared::DisconnectReason;

use naia_test_harness::{
    protocol, Auth, ClientDisconnectEvent, ClientKey, Scenario, ServerAuthEvent,
};

mod _helpers;
use _helpers::{client_connect, server_and_client_connected};

fn connect_one(
    scenario: &mut Scenario,
    room_key: &naia_server::RoomKey,
    name: &str,
    test_protocol: &naia_shared::Protocol,
) -> ClientKey {
    let key = client_connect(
        scenario,
        room_key,
        name,
        Auth::new(name, "password"),
        ClientConfig::default(),
        test_protocol.clone(),
    );
    scenario.mutate(|_ctx| {});
    scenario.expect(|ctx| server_and_client_connected(ctx, key));
    scenario.mutate(|_ctx| {});
    key
}

fn start_multi_server(scenario: &mut Scenario, test_protocol: &naia_shared::Protocol) {
    scenario.server_start_with_multi_socket(ServerConfig::default(), test_protocol.clone());
}

fn main_users_count(scenario: &Scenario) -> usize {
    scenario
        .server()
        .expect("server must be running")
        .users_count()
}

fn raw_sender(scenario: &Scenario) -> Box<dyn PacketSender> {
    scenario
        .server()
        .expect("server must be running")
        .sender_cloned_for_tests()
}

/// Drive only the server (clients stay frozen, hub channels stay live)
/// until the main-side user count reaches `want`.
fn drive_server_until(scenario: &mut Scenario, want: usize, label: &str) {
    for _ in 0..120 {
        if main_users_count(scenario) == want {
            return;
        }
        scenario.tick_server_only();
    }
    panic!(
        "{label}: server teardown stalled (users_count={})",
        main_users_count(scenario)
    );
}

/// 12121.3(a): N clients connect through the MultiSocket front; the server
/// kicks all three through the real teardown path (world delete, burst
/// attempt, main delete); every origin entry must be evicted (raw sends
/// refuse) and no user may remain.
#[test]
fn origin_map_drains_after_clients_disconnect() {
    let mut scenario = Scenario::new(naia_server::ServerMode::Resident);
    let test_protocol = protocol();
    start_multi_server(&mut scenario, &test_protocol);
    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.create_room().key()));

    let keys = ["evict-a", "evict-b", "evict-c"]
        .iter()
        .map(|name| connect_one(&mut scenario, &room_key, name, &test_protocol))
        .collect::<Vec<_>>();
    scenario.expect(|ctx| (ctx.server(|s| s.users_count()) == 3).then_some(()));

    let addrs: Vec<SocketAddr> = keys
        .iter()
        .map(|key| scenario.client_addr(*key).expect("client addr assigned"))
        .collect();

    // Kick all three without any tick (staged, not flushed), then drive
    // only the server through the real path: user_queue_disconnect ->
    // world user_disconnect/user_delete -> main disconnect_user (burst
    // attempt into the frozen clients' live channels) -> main user_delete.
    scenario.stage_without_ticking(|ctx| {
        ctx.server(|server| {
            for key in &keys {
                assert!(server.disconnect_user(key), "kick must queue");
            }
        });
    });
    drive_server_until(&mut scenario, 0, "triple kick");

    // Drain proof, clients still frozen: every evicted address must now
    // refuse sends, reporting the origin map and nothing else.
    let sender = raw_sender(&scenario);
    for addr in &addrs {
        assert!(
            sender.send(addr, &[7]).is_err(),
            "origin entry for disconnected {addr} was not evicted"
        );
    }
}

/// 12121.3(b), ORDERING control: a server-kicked client routed through
/// MultiSocket still receives its disconnect packet. Eviction must run
/// strictly after the disconnect burst; if it ever runs early, this goes
/// red (client would time out instead of seeing `Kicked`).
#[test]
fn disconnecting_client_still_receives_disconnect_packet() {
    let mut scenario = Scenario::new(naia_server::ServerMode::Resident);
    let test_protocol = protocol();
    start_multi_server(&mut scenario, &test_protocol);
    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.create_room().key()));

    let key = connect_one(&mut scenario, &room_key, "kick-me", &test_protocol);

    scenario.mutate(|ctx| {
        ctx.server(|server| {
            assert!(server.disconnect_user(&key), "kick must queue");
        });
    });
    scenario.expect(|ctx| {
        ctx.client(key, |client| {
            if let Some((reason, _message)) = client.read_event::<ClientDisconnectEvent>() {
                (reason == DisconnectReason::Kicked).then_some(())
            } else {
                None
            }
        })
    });
    scenario.mutate(|_ctx| {});
    scenario.expect(|ctx| (ctx.server(|s| s.users_count()) == 0).then_some(()));
}

/// 12121.3(c): a live client keeps its entry — kicking B must not disturb
/// A's routing. B is gone server-side (its address refuses); A stays
/// present and its address still routes.
#[test]
fn live_client_keeps_origin_after_other_disconnects() {
    let mut scenario = Scenario::new(naia_server::ServerMode::Resident);
    let test_protocol = protocol();
    start_multi_server(&mut scenario, &test_protocol);
    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.create_room().key()));

    let key_a = connect_one(&mut scenario, &room_key, "stays", &test_protocol);
    let key_b = connect_one(&mut scenario, &room_key, "leaves", &test_protocol);
    let addr_a = scenario.client_addr(key_a).expect("client addr assigned");
    let addr_b = scenario.client_addr(key_b).expect("client addr assigned");

    scenario.stage_without_ticking(|ctx| {
        ctx.server(|server| {
            assert!(server.disconnect_user(&key_b), "kick must queue");
        });
    });
    drive_server_until(&mut scenario, 1, "single kick");

    // Clients still frozen: B's address refuses (evicted), A's still routes.
    let sender = raw_sender(&scenario);
    assert!(
        sender.send(&addr_b, &[7]).is_err(),
        "disconnected client's origin entry was not evicted"
    );
    assert!(
        sender.send(&addr_a, &[7]).is_ok(),
        "live client's origin entry must survive another client's disconnect"
    );
}

/// 12121.1, auth plane: a rejected auth knock records an origin (the knock
/// arrived on an inner) and the reject path must evict it.
/// Unpaused reject delivery is covered by `01_connection_lifecycle`.
#[test]
fn rejected_auth_leaves_no_origin() {
    let mut scenario = Scenario::new(naia_server::ServerMode::Resident);
    let test_protocol = protocol();
    start_multi_server(&mut scenario, &test_protocol);
    let _room_key = scenario.mutate(|ctx| ctx.server(|server| server.create_room().key()));

    // C starts the handshake but is never accepted.
    let key_c = scenario.client_start(
        "rejected",
        Auth::new("rejected", "password"),
        ClientConfig::default(),
        test_protocol.clone(),
    );
    let addr_c = scenario.client_addr(key_c).expect("client addr assigned");
    scenario.mutate(|_ctx| {});
    // The knock arrived server-side: its origin is now recorded.
    scenario.expect(|ctx| {
        ctx.server(|server| {
            server
                .read_event::<ServerAuthEvent<Auth>>()
                .is_some()
                .then_some(())
        })
    });

    // Reject without any tick, then drive only the server: reject response
    // attempt into the frozen client's live channel, then user_delete.
    scenario.stage_without_ticking(|ctx| {
        ctx.server(|server| {
            server.reject_connection(&key_c);
        });
    });
    drive_server_until(&mut scenario, 0, "reject");

    // Clients still frozen: the rejected knock's entry must be gone.
    let sender = raw_sender(&scenario);
    assert!(
        sender.send(&addr_c, &[7]).is_err(),
        "rejected auth knock's origin entry was not evicted"
    );
}
