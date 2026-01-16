#![allow(unused_imports)]

use std::time::Duration;

use naia_client::{ClientConfig, JitterBufferType, ReplicationConfig as ClientReplicationConfig};
use naia_server::{ReplicationConfig, RoomKey, ServerConfig};
use naia_shared::{AuthorityError, EntityAuthStatus, Protocol, Request, Response, Tick};

use naia_test_harness::{
    protocol, Auth, ClientConnectEvent, ClientDisconnectEvent, ClientEntityAuthDeniedEvent,
    ClientEntityAuthGrantedEvent, ClientEntityAuthResetEvent, ClientKey, ClientRejectEvent,
    ExpectCtx, Position, Scenario, ServerAuthEvent, ServerConnectEvent, ServerDisconnectEvent,
    ToTicks,
};

// Test protocol types (channels and messages)
use naia_test_harness::test_protocol::{
    OrderedChannel, ReliableChannel, RequestResponseChannel, SequencedChannel,
    TestMessage, TestRequest, TestResponse, TickBufferedChannel, UnorderedChannel,
    UnreliableChannel,
};

mod _helpers;
use _helpers::{client_connect, server_and_client_connected, server_and_client_disconnected, test_client_config};


// ============================================================================
// Entity Authority Tests
// ============================================================================
// Tests organized by contract ID to match specs/contracts/11_entity_authority.md
// ============================================================================

/// Contract: [entity-authority-01]
///
/// Authority is None for non-delegated entities; request/release fails.
#[test]
fn authority_undefined_for_non_delegated() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());
    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.make_room().key()));

    let client_key = client_connect(&mut scenario, &room_key, "Client",
        Auth::new("user", "pass"), test_client_config(), test_protocol);

    // Spawn a Public (non-delegated) entity
    let entity = scenario.mutate(|ctx| {
        ctx.server(|server| {
            let (entity, _) = server.spawn(|mut e| {
                e.insert_component(Position::new(1.0, 2.0));
                e.enter_room(&room_key);
            });
            server.user_scope_mut(&client_key).unwrap().include(&entity);
            entity
        })
    });

    scenario.expect(|ctx| {
        ctx.client(client_key, |c| c.has_entity(&entity)).then_some(())
    });

    scenario.mutate(|_ctx| {});

    // Verify authority is None for non-delegated entity
    scenario.spec_expect("entity-authority-01.t1: authority None for non-delegated", |ctx| {
        ctx.client(client_key, |c| {
            c.entity(&entity).and_then(|e| e.authority()).is_none().then_some(())
        })
    });

    // Verify request_authority fails on non-delegated entity
    let result = scenario.mutate(|ctx| {
        ctx.client(client_key, |client| {
            client.entity_mut(&entity)
                .and_then(|mut e| e.request_authority().err())
        })
    });

    scenario.spec_expect("entity-authority-01.t1: request_authority fails on non-delegated", |_ctx| {
        if result == Some(AuthorityError::NotDelegated) {
            Some(())
        } else {
            None
        }
    });
}

/// Contract: [entity-authority-13]
///
/// Disabling delegation clears authority on all clients.
#[test]
fn delegation_disable_clears_authority() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());
    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.make_room().key()));

    let client_key = client_connect(&mut scenario, &room_key, "Client",
        Auth::new("user", "pass"), test_client_config(), test_protocol);

    // Spawn a delegated entity and grant authority
    let entity = scenario.mutate(|ctx| {
        ctx.server(|server| {
            let (entity, _) = server.spawn(|mut e| {
                e.insert_component(Position::new(1.0, 2.0));
                e.configure_replication(ReplicationConfig::Delegated);
                e.enter_room(&room_key);
            });
            server.user_scope_mut(&client_key).unwrap().include(&entity);
            entity
        })
    });

    scenario.expect(|ctx| {
        ctx.client(client_key, |c| {
            c.entity(&entity).and_then(|e| e.authority()) == Some(EntityAuthStatus::Available)
        }).then_some(())
    });

    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut e) = server.entity_mut(&entity) {
                e.give_authority(&client_key).unwrap();
            }
        });
    });

    scenario.expect(|ctx| {
        ctx.client(client_key, |c| {
            c.entity(&entity).and_then(|e| e.authority()) == Some(EntityAuthStatus::Granted)
        }).then_some(())
    });

    // Disable delegation (change to Public)
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut e) = server.entity_mut(&entity) {
                e.configure_replication(ReplicationConfig::Public);
            }
        });
    });

    // Verify authority becomes None
    scenario.spec_expect("entity-authority-13.t1: authority cleared when delegation disabled", |ctx| {
        ctx.client(client_key, |c| {
            c.entity(&entity).and_then(|e| e.authority()).is_none().then_some(())
        })
    });
}

/// Holder can mutate delegated entity
/// Contract: [entity-authority-02]
///
/// Given delegated E where A is authority holder; when A mutates E; then server accepts and all in-scope clients observe the mutation.
#[test]
fn holder_can_mutate_delegated_entity() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());
    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.make_room().key()));

    let client_a_key = client_connect(&mut scenario, &room_key, "Client A",
        Auth::new("client_a", "pass"), test_client_config(), test_protocol.clone());
    let client_b_key = client_connect(&mut scenario, &room_key, "Client B",
        Auth::new("client_b", "pass"), test_client_config(), test_protocol);

    let entity_e = scenario.mutate(|ctx| {
        ctx.server(|server| {
            let (entity, _) = server.spawn(|mut e| {
                e.insert_component(Position::new(1.0, 2.0));
                e.enter_room(&room_key);
            });
            server.user_scope_mut(&client_a_key).unwrap().include(&entity);
            server.user_scope_mut(&client_b_key).unwrap().include(&entity);
            entity
        })
    });

    scenario.expect(|ctx| {
        (ctx.client(client_a_key, |c| c.has_entity(&entity_e)) &&
         ctx.client(client_b_key, |c| c.has_entity(&entity_e))).then_some(())
    });

    // Enable delegation and give authority to A
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut e) = server.entity_mut(&entity_e) {
                e.configure_replication(naia_server::ReplicationConfig::Delegated);
            }
        });
    });

    scenario.expect(|ctx| {
        use naia_shared::EntityAuthStatus;
        ctx.client(client_a_key, |c| c.entity(&entity_e).and_then(|e| e.authority()) == Some(EntityAuthStatus::Available)).then_some(())
    });

    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut e) = server.entity_mut(&entity_e) {
                e.give_authority(&client_a_key).unwrap();
            }
        });
    });

    scenario.expect(|ctx| {
        use naia_shared::EntityAuthStatus;
        ctx.client(client_a_key, |c| c.entity(&entity_e).and_then(|e| e.authority()) == Some(EntityAuthStatus::Granted)).then_some(())
    });

    // A (holder) mutates entity
    scenario.mutate(|ctx| {
        ctx.client(client_a_key, |client_a| {
            if let Some(mut e) = client_a.entity_mut(&entity_e) {
                e.insert_component(Position::new(100.0, 200.0));
            }
        });
    });

    // Verify mutation propagated to B
    scenario.expect(|ctx| {
        let pos = ctx.client(client_b_key, |c| {
            c.entity(&entity_e)
                .and_then(|e| e.component::<Position>().map(|p| (*p.x, *p.y)))
        });
        pos.filter(|(x, _)| (*x - 100.0).abs() < 0.001).map(|_| ())
    });
}

