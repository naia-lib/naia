#![allow(unused_imports)]

use std::time::Duration;

use naia_client::{ClientConfig, JitterBufferType, ReplicationConfig as ClientReplicationConfig};
use naia_server::{ReplicationConfig, RoomKey, ServerConfig};
use naia_shared::{AuthorityError, EntityAuthStatus, Protocol, Request, Response, Tick};

use naia_test::{
    protocol, Auth, ClientConnectEvent, ClientDisconnectEvent, ClientEntityAuthDeniedEvent,
    ClientEntityAuthGrantedEvent, ClientEntityAuthResetEvent, ClientKey, ClientRejectEvent,
    ExpectCtx, Position, Scenario, ServerAuthEvent, ServerConnectEvent, ServerDisconnectEvent,
    ToTicks,
};

// Test protocol types (channels and messages)
use naia_test::test_protocol::{
    OrderedChannel, ReliableChannel, RequestResponseChannel, SequencedChannel,
    TestMessage, TestRequest, TestResponse, TickBufferedChannel, UnorderedChannel,
    UnreliableChannel,
};

mod _helpers;
use _helpers::{client_connect, server_and_client_connected, server_and_client_disconnected, test_client_config};


// ============================================================================
// Entity Delegation Tests
// ============================================================================
// Tests organized by contract ID to match specs/contracts/10_entity_delegation.md
// ============================================================================

/// Cannot delegate client-owned Unpublished (ErrNotPublished)
/// Contract: [entity-delegation-01], [entity-delegation-02]
///
/// Given client-owned Unpublished E owned by A; when server (or A) attempts to delegate E; then operation fails with ErrNotPublished and E remains client-owned Unpublished.
#[test]
fn cannot_delegate_client_owned_unpublished_err_not_published() {
    todo!()
}

/// Client request_authority on non-delegated returns ErrNotDelegated
/// Contract: [entity-delegation-01]
/// Contract: [entity-authority-01]
///
/// Given server-owned undelegated E in scope for A; when A calls request_authority(E); then the call returns ErrNotDelegated and no state/events change.
#[test]
fn client_request_authority_on_non_delegated_returns_err_not_delegated() {
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
        test_protocol,
    );

    // Server spawns E (server-owned, undelegated) and include in A's scope
    let entity_e = scenario.mutate(|ctx| {
        ctx.server(|server| {
            let (entity, _) = server.spawn(|mut e| {
                e.insert_component(Position::new(1.0, 2.0));
                e.enter_room(&room_key);
            });
            server
                .user_scope_mut(&client_a_key)
                .unwrap()
                .include(&entity);
            entity
        })
    });

    // Wait for A to see E
    scenario.expect(|ctx| ctx.client(client_a_key, |c| c.has_entity(&entity_e)).then_some(()));

    // A calls request_authority(E) - should return Err(AuthorityError::NotDelegated)
    let result_err = scenario.mutate(|ctx| {
        ctx.client(client_a_key, |client_a| {
            if let Some(mut entity_mut) = client_a.entity_mut(&entity_e) {
                entity_mut.request_authority().err()
            } else {
                Some(AuthorityError::NotInScope)
            }
        })
    });

    // Assert: result is Err(AuthorityError::NotDelegated)
    assert_eq!(result_err, Some(AuthorityError::NotDelegated));

    // Assert: no state/events change - entity replication config is still Public (not Delegated) and no auth events
    scenario.expect(|ctx| {
        let config = ctx.server(|server| server.entity(&entity_e)?.replication_config());
        let config_ok = config == Some(ReplicationConfig::Public);
        let no_grant = !ctx.client(client_a_key, |c| {
            if c.has::<ClientEntityAuthGrantedEvent>() {
                c.read_event::<ClientEntityAuthGrantedEvent>()
                    .map(|e| e == entity_e)
                    .unwrap_or(false)
            } else {
                false
            }
        });
        (config_ok && no_grant).then_some(())
    });
}

