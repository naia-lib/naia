//! Card 27945 cross-build test: a feature-ON server interoperates with a
//! feature-OFF client.
//!
//! Configuration matrix (via the harness `entity_delegation` feature, ON by
//! default; `cargo test -p naia-test-harness --no-default-features` builds the
//! OFF leg):
//!
//! - ON leg (`cfg(feature = "entity_delegation")`): server + client both have
//!   delegation. Handshake + replication + delegation grant all work — the
//!   behavior-preservation sanity arm.
//! - OFF leg (`cfg(not(...))`): server has `entity_delegation`, the client
//!   does not (same split as production: native full-feature server,
//!   `default-features = false` game-wasm client). Handshake + replication
//!   still complete; the server's delegation attempt FAILS CLOSED on the
//!   client with the named error `AuthorityError::DelegationDisabled` (no
//!   silent drop, no panic), and replication stays healthy afterwards.

#![allow(
    unused_imports,
    unused_variables,
    unused_must_use,
    unused_mut,
    dead_code,
    for_loops_over_fallibles
)]

use naia_client::{ClientConfig, JitterBufferType, Publicity};
use naia_server::{ReplicationConfig, ServerConfig};
use naia_shared::{AuthorityError, EntityAuthStatus};

use naia_test_harness::{protocol, Auth, ClientKey, ExpectCtx, Position, Scenario, ToTicks};

mod _helpers;
use _helpers::{client_connect, test_client_config};

fn setup() -> (Scenario, ClientKey, naia_test_harness::EntityKey) {
    let mut scenario = Scenario::new(naia_server::ServerMode::Resident);
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());
    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.create_room().key()));

    let client_key = client_connect(
        &mut scenario,
        &room_key,
        "Client A",
        Auth::new("client_a", "password"),
        test_client_config(),
        test_protocol,
    );

    // Server spawns E (server-owned, undelegated) in A's scope.
    let entity_e = scenario.mutate(|ctx| {
        ctx.server(|server| {
            let (entity, _) = server.spawn(|mut e| {
                e.insert_component(Position::new(1.0, 2.0));
                e.enter_room(&room_key);
            });
            server.user_scope_mut(&client_key).unwrap().include(&entity);
            entity
        })
    });

    // Handshake + replication round: A sees E with its Position.
    scenario.expect(|ctx| {
        ctx.client(client_key, |c| {
            let entity = c.entity(&entity_e)?;
            let pos = entity.component::<Position>()?;
            (*pos.x == 1.0 && *pos.y == 2.0).then_some(())
        })
    });

    (scenario, client_key, entity_e)
}

/// ON leg: delegation grant works end to end (behavior preservation).
#[cfg(feature = "entity_delegation")]
#[test]
fn on_server_on_client_delegation_grant_works() {
    let (mut scenario, client_key, entity_e) = setup();

    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut entity_mut) = server.entity_mut(&entity_e) {
                entity_mut.configure_replication(ReplicationConfig::delegated());
            }
        });
    });

    // A observes E as Available (the grant flow is intact).
    scenario.expect(|ctx| {
        let status = ctx.client(client_key, |c| {
            c.entity(&entity_e).and_then(|e| e.authority())
        });
        (status == Some(EntityAuthStatus::Available)).then_some(())
    });
}

/// OFF leg: ON-server + OFF-client handshake + replication complete, and the
/// server's delegation attempt fails closed with the named error.
#[cfg(not(feature = "entity_delegation"))]
#[test]
fn on_server_off_client_handshake_replication_and_fail_closed() {
    let (mut scenario, client_key, entity_e) = setup();

    // Server (feature ON) enables delegation on E. The wire EnableDelegation /
    // SetAuthority packets reach the OFF client and must fail closed.
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut entity_mut) = server.entity_mut(&entity_e) {
                entity_mut.configure_replication(ReplicationConfig::delegated());
            }
        });
    });

    // Replication stays healthy while the denial is pending: a Position
    // update flows. This roundtrip also synchronizes the test — once the
    // update arrives, the earlier-queued delegation packets have necessarily
    // reached the client too.
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut entity_mut) = server.entity_mut(&entity_e) {
                entity_mut.insert_component(Position::new(7.0, 8.0));
            }
        });
    });
    scenario.expect(|ctx| {
        ctx.client(client_key, |c| {
            let entity = c.entity(&entity_e)?;
            let pos = entity.component::<Position>()?;
            (*pos.x == 7.0 && *pos.y == 8.0).then_some(())
        })
    });

    // The delegation state is NOT applied (no Available status).
    scenario.expect(|ctx| {
        let no_status = ctx.client(client_key, |c| {
            c.entity(&entity_e).and_then(|e| e.authority()).is_none()
        });
        no_status.then_some(())
    });

    // The named error is recorded (no silent drop, no panic).
    let recorded = scenario.mutate(|ctx| {
        ctx.client(client_key, |c| {
            c.take_authority_error() == Some(AuthorityError::DelegationDisabled)
        })
    });
    assert!(recorded, "expected DelegationDisabled to be recorded");

    // Local authority API fails closed with the same named error.
    let result_err = scenario.mutate(|ctx| {
        ctx.client(client_key, |c| {
            if let Some(mut entity_mut) = c.entity_mut(&entity_e) {
                entity_mut.request_authority().err()
            } else {
                Some(AuthorityError::NotInScope)
            }
        })
    });
    assert_eq!(result_err, Some(AuthorityError::DelegationDisabled));
}