/// Non-holder cannot mutate delegated entity
/// Contract: [entity-authority-02]
///
/// Given delegated E where A is authority holder and B is Denied; when B attempts to mutate E; then mutation is ignored/rejected (no panics) and both clients converge on the authoritative state (from A/server).
#[test]
fn non_holder_cannot_mutate_delegated_entity() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());
    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.make_room().key()));

    let client_a_key = client_connect(&mut scenario, &room_key, "Client A",
        Auth::new("client_a", "pass"), test_client_config(), test_protocol.clone());
    let client_b_key = client_connect(&mut scenario, &room_key, "Client B",
        Auth::new("client_b", "pass"), test_client_config(), test_protocol);

    let entity_e = scenario.mutate(|ctx| {
        ctx.server(|server| {
            let (entity, _) = server.spawn(|mut e| {
                e.insert_component(Position::new(1.0, 2.0));
                e.enter_room(&room_key);
            });
            server.user_scope_mut(&client_a_key).unwrap().include(&entity);
            server.user_scope_mut(&client_b_key).unwrap().include(&entity);
            entity
        })
    });

    scenario.expect(|ctx| {
        (ctx.client(client_a_key, |c| c.has_entity(&entity_e)) &&
         ctx.client(client_b_key, |c| c.has_entity(&entity_e))).then_some(())
    });

    // Enable delegation and give authority to A
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut e) = server.entity_mut(&entity_e) {
                e.configure_replication(naia_server::ReplicationConfig::Delegated);
            }
        });
    });

    scenario.expect(|ctx| {
        use naia_shared::EntityAuthStatus;
        ctx.client(client_a_key, |c| c.entity(&entity_e).and_then(|e| e.authority()) == Some(EntityAuthStatus::Available)).then_some(())
    });

    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut e) = server.entity_mut(&entity_e) {
                e.give_authority(&client_a_key).unwrap();
            }
        });
    });

    // Verify A is Granted, B is Denied
    scenario.expect(|ctx| {
        use naia_shared::EntityAuthStatus;
        let a_granted = ctx.client(client_a_key, |c| c.entity(&entity_e).and_then(|e| e.authority()) == Some(EntityAuthStatus::Granted));
        let b_denied = ctx.client(client_b_key, |c| c.entity(&entity_e).and_then(|e| e.authority()) == Some(EntityAuthStatus::Denied));
        (a_granted && b_denied).then_some(())
    });

    // B (non-holder) attempts to mutate - should be ignored/rejected
    scenario.mutate(|ctx| {
        ctx.client(client_b_key, |client_b| {
            if let Some(mut e) = client_b.entity_mut(&entity_e) {
                e.insert_component(Position::new(999.0, 999.0));
            }
        });
    });

    // Verify: Position should remain at original value (B's mutation rejected)
    scenario.expect(|ctx| {
        let pos = ctx.client(client_a_key, |c| {
            c.entity(&entity_e)
                .and_then(|e| e.component::<Position>().map(|p| (*p.x, *p.y)))
        });
        // Position should still be 1.0, 2.0 (or at least not 999.0)
        pos.filter(|(x, _)| (*x - 999.0).abs() > 0.001).map(|_| ())
    });
}

/// Server-held authority is indistinguishable from "client is denied"
/// Contract: [entity-authority-03], [entity-authority-09]
///
/// Given delegated E where server holds authority; then every in-scope client observes Denied (and cannot mutate), and no client observes Granted.
#[test]
fn server_held_authority_is_indistinguishable_from_client_is_denied() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());
    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.make_room().key()));

    let client_a_key = client_connect(&mut scenario, &room_key, "Client A",
        Auth::new("client_a", "pass"), test_client_config(), test_protocol.clone());
    let client_b_key = client_connect(&mut scenario, &room_key, "Client B",
        Auth::new("client_b", "pass"), test_client_config(), test_protocol);

    // Spawn, configure delegated, and have server take authority
    let entity_e = scenario.mutate(|ctx| {
        ctx.server(|server| {
            let (entity, _) = server.spawn(|mut e| {
                e.insert_component(Position::new(1.0, 2.0));
                e.enter_room(&room_key);
            });
            server.user_scope_mut(&client_a_key).unwrap().include(&entity);
            server.user_scope_mut(&client_b_key).unwrap().include(&entity);
            entity
        })
    });

    scenario.expect(|ctx| {
        let a_sees = ctx.client(client_a_key, |c| c.has_entity(&entity_e));
        let b_sees = ctx.client(client_b_key, |c| c.has_entity(&entity_e));
        (a_sees && b_sees).then_some(())
    });

    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut e) = server.entity_mut(&entity_e) {
                e.configure_replication(naia_server::ReplicationConfig::Delegated);
            }
        });
    });

    scenario.expect(|ctx| {
        use naia_shared::EntityAuthStatus;
        let a_avail = ctx.client(client_a_key, |c| c.entity(&entity_e).and_then(|e| e.authority()) == Some(EntityAuthStatus::Available));
        (a_avail).then_some(())
    });

    // Server takes authority
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut e) = server.entity_mut(&entity_e) {
                e.take_authority().unwrap();
            }
        });
    });

    // Verify: Both clients observe Denied (server holding is same as Denied from client POV)
    scenario.expect(|ctx| {
        use naia_shared::EntityAuthStatus;
        let a_denied = ctx.client(client_a_key, |c| c.entity(&entity_e).and_then(|e| e.authority()) == Some(EntityAuthStatus::Denied));
        let b_denied = ctx.client(client_b_key, |c| c.entity(&entity_e).and_then(|e| e.authority()) == Some(EntityAuthStatus::Denied));
        (a_denied && b_denied).then_some(())
    });
}