/// Disable delegation clears authority semantics
/// Contract: [entity-delegation-01]
/// Contract: [entity-delegation-02]
/// Contract: [entity-delegation-16]
/// Contract: [entity-delegation-17]
/// Contract: [entity-authority-13]
///
/// Given delegated E in scope for A and B with some current authority holder; when server disables delegation on E; then E becomes server-owned undelegated and clients MUST NOT receive further authority statuses/events for E; subsequent client request_authority returns ErrNotDelegated.
#[test]
fn disable_delegation_clears_authority_semantics() {
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

    // Server spawns E (server-owned, undelegated) and include in both A's and B's scopes
    let entity_e = scenario.mutate(|ctx| {
        ctx.server(|server| {
            let (entity, _) = server.spawn(|mut e| {
                e.insert_component(Position::new(1.0, 2.0));
                e.enter_room(&room_key);
            });
            server
                .user_scope_mut(&client_a_key)
                .unwrap()
                .include(&entity);
            server
                .user_scope_mut(&client_b_key)
                .unwrap()
                .include(&entity);
            entity
        })
    });

    // Wait for both to see E
    scenario.expect(|ctx| {
        let a_sees_e = ctx.client(client_a_key, |c| c.has_entity(&entity_e));
        let b_sees_e = ctx.client(client_b_key, |c| c.has_entity(&entity_e));
        (a_sees_e && b_sees_e).then_some(())
    });

    // Server enables delegation on E
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut entity_mut) = server.entity_mut(&entity_e) {
                entity_mut.configure_replication(ReplicationConfig::Delegated);
            }
        });
    });

    // Wait for delegation to be enabled
    scenario.expect(|ctx| {
        let config = ctx.server(|server| server.entity(&entity_e)?.replication_config());
        (config == Some(ReplicationConfig::Delegated)).then_some(())
    });

    // Server gives authority to A
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
            if let Some(entity) = c.entity(&entity_e) {
                entity.authority()
            } else {
                None
            }
        });
        let b_status = ctx.client(client_b_key, |c| {
            if let Some(entity) = c.entity(&entity_e) {
                entity.authority()
            } else {
                None
            }
        });
        (a_status == Some(EntityAuthStatus::Granted) && b_status == Some(EntityAuthStatus::Denied)).then_some(())
    });

    // Server disables delegation on E
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut entity_mut) = server.entity_mut(&entity_e) {
                entity_mut.configure_replication(ReplicationConfig::Public);
            }
        });
    });

    // Assert: Entity replication config is Public, clients no longer have authority status, subsequent request_authority returns ErrNotDelegated
    scenario.expect(|ctx| {
        let config = ctx.server(|server| server.entity(&entity_e)?.replication_config());
        let config_ok = config == Some(ReplicationConfig::Public);

        let a_no_status = ctx.client(client_a_key, |c| {
            if let Some(entity) = c.entity(&entity_e) {
                entity.authority().is_none()
            } else {
                false
            }
        });
        let b_no_status = ctx.client(client_b_key, |c| {
            if let Some(entity) = c.entity(&entity_e) {
                entity.authority().is_none()
            } else {
                false
            }
        });
        (config_ok && a_no_status && b_no_status).then_some(())
    });

    // Test that subsequent request_authority returns ErrNotDelegated
    let result_err = scenario.mutate(|ctx| {
        ctx.client(client_a_key, |client_a| {
            if let Some(mut entity_mut) = client_a.entity_mut(&entity_e) {
                entity_mut.request_authority().err()
            } else {
                Some(AuthorityError::NotInScope)
            }
        })
    });
    assert_eq!(result_err, Some(AuthorityError::NotDelegated));
}

