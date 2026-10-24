//! Transport boundary contract tests.
//!
//! Contract: `02_transport.spec.md` — packet-size boundary (MTU) and
//! Naia-level error behavior.

use naia_server::ServerConfig;

use naia_test_harness::test_protocol::{ReliableChannel, TestMessage, UnreliableChannel};
use naia_test_harness::{
    protocol, Auth, ExpectCtx, LargeTestMessage, LinkConditionerConfig, Scenario,
};

mod _helpers;
use _helpers::{client_connect, test_client_config};

// ============================================================================
// [transport-03] — Oversize outbound packet attempt returns Err
// ============================================================================

/// Oversize message on a non-fragmenting channel is refused with Err
/// Contract: [transport-03]
///
/// Given a connected client; when the client is asked to send a message
/// whose serialised body exceeds the fragmentation limit (hence any MTU
/// payload) on an unreliable channel that cannot fragment; then the
/// initiating API returns `Err` instead of queueing a guaranteed panic.
/// The same body on a fragmenting (reliable) channel is accepted.
/// (The harness server facade swallows the send `Result`, so the test
/// drives the client initiating API, which propagates it.)
#[test]
fn oversize_unreliable_send_returns_err() {
    let mut scenario = Scenario::new(naia_server::ServerMode::Resident);
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());
    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.create_room().key()));

    let client_a_key = client_connect(
        &mut scenario,
        &room_key,
        "Client A",
        Auth::new("client_a", "pass"),
        test_client_config(),
        test_protocol,
    );
    scenario.mutate(|_ctx| {});

    // Control leg: the same oversize body on a fragmenting channel is
    // accepted (reliable channels split it across packets).
    let reliable_accepted = scenario.mutate(|ctx| {
        ctx.client(client_a_key, |client| {
            client
                .send_message::<ReliableChannel, _>(&LargeTestMessage::new(5000))
                .is_ok()
        })
    });
    assert!(
        reliable_accepted,
        "oversize body on a fragmenting channel must be accepted"
    );

    // Obligation leg: the oversize body on a channel that cannot
    // fragment must be refused with Err at the initiating API.
    let unreliable_refused = scenario.mutate(|ctx| {
        ctx.client(client_a_key, |client| {
            client
                .send_message::<UnreliableChannel, _>(&LargeTestMessage::new(5000))
                .is_err()
        })
    });
    assert!(
        unreliable_refused,
        "oversize body on an unreliable channel must return Err"
    );

    // The refused send queued nothing: the client stays connected and
    // no panic or teardown follows the refusal.
    scenario.spec_expect(
        "transport-03.t1: oversize unreliable send refused, session unaffected",
        |ctx: &mut ExpectCtx<'_>| {
            ctx.client(client_a_key, |c| c.connection_status().is_connected())
                .then_some(())
        },
    );
}

// ============================================================================
// [transport-05] — No transport-specific guarantees leak upward
// ============================================================================