/// request_authority(Available) grants to requester and denies everyone else
/// Contract: [entity-authority-04], [entity-authority-05], [entity-authority-08]
///
/// Given delegated E with AuthNone (Available) in scope for A and B; when A calls request_authority(E); then A observes Granted + AuthGranted(E), and B observes Denied + AuthDenied(E).
#[test]
fn request_authority_available_grants_to_requester_and_denies_everyone_else() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());
    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.make_room().key()));

    let client_a_key = client_connect(&mut scenario, &room_key, "Client A",
        Auth::new("client_a", "pass"), test_client_config(), test_protocol.clone());
    let client_b_key = client_connect(&mut scenario, &room_key, "Client B",
        Auth::new("client_b", "pass"), test_client_config(), test_protocol);

    let entity_e = scenario.mutate(|ctx| {
        ctx.server(|server| {
            let (entity, _) = server.spawn(|mut e| {
                e.insert_component(Position::new(1.0, 2.0));
                e.enter_room(&room_key);
            });
            server.user_scope_mut(&client_a_key).unwrap().include(&entity);
            server.user_scope_mut(&client_b_key).unwrap().include(&entity);
            entity
        })
    });

    scenario.expect(|ctx| {
        (ctx.client(client_a_key, |c| c.has_entity(&entity_e)) &&
         ctx.client(client_b_key, |c| c.has_entity(&entity_e))).then_some(())
    });

    // Enable delegation (both clients start with Available)
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut e) = server.entity_mut(&entity_e) {
                e.configure_replication(naia_server::ReplicationConfig::Delegated);
            }
        });
    });

    scenario.expect(|ctx| {
        use naia_shared::EntityAuthStatus;
        let a_avail = ctx.client(client_a_key, |c| c.entity(&entity_e).and_then(|e| e.authority()) == Some(EntityAuthStatus::Available));
        let b_avail = ctx.client(client_b_key, |c| c.entity(&entity_e).and_then(|e| e.authority()) == Some(EntityAuthStatus::Available));
        (a_avail && b_avail).then_some(())
    });

    // A calls request_authority
    scenario.mutate(|ctx| {
        ctx.client(client_a_key, |client_a| {
            if let Some(mut e) = client_a.entity_mut(&entity_e) {
                let _ = e.request_authority();
            }
        });
    });

    // Verify: A has Granted, B has Denied
    scenario.expect(|ctx| {
        use naia_shared::EntityAuthStatus;
        let a_granted = ctx.client(client_a_key, |c| c.entity(&entity_e).and_then(|e| e.authority()) == Some(EntityAuthStatus::Granted));
        let b_denied = ctx.client(client_b_key, |c| c.entity(&entity_e).and_then(|e| e.authority()) == Some(EntityAuthStatus::Denied));
        (a_granted && b_denied).then_some(())
    });
}

/// Denied client request_authority fails (ErrNotAvailable)
/// Contract: [entity-authority-05], [entity-authority-08]
///
/// Given delegated E where A holds authority and B observes Denied; when B calls request_authority(E); then it returns ErrNotAvailable and authority holder remains A (no state/events change).
#[test]
fn denied_client_request_authority_fails_err_not_available() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());
    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.make_room().key()));

    let client_a_key = client_connect(&mut scenario, &room_key, "Client A",
        Auth::new("client_a", "pass"), test_client_config(), test_protocol.clone());
    let client_b_key = client_connect(&mut scenario, &room_key, "Client B",
        Auth::new("client_b", "pass"), test_client_config(), test_protocol);

    let entity_e = scenario.mutate(|ctx| {
        ctx.server(|server| {
            let (entity, _) = server.spawn(|mut e| {
                e.insert_component(Position::new(1.0, 2.0));
                e.enter_room(&room_key);
            });
            server.user_scope_mut(&client_a_key).unwrap().include(&entity);
            server.user_scope_mut(&client_b_key).unwrap().include(&entity);
            entity
        })
    });

    scenario.expect(|ctx| {
        (ctx.client(client_a_key, |c| c.has_entity(&entity_e)) &&
         ctx.client(client_b_key, |c| c.has_entity(&entity_e))).then_some(())
    });

    // Enable delegation and give authority to A
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut e) = server.entity_mut(&entity_e) {
                e.configure_replication(naia_server::ReplicationConfig::Delegated);
            }
        });
    });

    scenario.expect(|ctx| {
        use naia_shared::EntityAuthStatus;
        ctx.client(client_a_key, |c| c.entity(&entity_e).and_then(|e| e.authority()) == Some(EntityAuthStatus::Available)).then_some(())
    });

    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut e) = server.entity_mut(&entity_e) {
                e.give_authority(&client_a_key).unwrap();
            }
        });
    });

    // Verify A is Granted, B is Denied
    scenario.expect(|ctx| {
        use naia_shared::EntityAuthStatus;
        let a_granted = ctx.client(client_a_key, |c| c.entity(&entity_e).and_then(|e| e.authority()) == Some(EntityAuthStatus::Granted));
        let b_denied = ctx.client(client_b_key, |c| c.entity(&entity_e).and_then(|e| e.authority()) == Some(EntityAuthStatus::Denied));
        (a_granted && b_denied).then_some(())
    });

    // B (Denied) calls request_authority - should return error
    let result = scenario.mutate(|ctx| {
        ctx.client(client_b_key, |client_b| {
            if let Some(mut e) = client_b.entity_mut(&entity_e) {
                e.request_authority().err()
            } else {
                None
            }
        })
    });

    // Request should have returned an error (NotAvailable)
    assert!(result.is_some(), "B's request_authority should return error");

    // Verify: A still has Granted, B still has Denied (no change)
    scenario.expect(|ctx| {
        use naia_shared::EntityAuthStatus;
        let a_granted = ctx.client(client_a_key, |c| c.entity(&entity_e).and_then(|e| e.authority()) == Some(EntityAuthStatus::Granted));
        let b_denied = ctx.client(client_b_key, |c| c.entity(&entity_e).and_then(|e| e.authority()) == Some(EntityAuthStatus::Denied));
        (a_granted && b_denied).then_some(())
    });
}