/// Disable delegation while client holds authority
/// Contract: [entity-delegation-01], [entity-delegation-13], [entity-delegation-16], [entity-delegation-17]
///
/// Given delegated E held by A; when server disables delegation; then A emits AuthLost(E) (since it lost Granted), all clients stop having auth semantics, and client mutations are rejected as non-delegated.
#[test]
fn disable_delegation_while_client_holds_authority() {
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

    // Server spawns E (server-owned, undelegated) and include in both A's and B's scopes
    let entity_e = scenario.mutate(|ctx| {
        ctx.server(|server| {
            let (entity, _) = server.spawn(|mut e| {
                e.insert_component(Position::new(1.0, 2.0));
                e.enter_room(&room_key);
            });
            server
                .user_scope_mut(&client_a_key)
                .unwrap()
                .include(&entity);
            server
                .user_scope_mut(&client_b_key)
                .unwrap()
                .include(&entity);
            entity
        })
    });

    // Wait for both to see E
    scenario.expect(|ctx| {
        let a_sees_e = ctx.client(client_a_key, |c| c.has_entity(&entity_e));
        let b_sees_e = ctx.client(client_b_key, |c| c.has_entity(&entity_e));
        (a_sees_e && b_sees_e).then_some(())
    });

    // Server enables delegation on E
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut entity_mut) = server.entity_mut(&entity_e) {
                entity_mut.configure_replication(ReplicationConfig::Delegated);
            }
        });
    });

    // Wait for delegation to be enabled and clients to see Available status
    scenario.expect(|ctx| {
        use naia_shared::EntityAuthStatus;
        let config = ctx.server(|server| server.entity(&entity_e)?.replication_config());
        let config_ok = config == Some(ReplicationConfig::Delegated);
        let a_available = ctx.client(client_a_key, |c| {
            if let Some(entity) = c.entity(&entity_e) {
                entity.authority() == Some(EntityAuthStatus::Available)
            } else {
                false
            }
        });
        (config_ok && a_available).then_some(())
    });

    // A requests and gets authority
    scenario.mutate(|ctx| {
        ctx.client(client_a_key, |client_a| {
            if let Some(mut entity_mut) = client_a.entity_mut(&entity_e) {
                entity_mut.request_authority().unwrap();
            }
        });
    });

    // Wait for A to have Granted
    scenario.expect(|ctx| {
        use naia_shared::EntityAuthStatus;
        let a_status = ctx.client(client_a_key, |c| {
            if let Some(entity) = c.entity(&entity_e) {
                entity.authority()
            } else {
                None
            }
        });
        (a_status == Some(EntityAuthStatus::Granted)).then_some(())
    });

    // Server disables delegation on E
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut entity_mut) = server.entity_mut(&entity_e) {
                entity_mut.configure_replication(ReplicationConfig::Public);
            }
        });
    });

    // Assert: A receives AuthLost (AuthResetEvent), all clients lose authority status, entity is Public, client mutations are rejected
    scenario.expect(|ctx| {
        let a_received_reset = ctx.client(client_a_key, |c| {
            if c.has::<ClientEntityAuthResetEvent>() {
                c.read_event::<ClientEntityAuthResetEvent>().map(|e| e == entity_e).unwrap_or(false)
            } else {
                false
            }
        });
        let a_no_status = ctx.client(client_a_key, |c| {
            if let Some(entity) = c.entity(&entity_e) {
                entity.authority().is_none()
            } else {
                false
            }
        });
        let b_no_status = ctx.client(client_b_key, |c| {
            if let Some(entity) = c.entity(&entity_e) {
                entity.authority().is_none()
            } else {
                false
            }
        });
        let config = ctx.server(|server| server.entity(&entity_e)?.replication_config());
        let config_ok = config == Some(ReplicationConfig::Public);
        (a_received_reset && a_no_status && b_no_status && config_ok).then_some(())
    });

    // Test that client mutations are rejected (request_authority returns ErrNotDelegated)
    let result_err = scenario.mutate(|ctx| {
        ctx.client(client_a_key, |client_a| {
            if let Some(mut entity_mut) = client_a.entity_mut(&entity_e) {
                entity_mut.request_authority().err()
            } else {
                Some(AuthorityError::NotInScope)
            }
        })
    });
    assert_eq!(result_err, Some(AuthorityError::NotDelegated));
}