/// Higher layers carry no transport cfgs
/// Contract: [transport-05]
///
/// Given the three core crates (`client`, `server`, `shared`); when their
/// sources are walked excluding the `transport/` subtrees (the adapters)
/// and the `handshake/` subtrees (address-validation, owned by
/// `01_connection_lifecycle`); then no `.rs` file carries a
/// `transport_udp`/`transport_local` cfg outside one allowlisted
/// module-gate line. Messaging, replication, and connection code therefore
/// cannot assume anything about the underlying transport — any guarantee
/// stronger than transport-01 must be proven at the packet boundary
/// (rows 01–04), not inferred from the adapter.
///
/// The positive controls keep this from passing vacuously: the handshake
/// subtrees MUST still contain transport cfgs (the scanner sees branching
/// where it belongs), and the module-gate line itself MUST be found (the
/// scanner reached the gate). Socket crates and Bevy adapters are outside
/// this row's remit (the spec excludes socket-crate behavior; adapters are
/// consumers, and their one transport cfg is `#[cfg(test)]`-only).
#[test]
fn higher_layers_carry_no_transport_cfgs() {
    use std::fs;
    use std::path::{Path, PathBuf};

    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(dir).expect("crate src dir must be readable") {
            let path = entry.expect("dir entry must be readable").path();
            if path.is_dir() {
                let name = path
                    .file_name()
                    .expect("dir must have a name")
                    .to_string_lossy();
                if name == "transport" || name == "handshake" {
                    continue;
                }
                walk(&path, out);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                out.push(path);
            }
        }
    }

    let root = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
    let mut files = Vec::new();
    for krate in ["client/src", "server/src", "shared/src"] {
        walk(&root.join(krate), &mut files);
    }
    assert!(
        !files.is_empty(),
        "walker must find crate sources under {root:?}"
    );

    let mut hits: Vec<(String, usize, String)> = Vec::new();
    for path in &files {
        let text = fs::read_to_string(path).expect("rs file must be readable");
        for (index, line) in text.lines().enumerate() {
            if (line.contains("transport_udp") || line.contains("transport_local"))
                && line.contains("cfg")
            {
                let rel = path
                    .strip_prefix(root)
                    .expect("scanned file must sit under the repo root")
                    .to_string_lossy()
                    .into_owned();
                hits.push((rel, index + 1, line.trim().to_string()));
            }
        }
    }

    // Positive control 1: the handshake subtrees still branch on transport
    // (address-validation lives there, owned by 01_connection_lifecycle).
    for (krate, dir) in [
        ("client", "client/src/handshake"),
        ("server", "server/src/handshake"),
    ] {
        let mut count = 0;
        for entry in fs::read_dir(root.join(dir)).expect("handshake dir must exist") {
            let path = entry.expect("dir entry must be readable").path();
            if path.extension().is_some_and(|ext| ext == "rs") {
                let text = fs::read_to_string(&path).expect("rs file must be readable");
                count += text
                    .lines()
                    .filter(|line| {
                        (line.contains("transport_udp") || line.contains("transport_local"))
                            && line.contains("cfg")
                    })
                    .count();
            }
        }
        assert!(
            count > 0,
            "{krate} handshake must still branch on transport (positive control)"
        );
    }

    // Positive control 2 + the pin: the ONLY hit outside transport/ and
    // handshake/ is the shared transport-module gate in lib.rs.
    let gate = (
        "shared/src/lib.rs".to_string(),
        "if #[cfg(any(feature = \"transport_udp\", feature = \"transport_local\"))]{".to_string(),
    );
    let gate_found = hits
        .iter()
        .any(|(path, _, line)| path == &gate.0 && line == &gate.1);
    assert!(
        gate_found,
        "the shared transport-module gate must be found (positive control); hits: {hits:?}"
    );
    let leaks: Vec<_> = hits
        .iter()
        .filter(|(path, _, line)| path != &gate.0 || line != &gate.1)
        .collect();
    assert!(
        leaks.is_empty(),
        "higher layers must carry no transport cfgs, found: {leaks:?}"
    );
}

// ============================================================================
// [transport-01] — Naia assumes transport is unordered & unreliable
// ============================================================================

/// Reliable delivery survives packet loss at the packet boundary
/// Contract: [transport-01]
///
/// Given a connected client with a lossy server→client leg (20% of
/// packets dropped by the link conditioner); when the server sends on a
/// reliable channel; then the message still arrives (resend covers the
/// drops) and the session stays healthy after the loss clears. Reorder
/// tolerance is pinned at the messaging layer (`03_messaging`
/// hold/overtake leg); duplicate tolerance at the receiver unit level
/// (`duplicates_and_stale_indices_are_still_ignored`).
#[test]
fn reliable_delivery_survives_packet_loss() {
    let mut scenario = Scenario::new(naia_server::ServerMode::Resident);
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());
    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.create_room().key()));

    let client_a_key = client_connect(
        &mut scenario,
        &room_key,
        "Client A",
        Auth::new("client_a", "pass"),
        test_client_config(),
        test_protocol,
    );
    scenario.mutate(|_ctx| {});

    // Drop 20% of server→client packets at the boundary.
    assert!(scenario.configure_link_conditioner(
        &client_a_key,
        None,
        Some(LinkConditionerConfig::new(0, 0, 0.2)),
    ));
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.send_message::<ReliableChannel, _>(&client_a_key, &TestMessage::new(7));
        });
    });

    // The message must still arrive: drops cost ticks, not delivery.
    let mut seen = false;
    scenario.expect(|ctx| {
        ctx.client(client_a_key, |client| {
            for message in client.read_message::<ReliableChannel, TestMessage>() {
                if message.value == 7 {
                    seen = true;
                }
            }
        });
        seen.then_some(())
    });
    assert!(seen, "reliable message must arrive despite 20% packet loss");

    // Clear the loss; the session underneath is healthy, not limping.
    assert!(scenario.configure_link_conditioner(&client_a_key, None, None));
    scenario.spec_expect(
        "transport-01.t1: session healthy after lossy window",
        |ctx: &mut ExpectCtx<'_>| {
            ctx.client(client_a_key, |c| c.connection_status().is_connected())
                .then_some(())
        },
    );
}