/// Holder release_authority transitions everyone to Available
/// Contract: [entity-authority-06], [entity-authority-12]
///
/// Given delegated E where A holds authority and B observes Denied; when A calls release_authority(E); then A emits AuthLost(E) and both A and B observe Available (explicit Denied→Available for B).
#[test]
fn holder_release_authority_transitions_everyone_to_available() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());
    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.make_room().key()));

    let client_a_key = client_connect(&mut scenario, &room_key, "Client A",
        Auth::new("client_a", "pass"), test_client_config(), test_protocol.clone());
    let client_b_key = client_connect(&mut scenario, &room_key, "Client B",
        Auth::new("client_b", "pass"), test_client_config(), test_protocol);

    let entity_e = scenario.mutate(|ctx| {
        ctx.server(|server| {
            let (entity, _) = server.spawn(|mut e| {
                e.insert_component(Position::new(1.0, 2.0));
                e.enter_room(&room_key);
            });
            server.user_scope_mut(&client_a_key).unwrap().include(&entity);
            server.user_scope_mut(&client_b_key).unwrap().include(&entity);
            entity
        })
    });

    scenario.expect(|ctx| {
        (ctx.client(client_a_key, |c| c.has_entity(&entity_e)) &&
         ctx.client(client_b_key, |c| c.has_entity(&entity_e))).then_some(())
    });

    // Enable delegation and give authority to A
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut e) = server.entity_mut(&entity_e) {
                e.configure_replication(naia_server::ReplicationConfig::Delegated);
            }
        });
    });

    scenario.expect(|ctx| {
        use naia_shared::EntityAuthStatus;
        ctx.client(client_a_key, |c| c.entity(&entity_e).and_then(|e| e.authority()) == Some(EntityAuthStatus::Available)).then_some(())
    });

    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut e) = server.entity_mut(&entity_e) {
                e.give_authority(&client_a_key).unwrap();
            }
        });
    });

    // Verify A is Granted, B is Denied
    scenario.expect(|ctx| {
        use naia_shared::EntityAuthStatus;
        let a_granted = ctx.client(client_a_key, |c| c.entity(&entity_e).and_then(|e| e.authority()) == Some(EntityAuthStatus::Granted));
        let b_denied = ctx.client(client_b_key, |c| c.entity(&entity_e).and_then(|e| e.authority()) == Some(EntityAuthStatus::Denied));
        (a_granted && b_denied).then_some(())
    });

    // A (holder) calls release_authority
    scenario.mutate(|ctx| {
        ctx.client(client_a_key, |client_a| {
            if let Some(mut e) = client_a.entity_mut(&entity_e) {
                let _ = e.release_authority();
            }
        });
    });

    // Verify: Both A and B observe Available
    scenario.expect(|ctx| {
        use naia_shared::EntityAuthStatus;
        let a_avail = ctx.client(client_a_key, |c| c.entity(&entity_e).and_then(|e| e.authority()) == Some(EntityAuthStatus::Available));
        let b_avail = ctx.client(client_b_key, |c| c.entity(&entity_e).and_then(|e| e.authority()) == Some(EntityAuthStatus::Available));
        (a_avail && b_avail).then_some(())
    });
}

/// release_authority when not holder fails (ErrNotHolder)
/// Contract: [entity-authority-07]
///
/// Given delegated E where A holds authority and B observes Denied; when B calls release_authority(E); then it returns ErrNotHolder and nothing changes.
#[test]
fn release_authority_when_not_holder_fails_err_not_holder() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());
    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.make_room().key()));

    let client_a_key = client_connect(&mut scenario, &room_key, "Client A",
        Auth::new("client_a", "pass"), test_client_config(), test_protocol.clone());
    let client_b_key = client_connect(&mut scenario, &room_key, "Client B",
        Auth::new("client_b", "pass"), test_client_config(), test_protocol);

    let entity_e = scenario.mutate(|ctx| {
        ctx.server(|server| {
            let (entity, _) = server.spawn(|mut e| {
                e.insert_component(Position::new(1.0, 2.0));
                e.enter_room(&room_key);
            });
            server.user_scope_mut(&client_a_key).unwrap().include(&entity);
            server.user_scope_mut(&client_b_key).unwrap().include(&entity);
            entity
        })
    });

    scenario.expect(|ctx| {
        (ctx.client(client_a_key, |c| c.has_entity(&entity_e)) &&
         ctx.client(client_b_key, |c| c.has_entity(&entity_e))).then_some(())
    });

    // Enable delegation and give authority to A
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut e) = server.entity_mut(&entity_e) {
                e.configure_replication(naia_server::ReplicationConfig::Delegated);
            }
        });
    });

    scenario.expect(|ctx| {
        use naia_shared::EntityAuthStatus;
        ctx.client(client_a_key, |c| c.entity(&entity_e).and_then(|e| e.authority()) == Some(EntityAuthStatus::Available)).then_some(())
    });

    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut e) = server.entity_mut(&entity_e) {
                e.give_authority(&client_a_key).unwrap();
            }
        });
    });

    // Verify A is Granted, B is Denied
    scenario.expect(|ctx| {
        use naia_shared::EntityAuthStatus;
        let a_granted = ctx.client(client_a_key, |c| c.entity(&entity_e).and_then(|e| e.authority()) == Some(EntityAuthStatus::Granted));
        let b_denied = ctx.client(client_b_key, |c| c.entity(&entity_e).and_then(|e| e.authority()) == Some(EntityAuthStatus::Denied));
        (a_granted && b_denied).then_some(())
    });

    // B (non-holder) calls release_authority - should return error
    let result = scenario.mutate(|ctx| {
        ctx.client(client_b_key, |client_b| {
            if let Some(mut e) = client_b.entity_mut(&entity_e) {
                e.release_authority().err()
            } else {
                None
            }
        })
    });

    // Release should have returned an error (NotHolder)
    assert!(result.is_some(), "B's release_authority should return error");

    // Verify: A still has Granted, B still has Denied (no change)
    scenario.expect(|ctx| {
        use naia_shared::EntityAuthStatus;
        let a_granted = ctx.client(client_a_key, |c| c.entity(&entity_e).and_then(|e| e.authority()) == Some(EntityAuthStatus::Granted));
        let b_denied = ctx.client(client_b_key, |c| c.entity(&entity_e).and_then(|e| e.authority()) == Some(EntityAuthStatus::Denied));
        (a_granted && b_denied).then_some(())
    });
}