/// Enable delegation makes entity Available for all in-scope clients
/// Contract: [entity-delegation-01], [entity-delegation-03], [entity-delegation-17]
///
/// Given server-owned undelegated E in scope for A and B; when server enables delegation on E; then A and B observe E as Available (no holder), and no client has Granted.
#[test]
fn enable_delegation_makes_entity_available_for_all_in_scope_clients() {
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

    // Server spawns E (server-owned, undelegated) and include in both A's and B's scopes
    let entity_e = scenario.mutate(|ctx| {
        ctx.server(|server| {
            let (entity, _) = server.spawn(|mut e| {
                e.insert_component(Position::new(1.0, 2.0));
                e.enter_room(&room_key);
            });
            server
                .user_scope_mut(&client_a_key)
                .unwrap()
                .include(&entity);
            server
                .user_scope_mut(&client_b_key)
                .unwrap()
                .include(&entity);
            entity
        })
    });

    // Wait for both to see E
    scenario.expect(|ctx| {
        let a_sees_e = ctx.client(client_a_key, |c| c.has_entity(&entity_e));
        let b_sees_e = ctx.client(client_b_key, |c| c.has_entity(&entity_e));
        (a_sees_e && b_sees_e).then_some(())
    });

    // Server enables delegation on E
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut entity_mut) = server.entity_mut(&entity_e) {
                entity_mut.configure_replication(ReplicationConfig::Delegated);
            }
        });
    });

    // Assert: A and B observe E as Available, entity replication config is Delegated, no client has Granted
    scenario.expect(|ctx| {
        use naia_shared::EntityAuthStatus;
        let a_status = ctx.client(client_a_key, |c| {
            if let Some(entity) = c.entity(&entity_e) {
                entity.authority()
            } else {
                None
            }
        });
        let b_status = ctx.client(client_b_key, |c| {
            if let Some(entity) = c.entity(&entity_e) {
                entity.authority()
            } else {
                None
            }
        });
        let config = ctx.server(|server| server.entity(&entity_e)?.replication_config());
        
        let a_available = a_status == Some(EntityAuthStatus::Available);
        let b_available = b_status == Some(EntityAuthStatus::Available);
        let config_delegated = config == Some(ReplicationConfig::Delegated);
        let no_granted = a_status != Some(EntityAuthStatus::Granted) && b_status != Some(EntityAuthStatus::Granted);
        
        (a_available && b_available && config_delegated && no_granted).then_some(())
    });
}

/// Server authority APIs on non-delegated return ErrNotDelegated
/// Contract: [entity-delegation-01]
/// Contract: [entity-authority-01]
///
/// Given server-owned undelegated E; when server calls give_authority/take_authority/release_authority for E; then each returns ErrNotDelegated and E remains undelegated.
#[test]
fn server_authority_apis_on_non_delegated_return_err_not_delegated() {
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
        test_protocol,
    );

    // Server spawns E (server-owned, undelegated)
    let entity_e = scenario.mutate(|ctx| {
        ctx.server(|server| {
            let (entity, _) = server.spawn(|mut e| {
                e.insert_component(Position::new(1.0, 2.0));
                e.enter_room(&room_key);
            });
            entity
        })
    });

    // Allow mutate/expect alternation (no-op expect)
    scenario.expect(|_ctx| Some(()));

    // Test give_authority/take_authority/release_authority - all should return ErrNotDelegated
    let (give_result, take_result, release_result) = scenario.mutate(|ctx| {
        ctx.server(|server| {
            let give = if let Some(mut entity_mut) = server.entity_mut(&entity_e) {
                entity_mut.give_authority(&client_a_key).err()
            } else {
                Some(AuthorityError::NotInScope)
            };
            let take = if let Some(mut entity_mut) = server.entity_mut(&entity_e) {
                entity_mut.take_authority().err()
            } else {
                Some(AuthorityError::NotInScope)
            };
            let release = if let Some(mut entity_mut) = server.entity_mut(&entity_e) {
                entity_mut.release_authority().err()
            } else {
                Some(AuthorityError::NotInScope)
            };
            (give, take, release)
        })
    });
    assert_eq!(give_result, Some(AuthorityError::NotDelegated));
    assert_eq!(take_result, Some(AuthorityError::NotDelegated));
    assert_eq!(release_result, Some(AuthorityError::NotDelegated));

    // Assert: E remains undelegated (Public, not Delegated)
    scenario.expect(|ctx| {
        let config = ctx.server(|server| server.entity(&entity_e)?.replication_config());
        (config == Some(ReplicationConfig::Public)).then_some(())
    });
}

