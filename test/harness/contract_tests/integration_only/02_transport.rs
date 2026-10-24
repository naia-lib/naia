//! Transport boundary contract tests.
//!
//! Contract: `02_transport.spec.md` — packet-size boundary (MTU) and
//! Naia-level error behavior.

use naia_server::ServerConfig;

use naia_test_harness::test_protocol::{ReliableChannel, UnreliableChannel};
use naia_test_harness::{protocol, Auth, ExpectCtx, LargeTestMessage, Scenario};

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