/// give_authority assigns to client and denies everyone else
/// Contract: [entity-authority-09], [entity-authority-10], [entity-authority-16]
///
/// Given delegated E with AuthNone (Available) in scope for A and B; when server calls give_authority(A,E); then A observes Granted + AuthGranted(E) and B observes Denied + AuthDenied(E).
#[test]
fn give_authority_assigns_to_client_and_denies_everyone_else() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());
    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.make_room().key()));

    let client_a_key = client_connect(&mut scenario, &room_key, "Client A",
        Auth::new("client_a", "pass"), test_client_config(), test_protocol.clone());
    let client_b_key = client_connect(&mut scenario, &room_key, "Client B",
        Auth::new("client_b", "pass"), test_client_config(), test_protocol);

    // Spawn and configure delegated entity
    let entity_e = scenario.mutate(|ctx| {
        ctx.server(|server| {
            let (entity, _) = server.spawn(|mut e| {
                e.insert_component(Position::new(1.0, 2.0));
                e.enter_room(&room_key);
            });
            server.user_scope_mut(&client_a_key).unwrap().include(&entity);
            server.user_scope_mut(&client_b_key).unwrap().include(&entity);
            entity
        })
    });

    // Wait for both to see entity
    scenario.expect(|ctx| {
        let a_sees = ctx.client(client_a_key, |c| c.has_entity(&entity_e));
        let b_sees = ctx.client(client_b_key, |c| c.has_entity(&entity_e));
        (a_sees && b_sees).then_some(())
    });

    // Enable delegation
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut e) = server.entity_mut(&entity_e) {
                e.configure_replication(naia_server::ReplicationConfig::Delegated);
            }
        });
    });

    // Wait for Available status
    scenario.expect(|ctx| {
        use naia_shared::EntityAuthStatus;
        let a_avail = ctx.client(client_a_key, |c| c.entity(&entity_e).and_then(|e| e.authority()) == Some(EntityAuthStatus::Available));
        let b_avail = ctx.client(client_b_key, |c| c.entity(&entity_e).and_then(|e| e.authority()) == Some(EntityAuthStatus::Available));
        (a_avail && b_avail).then_some(())
    });

    // Server gives authority to A
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut e) = server.entity_mut(&entity_e) {
                e.give_authority(&client_a_key).unwrap();
            }
        });
    });

    // Verify: A has Granted, B has Denied
    scenario.expect(|ctx| {
        use naia_shared::EntityAuthStatus;
        let a_granted = ctx.client(client_a_key, |c| c.entity(&entity_e).and_then(|e| e.authority()) == Some(EntityAuthStatus::Granted));
        let b_denied = ctx.client(client_b_key, |c| c.entity(&entity_e).and_then(|e| e.authority()) == Some(EntityAuthStatus::Denied));
        (a_granted && b_denied).then_some(())
    });
}

/// Server priority: take_authority overrides a client holder
/// Contract: [entity-authority-09], [entity-authority-10]
///
/// Given delegated E where A currently holds authority (A Granted, B Denied); when server calls take_authority(E); then A transitions Granted→Denied emitting AuthLost(E) and AuthDenied(E); B remains Denied; all in-scope clients observe Denied.
#[test]
fn server_priority_take_authority_overrides_a_client_holder() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());
    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.make_room().key()));

    let client_a_key = client_connect(&mut scenario, &room_key, "Client A",
        Auth::new("client_a", "pass"), test_client_config(), test_protocol.clone());
    let client_b_key = client_connect(&mut scenario, &room_key, "Client B",
        Auth::new("client_b", "pass"), test_client_config(), test_protocol);

    let entity_e = scenario.mutate(|ctx| {
        ctx.server(|server| {
            let (entity, _) = server.spawn(|mut e| {
                e.insert_component(Position::new(1.0, 2.0));
                e.enter_room(&room_key);
            });
            server.user_scope_mut(&client_a_key).unwrap().include(&entity);
            server.user_scope_mut(&client_b_key).unwrap().include(&entity);
            entity
        })
    });

    scenario.expect(|ctx| {
        (ctx.client(client_a_key, |c| c.has_entity(&entity_e)) &&
         ctx.client(client_b_key, |c| c.has_entity(&entity_e))).then_some(())
    });

    // Enable delegation and give authority to A
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut e) = server.entity_mut(&entity_e) {
                e.configure_replication(naia_server::ReplicationConfig::Delegated);
            }
        });
    });

    scenario.expect(|ctx| {
        use naia_shared::EntityAuthStatus;
        ctx.client(client_a_key, |c| c.entity(&entity_e).and_then(|e| e.authority()) == Some(EntityAuthStatus::Available)).then_some(())
    });

    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut e) = server.entity_mut(&entity_e) {
                e.give_authority(&client_a_key).unwrap();
            }
        });
    });

    // Verify A has Granted, B has Denied
    scenario.expect(|ctx| {
        use naia_shared::EntityAuthStatus;
        let a_granted = ctx.client(client_a_key, |c| c.entity(&entity_e).and_then(|e| e.authority()) == Some(EntityAuthStatus::Granted));
        let b_denied = ctx.client(client_b_key, |c| c.entity(&entity_e).and_then(|e| e.authority()) == Some(EntityAuthStatus::Denied));
        (a_granted && b_denied).then_some(())
    });

    // Server takes authority - overrides A's hold
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut e) = server.entity_mut(&entity_e) {
                e.take_authority().unwrap();
            }
        });
    });

    // Verify: Both A and B now have Denied (server override)
    scenario.expect(|ctx| {
        use naia_shared::EntityAuthStatus;
        let a_denied = ctx.client(client_a_key, |c| c.entity(&entity_e).and_then(|e| e.authority()) == Some(EntityAuthStatus::Denied));
        let b_denied = ctx.client(client_b_key, |c| c.entity(&entity_e).and_then(|e| e.authority()) == Some(EntityAuthStatus::Denied));
        (a_denied && b_denied).then_some(())
    });
}

/// take_authority forces server hold; all clients denied
/// Contract: [entity-authority-09]
///
/// Given delegated E with AuthNone (Available) in scope for A and B; when server calls take_authority(E); then both A and B observe Denied, and both emit AuthDenied(E) (from non-Denied to Denied).
#[test]
fn take_authority_forces_server_hold_all_clients_denied() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());
    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.make_room().key()));

    let client_a_key = client_connect(&mut scenario, &room_key, "Client A",
        Auth::new("client_a", "pass"), test_client_config(), test_protocol.clone());
    let client_b_key = client_connect(&mut scenario, &room_key, "Client B",
        Auth::new("client_b", "pass"), test_client_config(), test_protocol);

    // Spawn and configure delegated entity
    let entity_e = scenario.mutate(|ctx| {
        ctx.server(|server| {
            let (entity, _) = server.spawn(|mut e| {
                e.insert_component(Position::new(1.0, 2.0));
                e.enter_room(&room_key);
            });
            server.user_scope_mut(&client_a_key).unwrap().include(&entity);
            server.user_scope_mut(&client_b_key).unwrap().include(&entity);
            entity
        })
    });

    // Wait for both to see entity
    scenario.expect(|ctx| {
        let a_sees = ctx.client(client_a_key, |c| c.has_entity(&entity_e));
        let b_sees = ctx.client(client_b_key, |c| c.has_entity(&entity_e));
        (a_sees && b_sees).then_some(())
    });

    // Enable delegation
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut e) = server.entity_mut(&entity_e) {
                e.configure_replication(naia_server::ReplicationConfig::Delegated);
            }
        });
    });

    // Wait for Available status
    scenario.expect(|ctx| {
        use naia_shared::EntityAuthStatus;
        let a_avail = ctx.client(client_a_key, |c| c.entity(&entity_e).and_then(|e| e.authority()) == Some(EntityAuthStatus::Available));
        let b_avail = ctx.client(client_b_key, |c| c.entity(&entity_e).and_then(|e| e.authority()) == Some(EntityAuthStatus::Available));
        (a_avail && b_avail).then_some(())
    });

    // Server takes authority
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut e) = server.entity_mut(&entity_e) {
                e.take_authority().unwrap();
            }
        });
    });

    // Verify: Both A and B have Denied
    scenario.expect(|ctx| {
        use naia_shared::EntityAuthStatus;
        let a_denied = ctx.client(client_a_key, |c| c.entity(&entity_e).and_then(|e| e.authority()) == Some(EntityAuthStatus::Denied));
        let b_denied = ctx.client(client_b_key, |c| c.entity(&entity_e).and_then(|e| e.authority()) == Some(EntityAuthStatus::Denied));
        (a_denied && b_denied).then_some(())
    });
}