/// Server-owned undelegated accepts only server writes
/// Contract: [entity-delegation-01]
///
/// Given server-owned undelegated E in scope for A and B; when A or B attempts to mutate E; then changes are ignored/rejected; when server mutates E; then A and B observe server's updates.
#[test]
fn server_owned_undelegated_accepts_only_server_writes() {
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

    // Server spawns E (server-owned, undelegated) and include in both A's and B's scopes
    let entity_e = scenario.mutate(|ctx| {
        ctx.server(|server| {
            let (entity, _) = server.spawn(|mut e| {
                e.insert_component(Position::new(1.0, 2.0));
                e.enter_room(&room_key);
            });
            server
                .user_scope_mut(&client_a_key)
                .unwrap()
                .include(&entity);
            server
                .user_scope_mut(&client_b_key)
                .unwrap()
                .include(&entity);
            entity
        })
    });

    // Wait for both to see E
    scenario.expect(|ctx| {
        let a_sees_e = ctx.client(client_a_key, |c| c.has_entity(&entity_e));
        let b_sees_e = ctx.client(client_b_key, |c| c.has_entity(&entity_e));
        (a_sees_e && b_sees_e).then_some(())
    });

    // Server updates E
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut entity_mut) = server.entity_mut(&entity_e) {
                entity_mut.insert_component(Position::new(10.0, 20.0));
            }
        });
    });

    // Verify both A and B see server's update
    scenario.expect(|ctx| {
        let a_pos = ctx.client(client_a_key, |c| {
            if let Some(e) = c.entity(&entity_e) {
                e.component::<Position>().map(|p| (*p.x, *p.y))
            } else {
                None
            }
        });
        let b_pos = ctx.client(client_b_key, |c| {
            if let Some(e) = c.entity(&entity_e) {
                e.component::<Position>().map(|p| (*p.x, *p.y))
            } else {
                None
            }
        });

        if let (Some((ax, ay)), Some((bx, by))) = (a_pos, b_pos) {
            let correct = (ax - 10.0).abs() < 0.001 && (ay - 20.0).abs() < 0.001;
            let same = (ax - bx).abs() < 0.001 && (ay - by).abs() < 0.001;
            (correct && same).then_some(())
        } else {
            None
        }
    });

    // A and B attempt to update E (should be ignored)
    scenario.mutate(|ctx| {
        ctx.client(client_a_key, |client_a| {
            if let Some(mut entity_mut) = client_a.entity_mut(&entity_e) {
                entity_mut.insert_component(Position::new(100.0, 200.0));
            }
        });
        ctx.client(client_b_key, |client_b| {
            if let Some(mut entity_mut) = client_b.entity_mut(&entity_e) {
                entity_mut.insert_component(Position::new(300.0, 400.0));
            }
        });
    });

    // Verify entity still has server's value (10, 20), not client updates
    scenario.expect(|ctx| {
        let a_pos = ctx.client(client_a_key, |c| {
            if let Some(e) = c.entity(&entity_e) {
                e.component::<Position>().map(|p| (*p.x, *p.y))
            } else {
                None
            }
        });
        let b_pos = ctx.client(client_b_key, |c| {
            if let Some(e) = c.entity(&entity_e) {
                e.component::<Position>().map(|p| (*p.x, *p.y))
            } else {
                None
            }
        });

        if let (Some((ax, ay)), Some((bx, by))) = (a_pos, b_pos) {
            let correct = (ax - 10.0).abs() < 0.001 && (ay - 20.0).abs() < 0.001;
            let same = (ax - bx).abs() < 0.001 && (ay - by).abs() < 0.001;
            (correct && same).then_some(())
        } else {
            None
        }
    });
}

/// Server-owned undelegated has no authority status and no auth events
/// Contract: [entity-delegation-01], [entity-delegation-17]
///
/// Given server-owned undelegated E in scope for A; then A MUST observe no authority events for E under any circumstance.
#[test]
fn server_owned_undelegated_has_no_authority_status_and_no_auth_events() {
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
        test_protocol,
    );

    // Server spawns E (server-owned, undelegated) and include in A's scope
    let entity_e = scenario.mutate(|ctx| {
        ctx.server(|server| {
            let (entity, _) = server.spawn(|mut e| {
                e.insert_component(Position::new(1.0, 2.0));
                e.enter_room(&room_key);
            });
            server
                .user_scope_mut(&client_a_key)
                .unwrap()
                .include(&entity);
            entity
        })
    });

    // Wait for A to see E, then verify no authority status and no auth events
    scenario.expect(|ctx| {
        let has_entity = ctx.client(client_a_key, |c| c.has_entity(&entity_e));
        if !has_entity {
            return None;
        }
        let no_authority_status = ctx.client(client_a_key, |c| {
            if let Some(entity) = c.entity(&entity_e) {
                entity.authority().is_none()
            } else {
                false
            }
        });
        let no_grant = !ctx.client(client_a_key, |c| {
            if c.has::<ClientEntityAuthGrantedEvent>() {
                c.read_event::<ClientEntityAuthGrantedEvent>()
                    .map(|e| e == entity_e)
                    .unwrap_or(false)
            } else {
                false
            }
        });
        (no_authority_status && no_grant).then_some(())
    });
}