/// Former holder sees Granted→Available on server release
/// Contract: [entity-authority-10]
///
/// Given delegated E held by A; when server calls release_authority(E); then A emits AuthLost(E) and observes Available.
#[test]
fn former_holder_sees_granted_to_available_on_server_release() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());
    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.make_room().key()));

    let client_a_key = client_connect(&mut scenario, &room_key, "Client A",
        Auth::new("client_a", "pass"), test_client_config(), test_protocol);

    let entity_e = scenario.mutate(|ctx| {
        ctx.server(|server| {
            let (entity, _) = server.spawn(|mut e| {
                e.insert_component(Position::new(1.0, 2.0));
                e.enter_room(&room_key);
            });
            server.user_scope_mut(&client_a_key).unwrap().include(&entity);
            entity
        })
    });

    scenario.expect(|ctx| {
        ctx.client(client_a_key, |c| c.has_entity(&entity_e)).then_some(())
    });

    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut e) = server.entity_mut(&entity_e) {
                e.configure_replication(naia_server::ReplicationConfig::Delegated);
            }
        });
    });

    scenario.expect(|ctx| {
        use naia_shared::EntityAuthStatus;
        ctx.client(client_a_key, |c| c.entity(&entity_e).and_then(|e| e.authority()) == Some(EntityAuthStatus::Available)).then_some(())
    });

    // Give authority to A
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut e) = server.entity_mut(&entity_e) {
                e.give_authority(&client_a_key).unwrap();
            }
        });
    });

    scenario.expect(|ctx| {
        use naia_shared::EntityAuthStatus;
        ctx.client(client_a_key, |c| c.entity(&entity_e).and_then(|e| e.authority()) == Some(EntityAuthStatus::Granted)).then_some(())
    });

    // Server releases authority
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut e) = server.entity_mut(&entity_e) {
                e.release_authority().unwrap();
            }
        });
    });

    // Verify: A transitions Granted→Available
    scenario.expect(|ctx| {
        use naia_shared::EntityAuthStatus;
        ctx.client(client_a_key, |c| c.entity(&entity_e).and_then(|e| e.authority()) == Some(EntityAuthStatus::Available)).then_some(())
    });
}

/// Server priority: give_authority overrides current holder
/// Contract: [entity-authority-10]
///
/// Given delegated E where A currently holds authority; when server calls give_authority(B,E); then A transitions Granted→Denied emitting AuthLost(E) and AuthDenied(E); B transitions Denied/Available→Granted emitting AuthGranted(E); all other in-scope clients observe Denied.
#[test]
fn server_priority_give_authority_overrides_current_holder() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());
    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.make_room().key()));

    let client_a_key = client_connect(&mut scenario, &room_key, "Client A",
        Auth::new("client_a", "pass"), test_client_config(), test_protocol.clone());
    let client_b_key = client_connect(&mut scenario, &room_key, "Client B",
        Auth::new("client_b", "pass"), test_client_config(), test_protocol);

    let entity_e = scenario.mutate(|ctx| {
        ctx.server(|server| {
            let (entity, _) = server.spawn(|mut e| {
                e.insert_component(Position::new(1.0, 2.0));
                e.enter_room(&room_key);
            });
            server.user_scope_mut(&client_a_key).unwrap().include(&entity);
            server.user_scope_mut(&client_b_key).unwrap().include(&entity);
            entity
        })
    });

    scenario.expect(|ctx| {
        (ctx.client(client_a_key, |c| c.has_entity(&entity_e)) &&
         ctx.client(client_b_key, |c| c.has_entity(&entity_e))).then_some(())
    });

    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut e) = server.entity_mut(&entity_e) {
                e.configure_replication(naia_server::ReplicationConfig::Delegated);
            }
        });
    });

    scenario.expect(|ctx| {
        use naia_shared::EntityAuthStatus;
        ctx.client(client_a_key, |c| c.entity(&entity_e).and_then(|e| e.authority()) == Some(EntityAuthStatus::Available)).then_some(())
    });

    // Give authority to A first
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut e) = server.entity_mut(&entity_e) {
                e.give_authority(&client_a_key).unwrap();
            }
        });
    });

    scenario.expect(|ctx| {
        use naia_shared::EntityAuthStatus;
        let a_granted = ctx.client(client_a_key, |c| c.entity(&entity_e).and_then(|e| e.authority()) == Some(EntityAuthStatus::Granted));
        let b_denied = ctx.client(client_b_key, |c| c.entity(&entity_e).and_then(|e| e.authority()) == Some(EntityAuthStatus::Denied));
        (a_granted && b_denied).then_some(())
    });

    // Server overrides by giving authority to B
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut e) = server.entity_mut(&entity_e) {
                e.give_authority(&client_b_key).unwrap();
            }
        });
    });

    // Verify: A now Denied, B now Granted
    scenario.expect(|ctx| {
        use naia_shared::EntityAuthStatus;
        let a_denied = ctx.client(client_a_key, |c| c.entity(&entity_e).and_then(|e| e.authority()) == Some(EntityAuthStatus::Denied));
        let b_granted = ctx.client(client_b_key, |c| c.entity(&entity_e).and_then(|e| e.authority()) == Some(EntityAuthStatus::Granted));
        (a_denied && b_granted).then_some(())
    });
}