/// Delegating client-owned Published migrates identity without despawn+spawn
/// Contract: [entity-delegation-03], [entity-delegation-04], [entity-delegation-05]
///
/// Given client-owned Published E owned by A and in scope for A and B; when server (or A) delegates E; then E remains the same identity on clients (continuity), and becomes server-owned delegated.
#[test]
fn delegating_client_owned_published_migrates_identity_without_despawn_spawn() {
    todo!()
}

/// Migration assigns initial authority to owner if owner in scope
/// Contract: [entity-delegation-06], [entity-delegation-07]
///
/// Given client-owned Published E owned by A and InScope(A,E); when E is delegated (migrates); then resulting delegated E has holder Client(A): A observes Granted + AuthGranted(E), and every other in-scope client observes Denied + AuthDenied(E).
#[test]
fn migration_assigns_initial_authority_to_owner_if_owner_in_scope() {
    todo!()
}

/// Migration yields no holder if owner out of scope
/// Contract: [entity-delegation-08], [entity-delegation-09]
///
/// Given client-owned Published E owned by A but OutOfScope(A,E) at migration moment; when E is delegated (migrates); then resulting delegated E has AuthNone and every in-scope client observes Available (no initial holder).
#[test]
fn migration_yields_no_holder_if_owner_out_of_scope() {
    todo!()
}

/// No auth events for non-delegated entities, ever
/// Contract: [entity-delegation-10], [entity-delegation-11]
///
/// Given any non-delegated entity (server-owned undelegated or any client-owned); then AuthGranted/AuthDenied/AuthLost MUST NOT be emitted regardless of scope/mutations.
#[test]
fn no_auth_events_for_non_delegated_entities_ever() {
    todo!()
}

/// After migration, writes follow delegated rules
/// Contract: [entity-delegation-12], [entity-delegation-13]
///
/// Given migrated delegated E; when owner A is not the authority holder; then A's mutations are ignored/rejected; when A later acquires authority (Available→Granted) then A's mutations are accepted.
#[test]
fn after_migration_writes_follow_delegated_rules() {
    todo!()
}

/// Duplicate SetAuthority does not emit duplicate events
/// Contract: [entity-delegation-12]
///
/// Given delegated E in a stable status S for client C; when server re-sends SetAuthority(S) (same status); then C emits no additional auth events and status remains S.
#[test]
fn duplicate_set_authority_does_not_emit_duplicate_events() {
    todo!()
}

/// AuthGranted emitted exactly once on Available→Granted
/// Contract: [entity-delegation-14], [entity-delegation-15]
///
/// Given delegated E Available for A; when A becomes holder (via request_authority or give_authority); then exactly one AuthGranted(E) is emitted to A for that transition.
#[test]
fn auth_granted_emitted_exactly_once_on_available_to_granted() {
    todo!()
}

/// AuthDenied emitted exactly once per transition into Denied
/// Contract: [entity-delegation-16]
///
/// Given delegated E where a client C transitions into Denied (from Available or Granted); then exactly one AuthDenied(E) is emitted for that transition.
#[test]
fn auth_denied_emitted_exactly_once_per_transition_into_denied() {
    todo!()
}

/// AuthLost emitted exactly once per transition out of Granted
/// Contract: [entity-delegation-17]
///
/// Given delegated E where client A transitions from Granted to (Denied or Available); then exactly one AuthLost(E) is emitted for that transition.
#[test]
fn auth_lost_emitted_exactly_once_per_transition_out_of_granted() {
    todo!()
}