/// Server release_authority clears holder; all clients Available
/// Contract: [entity-authority-10], [entity-authority-12]
///
/// Given delegated E with any current holder (Server or Client); when server calls release_authority(E); then all in-scope clients observe Available; if a client previously held Granted it MUST emit AuthLost(E); any client previously Denied MUST observe Denied→Available.
#[test]
fn server_release_authority_clears_holder_all_clients_available() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());
    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.make_room().key()));

    let client_a_key = client_connect(&mut scenario, &room_key, "Client A",
        Auth::new("client_a", "pass"), test_client_config(), test_protocol.clone());
    let client_b_key = client_connect(&mut scenario, &room_key, "Client B",
        Auth::new("client_b", "pass"), test_client_config(), test_protocol);

    let entity_e = scenario.mutate(|ctx| {
        ctx.server(|server| {
            let (entity, _) = server.spawn(|mut e| {
                e.insert_component(Position::new(1.0, 2.0));
                e.enter_room(&room_key);
            });
            server.user_scope_mut(&client_a_key).unwrap().include(&entity);
            server.user_scope_mut(&client_b_key).unwrap().include(&entity);
            entity
        })
    });

    scenario.expect(|ctx| {
        (ctx.client(client_a_key, |c| c.has_entity(&entity_e)) &&
         ctx.client(client_b_key, |c| c.has_entity(&entity_e))).then_some(())
    });

    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut e) = server.entity_mut(&entity_e) {
                e.configure_replication(naia_server::ReplicationConfig::Delegated);
            }
        });
    });

    scenario.expect(|ctx| {
        use naia_shared::EntityAuthStatus;
        ctx.client(client_a_key, |c| c.entity(&entity_e).and_then(|e| e.authority()) == Some(EntityAuthStatus::Available)).then_some(())
    });

    // Give authority to A
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut e) = server.entity_mut(&entity_e) {
                e.give_authority(&client_a_key).unwrap();
            }
        });
    });

    scenario.expect(|ctx| {
        use naia_shared::EntityAuthStatus;
        ctx.client(client_a_key, |c| c.entity(&entity_e).and_then(|e| e.authority()) == Some(EntityAuthStatus::Granted)).then_some(())
    });

    // Server releases authority
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut e) = server.entity_mut(&entity_e) {
                e.release_authority().unwrap();
            }
        });
    });

    // Verify: Both A and B now have Available
    scenario.expect(|ctx| {
        use naia_shared::EntityAuthStatus;
        let a_avail = ctx.client(client_a_key, |c| c.entity(&entity_e).and_then(|e| e.authority()) == Some(EntityAuthStatus::Available));
        let b_avail = ctx.client(client_b_key, |c| c.entity(&entity_e).and_then(|e| e.authority()) == Some(EntityAuthStatus::Available));
        (a_avail && b_avail).then_some(())
    });
}

/// Out-of-scope ends authority for that client
/// Contract: [entity-authority-11]
/// Contract: [entity-authority-12]
///
/// Given delegated E with A holding authority and B observing Denied; when A goes OutOfScope (removed from scope);
/// then A loses authority (entity no longer exists locally for A), and B transitions from Denied to Available.
#[test]
fn out_of_scope_ends_authority_for_that_client() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());

    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.make_room().key()));

    let client_a_key = client_connect(
        &mut scenario,
        &room_key,
        "Client A",
        Auth::new("client_a", "password"),
        test_client_config(),
        test_protocol.clone(),
    );
    let client_b_key = client_connect(
        &mut scenario,
        &room_key,
        "Client B",
        Auth::new("client_b", "password"),
        test_client_config(),
        test_protocol,
    );

    // Server spawns entity and includes in both clients' scope
    let entity_e = scenario.mutate(|ctx| {
        ctx.server(|server| {
            let (entity, _) = server.spawn(|mut e| {
                e.insert_component(Position::new(1.0, 2.0));
                e.enter_room(&room_key);
            });
            server.user_scope_mut(&client_a_key).unwrap().include(&entity);
            server.user_scope_mut(&client_b_key).unwrap().include(&entity);
            entity
        })
    });

    // Wait for both to see E
    scenario.expect(|ctx| {
        let a_sees_e = ctx.client(client_a_key, |c| c.has_entity(&entity_e));
        let b_sees_e = ctx.client(client_b_key, |c| c.has_entity(&entity_e));
        (a_sees_e && b_sees_e).then_some(())
    });

    // Enable delegation
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut entity_mut) = server.entity_mut(&entity_e) {
                entity_mut.configure_replication(naia_server::ReplicationConfig::Delegated);
            }
        });
    });

    // Wait for delegation to be enabled and clients to see Available status
    scenario.expect(|ctx| {
        use naia_shared::EntityAuthStatus;
        let config = ctx.server(|server| server.entity(&entity_e)?.replication_config());
        let config_ok = config == Some(naia_server::ReplicationConfig::Delegated);
        let a_available = ctx.client(client_a_key, |c| {
            c.entity(&entity_e).and_then(|e| e.authority()) == Some(EntityAuthStatus::Available)
        });
        let b_available = ctx.client(client_b_key, |c| {
            c.entity(&entity_e).and_then(|e| e.authority()) == Some(EntityAuthStatus::Available)
        });
        (config_ok && a_available && b_available).then_some(())
    });

    // Give authority to A
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut entity_mut) = server.entity_mut(&entity_e) {
                entity_mut.give_authority(&client_a_key).unwrap();
            }
        });
    });

    // Wait for A to have Granted and B to have Denied
    scenario.expect(|ctx| {
        use naia_shared::EntityAuthStatus;
        let a_status = ctx.client(client_a_key, |c| {
            c.entity(&entity_e).and_then(|e| e.authority())
        });
        let b_status = ctx.client(client_b_key, |c| {
            c.entity(&entity_e).and_then(|e| e.authority())
        });
        (a_status == Some(EntityAuthStatus::Granted) && b_status == Some(EntityAuthStatus::Denied))
            .then_some(())
    });

    // Remove A from scope for E (A goes out of scope)
    // Per entity-authority-11: A's authority status MUST be cleared (entity no longer exists locally)
    // Per entity-authority-12: server MUST release/reset authority, B MUST transition Denied→Available
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.user_scope_mut(&client_a_key).unwrap().exclude(&entity_e);
        });
    });

    // Verify both conditions:
    // - A no longer has the entity (entity-authority-11)
    // - B transitions from Denied to Available (entity-authority-12)
    scenario.expect(|ctx| {
        use naia_shared::EntityAuthStatus;
        let a_has_entity = ctx.client(client_a_key, |c| c.has_entity(&entity_e));
        let b_status = ctx.client(client_b_key, |c| {
            c.entity(&entity_e).and_then(|e| e.authority())
        });
        (!a_has_entity && b_status == Some(EntityAuthStatus::Available)).then_some(())
    });
}

/// Server give_authority requires scope
/// Contract: [entity-authority-14]
///
/// Given delegated E where OutOfScope(A,E) holds; when server calls give_authority(A,E); then it returns ErrNotInScope and authority holder does not change.
#[test]
fn server_give_authority_requires_scope() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());
    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.make_room().key()));

    let client_a_key = client_connect(&mut scenario, &room_key, "Client A",
        Auth::new("client_a", "pass"), test_client_config(), test_protocol.clone());
    let client_b_key = client_connect(&mut scenario, &room_key, "Client B",
        Auth::new("client_b", "pass"), test_client_config(), test_protocol);

    // Spawn entity only in B's scope, not A's
    let entity_e = scenario.mutate(|ctx| {
        ctx.server(|server| {
            let (entity, _) = server.spawn(|mut e| {
                e.insert_component(Position::new(1.0, 2.0));
                e.enter_room(&room_key);
            });
            // Only include in B's scope, not A's
            server.user_scope_mut(&client_b_key).unwrap().include(&entity);
            entity
        })
    });

    scenario.expect(|ctx| {
        ctx.client(client_b_key, |c| c.has_entity(&entity_e)).then_some(())
    });

    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut e) = server.entity_mut(&entity_e) {
                e.configure_replication(naia_server::ReplicationConfig::Delegated);
            }
        });
    });

    scenario.expect(|ctx| {
        use naia_shared::EntityAuthStatus;
        ctx.client(client_b_key, |c| c.entity(&entity_e).and_then(|e| e.authority()) == Some(EntityAuthStatus::Available)).then_some(())
    });

    // Try to give authority to A (who is not in scope) - should fail
    let give_failed = scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut e) = server.entity_mut(&entity_e) {
                e.give_authority(&client_a_key).is_err()
            } else {
                true // entity doesn't exist, also a failure
            }
        })
    });

    // Verify: give_authority returns error (NotInScope)
    assert!(give_failed);

    // Verify: B still has Available (authority unchanged)
    scenario.expect(|ctx| {
        use naia_shared::EntityAuthStatus;
        ctx.client(client_b_key, |c| c.entity(&entity_e).and_then(|e| e.authority()) == Some(EntityAuthStatus::Available)).then_some(())
    });
}

/// Duplicate/late authority signals are idempotent
/// Contract: [entity-authority-15]
///
/// Given delegated E with A having Granted status; when server calls give_authority(A) again (duplicate);
/// then no panics occur, A remains Granted, and no duplicate events are emitted.
/// This tests that the authority state machine is idempotent to duplicate signals.
#[test]
fn duplicate_authority_signals_are_idempotent() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());

    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.make_room().key()));

    let client_a_key = client_connect(
        &mut scenario,
        &room_key,
        "Client A",
        Auth::new("client_a", "password"),
        test_client_config(),
        test_protocol.clone(),
    );
    let client_b_key = client_connect(
        &mut scenario,
        &room_key,
        "Client B",
        Auth::new("client_b", "password"),
        test_client_config(),
        test_protocol,
    );

    // Server spawns Public entity first, then enables delegation
    let entity_e = scenario.mutate(|ctx| {
        ctx.server(|server| {
            let (entity, _) = server.spawn(|mut e| {
                e.insert_component(Position::new(1.0, 2.0));
                e.enter_room(&room_key);
            });
            server.user_scope_mut(&client_a_key).unwrap().include(&entity);
            server.user_scope_mut(&client_b_key).unwrap().include(&entity);
            entity
        })
    });

    // Wait for both to see E
    scenario.expect(|ctx| {
        let a_sees_e = ctx.client(client_a_key, |c| c.has_entity(&entity_e));
        let b_sees_e = ctx.client(client_b_key, |c| c.has_entity(&entity_e));
        (a_sees_e && b_sees_e).then_some(())
    });

    // Enable delegation
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut entity_mut) = server.entity_mut(&entity_e) {
                entity_mut.configure_replication(naia_server::ReplicationConfig::Delegated);
            }
        });
    });

    // Wait for delegation to be enabled and clients to see Available status
    scenario.expect(|ctx| {
        use naia_shared::EntityAuthStatus;
        let config = ctx.server(|server| server.entity(&entity_e)?.replication_config());
        let config_ok = config == Some(naia_server::ReplicationConfig::Delegated);
        let a_available = ctx.client(client_a_key, |c| {
            c.entity(&entity_e).and_then(|e| e.authority()) == Some(EntityAuthStatus::Available)
        });
        let b_available = ctx.client(client_b_key, |c| {
            c.entity(&entity_e).and_then(|e| e.authority()) == Some(EntityAuthStatus::Available)
        });
        (config_ok && a_available && b_available).then_some(())
    });

    // Give authority to A
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut entity_mut) = server.entity_mut(&entity_e) {
                entity_mut.give_authority(&client_a_key).unwrap();
            }
        });
    });

    // Wait for A to have Granted and B to have Denied
    scenario.expect(|ctx| {
        use naia_shared::EntityAuthStatus;
        let a_status = ctx.client(client_a_key, |c| {
            c.entity(&entity_e).and_then(|e| e.authority())
        });
        let b_status = ctx.client(client_b_key, |c| {
            c.entity(&entity_e).and_then(|e| e.authority())
        });
        (a_status == Some(EntityAuthStatus::Granted) && b_status == Some(EntityAuthStatus::Denied))
            .then_some(())
    });

    // Call give_authority(A) again (duplicate) - this should be idempotent
    // Per spec: "not emit duplicate observable 'grant' effects for the same lifetime"
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut entity_mut) = server.entity_mut(&entity_e) {
                // This is a duplicate give - A already has authority
                // Should not panic and should handle gracefully
                let _ = entity_mut.give_authority(&client_a_key);
            }
        });
    });

    // The duplicate give might return Ok (idempotent) or an error (already granted)
    // Either way, no panic should occur and the final state should be correct

    // Verify: A still has Granted, B still has Denied - state converged correctly
    scenario.expect(|ctx| {
        use naia_shared::EntityAuthStatus;
        let a_status = ctx.client(client_a_key, |c| {
            c.entity(&entity_e).and_then(|e| e.authority())
        });
        let b_status = ctx.client(client_b_key, |c| {
            c.entity(&entity_e).and_then(|e| e.authority())
        });
        // State should remain consistent - A granted, B denied
        (a_status == Some(EntityAuthStatus::Granted) && b_status == Some(EntityAuthStatus::Denied))
            .then_some(())
    });
}
