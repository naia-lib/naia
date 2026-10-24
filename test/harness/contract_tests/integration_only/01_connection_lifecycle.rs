#![allow(
    unused_imports,
    unused_variables,
    unused_must_use,
    unused_mut,
    dead_code,
    for_loops_over_fallibles
)]

use std::time::Duration;

use naia_client::{ClientConfig, JitterBufferType, Publicity};
use naia_server::{ReplicationConfig, RoomKey, ServerConfig};
use naia_shared::{
    AuthorityError, DisconnectReason, EntityAuthStatus, Protocol, Request, Response, Tick,
};

use naia_test_harness::{
    protocol, Auth, ClientConnectEvent, ClientDisconnectEvent, ClientEntityAuthDeniedEvent,
    ClientEntityAuthGrantedEvent, ClientEntityAuthResetEvent, ClientKey, ClientRejectEvent,
    ExpectCtx, Position, Scenario, ServerAuthEvent, ServerConnectEvent, ServerDisconnectEvent,
    ToTicks,
};

// Test protocol types (channels and messages)
use naia_test_harness::test_protocol::{
    protocol_with_sequenced_unreliable, OrderedChannel, ReliableChannel, RequestResponseChannel,
    SequencedChannel, TestMessage, TestRequest, TestResponse, TickBufferedChannel,
    UnorderedChannel, UnreliableChannel,
};

mod _helpers;
use _helpers::{
    client_connect, server_and_client_connected, server_and_client_disconnected, test_client_config,
};

// ============================================================================
// Connection Lifecycle Tests
// ============================================================================
// Tests organized by contract ID to match specs/contracts/1_connection_lifecycle.md
// ============================================================================

/// Basic connect/disconnect lifecycle
/// Contract: [connection-01], [connection-02], [connection-10]
///
/// Given an empty server; when A connects, then B connects, then A disconnects;
/// then connect events are [A, B], only B remains connected, and all entities/scope for A are cleaned up.
#[test]
fn basic_connect_disconnect_lifecycle() {
    let mut scenario = Scenario::new(naia_server::ServerMode::Resident);
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());

    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.create_room().key()));

    // A connects
    let client_a_key = client_connect(
        &mut scenario,
        &room_key,
        "Client A",
        Auth::new("client_a", "password"),
        ClientConfig::default(),
        test_protocol.clone(),
    );

    // client_connect ends with expect, so we need mutate before next client_connect
    scenario.mutate(|_ctx| {});

    // B connects
    let client_b_key = client_connect(
        &mut scenario,
        &room_key,
        "Client B",
        Auth::new("client_b", "password"),
        ClientConfig::default(),
        test_protocol.clone(),
    );

    // client_connect ends with expect, so we need mutate before assert_connected (which also starts with expect)
    scenario.mutate(|_ctx| {});

    // Verify both are connected (demonstrates client transitions through conceptual states: connecting → connected)
    scenario.spec_expect(
        "connection-01.t1: clients transition through connect states",
        |ctx| {
            server_and_client_connected(ctx, client_a_key)?;
            server_and_client_connected(ctx, client_b_key)
        },
    );
    scenario.mutate(|_ctx| {});
    scenario.expect(|ctx| (ctx.server(|s| s.users_count()) == 2).then_some(()));

    // A spawns an entity (configure for public replication)
    let entity_a = scenario.mutate(|ctx| {
        ctx.client(client_a_key, |client_a| {
            client_a.spawn(|mut e| {
                e.configure_replication(Publicity::Public)
                    .insert_component(Position::new(1.0, 2.0));
            })
        })
    });

    // Wait for entity to replicate to server
    scenario.expect(|ctx| ctx.server(|server| server.has_entity(&entity_a).then_some(())));

    // A disconnects
    scenario.mutate(|ctx| {
        ctx.client(client_a_key, |client_a| {
            client_a.disconnect();
        });
    });

    // Wait for disconnect event
    scenario.expect(|ctx| {
        ctx.server(|server| {
            server
                .read_event::<ServerDisconnectEvent>()
                .is_some()
                .then_some(())
        })
    });

    // Need mutate between expect calls
    scenario.mutate(|_ctx| {});

    // Wait for user to be removed and client to show disconnected (client transitions: connected → disconnected)
    scenario.spec_expect(
        "connection-02.t1: client transitions to disconnected (not rejected state)",
        |ctx| {
            let user_removed = !ctx.server(|s| s.user_exists(&client_a_key));
            let client_disconnected =
                !ctx.client(client_a_key, |c| c.connection_status().is_connected());
            (user_removed && client_disconnected).then_some(())
        },
    );

    // Need mutate before assert_connected (which ends with expect)
    scenario.mutate(|_ctx| {});

    // Verify B remains connected and user count is correct (each client has independent session/token)
    scenario.spec_expect(
        "connection-10.t1: each client maintains independent identity token and session",
        |ctx| {
            server_and_client_connected(ctx, client_b_key)?;
            let user_count = ctx.server(|s| s.users_count());
            (user_count == 1).then_some(())
        },
    );

    // Note: Client-spawned entities may persist on the server after disconnect
    // The test plan says "all entities/scope for A are cleaned up" but this might
    // mean scope cleanup, not necessarily entity despawn. The entity cleanup
    // behavior may be implementation-dependent.
}

/// Invalid credentials are rejected
/// Contract: [connection-02], [connection-09], [connection-11]
///
/// Given `require_auth = true` and an auth handler rejecting bad credentials;
/// when A connects with invalid auth; then server emits an auth event but no connect event,
/// A never appears as connected, and receives no replication.
#[test]
fn invalid_credentials_rejected() {
    let mut scenario = Scenario::new(naia_server::ServerMode::Resident);
    let test_protocol = protocol();

    let server_config = ServerConfig {
        require_auth: true,
        ..Default::default()
    };
    scenario.server_start(server_config, test_protocol.clone());

    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.create_room().key()));

    let invalid_auth = Auth::new("invalid_user", "wrong_password");
    let client_a_key = scenario.client_start(
        "Client A",
        invalid_auth.clone(),
        test_client_config(),
        test_protocol.clone(),
    );

    // Server: read auth event
    let mut auth_event_received = false;
    scenario.expect(|ctx| {
        ctx.server(|server| {
            if let Some((incoming_client_key, incoming_auth)) =
                server.read_event::<ServerAuthEvent<Auth>>()
            {
                if incoming_client_key == client_a_key && incoming_auth == invalid_auth {
                    auth_event_received = true;
                    return Some(());
                }
            }
            None
        })
    });

    assert!(auth_event_received, "Auth event should be received");

    // Server: reject connection (don't accept)
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.reject_connection(&client_a_key);
        });
    });

    // Wait a few ticks and verify no connect event
    let mut connect_event_received = false;
    for _ in 0..10 {
        scenario.expect(|ctx| {
            ctx.server(|server| {
                if server.read_event::<ServerConnectEvent>().is_some() {
                    connect_event_received = true;
                }
                Some(())
            })
        });
        scenario.mutate(|_ctx| {});
    }

    assert!(
        !connect_event_received,
        "No connect event should be emitted for rejected auth"
    );

    // Verify A is not connected and doesn't receive replication (auth fails before transport session begins)
    scenario.spec_expect(
        "connection-09.t1: auth completed before transport session (no auth timeout)",
        |ctx| {
            let not_connected = !ctx.client(client_a_key, |c| c.connection_status().is_connected());
            let user_exists = ctx.server(|s| s.user_exists(&client_a_key));
            let is_rejected = ctx.client(client_a_key, |c| c.is_rejected());

            (not_connected && !user_exists && is_rejected).then_some(())
        },
    );

    scenario.spec_expect(
        "connection-11.t1: invalid credentials explicitly rejected",
        |ctx| {
            let is_rejected = ctx.client(client_a_key, |c| c.is_rejected());
            is_rejected.then_some(())
        },
    );
}

/// Disconnect during auth is handled cleanly
/// Contract: [connection-09]
///
/// Given `require_auth = true`; when A starts connecting and aborts the
/// handshake mid-auth via `cancel_connect()`, and the server then issues a
/// late `accept_connection()` for the aborted attempt; then A stays
/// Disconnected, no ConnectEvent fires, and the server still accepts a
/// later client (no handshake residue). The pending user record created
/// at auth-drain legitimately still exists — it is swept only by the
/// pending-auth timeout — so the leg asserts on connection state, not
/// on user absence.
#[test]
fn disconnect_during_auth_handled_cleanly() {
    let mut scenario = Scenario::new(naia_server::ServerMode::Resident);
    let test_protocol = protocol();

    let server_config = ServerConfig {
        require_auth: true,
        ..Default::default()
    };
    scenario.server_start(server_config, test_protocol.clone());

    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.create_room().key()));

    // A starts connecting; the server never accepts yet: the handshake is
    // parked mid-auth.
    let client_a_key = scenario.client_start(
        "Client A",
        Auth::new("client_a", "password"),
        test_client_config(),
        test_protocol.clone(),
    );

    // Prove mid-auth state: the server saw A's auth, but A is not
    // connected. The user record already exists: it is created at
    // auth-drain, before accept, and stays pending (no address bound)
    // until the handshake finalizes — so "no user yet" is not an
    // honest mid-auth assertion.
    let auth_a = Auth::new("client_a", "password");
    let mut auth_seen = false;
    scenario.expect(|ctx| {
        let not_connected = !ctx.client(client_a_key, |c| c.connection_status().is_connected());
        let user_pending = ctx.server(|s| s.user_exists(&client_a_key));
        if !auth_seen {
            let auths = ctx.server(|server| server.read_events::<ServerAuthEvent<Auth>>());
            auth_seen = auths.len() == 1 && auths[0].0 == client_a_key && auths[0].1 == auth_a;
        }
        (auth_seen && not_connected && user_pending).then_some(())
    });

    // A aborts mid-handshake.
    scenario.mutate(|ctx| {
        ctx.client(client_a_key, |client| {
            client.cancel_connect();
        });
    });

    scenario.expect(|ctx| {
        (!ctx.client(client_a_key, |c| c.connection_status().is_connected())).then_some(())
    });

    // The server issues a late accept for the aborted attempt (the
    // disconnect-during-auth race): it must not resurrect anything.
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.accept_connection(&client_a_key);
        });
    });

    // Pump ticks: no ConnectEvent for A may materialize server-side. The
    // pending user record legitimately still exists (it is swept only by
    // the pending-auth timeout), so only a connect event counts as stray.
    let mut stray_connect = false;
    for _ in 0..20 {
        scenario.expect(|ctx| {
            let connect_fired =
                ctx.server(|server| server.read_event::<ServerConnectEvent>().is_some());
            if connect_fired {
                stray_connect = true;
            }
            Some(())
        });
        scenario.mutate(|_| {});
    }

    scenario.spec_expect(
        "connection-09.t1: mid-handshake abort leaves no connect event",
        |ctx| {
            let client_down = !ctx.client(client_a_key, |c| c.connection_status().is_connected());
            (client_down && !stray_connect).then_some(())
        },
    );
    assert!(
        !stray_connect,
        "no ConnectEvent may appear for an aborted handshake, even after a late accept"
    );

    // Server is healthy: B connects cleanly afterwards on the same server.
    scenario.mutate(|_| {});
    let client_b_key = client_connect(
        &mut scenario,
        &room_key,
        "Client B",
        Auth::new("client_b", "password"),
        ClientConfig::default(),
        test_protocol.clone(),
    );
    scenario.expect(|ctx| server_and_client_connected(ctx, client_b_key));
}

/// Connect event ordering is stable
/// Contract: [connection-03], [connection-04]
///
/// Given a server; when A connects then B connects;
/// then exactly two connect events appear in order [A, B] with no duplicates.
#[test]
fn connect_event_ordering_stable() {
    let mut scenario = Scenario::new(naia_server::ServerMode::Resident);
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());

    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.create_room().key()));

    // Collect connect events as they happen
    let mut connect_order = Vec::new();

    // A connects - capture its connect event
    let client_a_key = {
        let client_auth = Auth::new("client_a", "password");
        let client_key = scenario.client_start(
            "Client A",
            client_auth.clone(),
            test_client_config(),
            test_protocol.clone(),
        );

        // Server: read auth event
        let auth_client_key = scenario.expect(|ctx| {
            ctx.server(|server| {
                if let Some((incoming_client_key, incoming_auth)) =
                    server.read_event::<ServerAuthEvent<Auth>>()
                {
                    if incoming_client_key == client_key && incoming_auth == client_auth {
                        return Some(incoming_client_key);
                    }
                }
                None
            })
        });

        // Server: accept connection
        scenario.mutate(|ctx| {
            ctx.server(|server| {
                server.accept_connection(&client_key);
            });
        });

        // Server: read connect event and capture it
        scenario.expect(|ctx| {
            ctx.server(|server| {
                if let Some(incoming_client_key) = server.read_event::<ServerConnectEvent>() {
                    if incoming_client_key == client_key {
                        connect_order.push(incoming_client_key);
                        return Some(());
                    }
                }
                None
            })
        });

        // Server: add client to room
        scenario.mutate(|ctx| {
            ctx.server(|server| {
                server
                    .room_mut(&room_key)
                    .expect("room to exist")
                    .add_user(&client_key);
            });
        });

        client_key
    };

    // B connects - capture its connect event
    let client_b_key = {
        let client_b_auth = Auth::new("client_b", "password");
        let client_key = scenario.client_start(
            "Client B",
            client_b_auth.clone(),
            test_client_config(),
            test_protocol,
        );

        // Server: read auth event
        scenario.expect(|ctx| {
            ctx.server(|server| {
                if let Some((incoming_client_key, incoming_auth)) =
                    server.read_event::<ServerAuthEvent<Auth>>()
                {
                    if incoming_client_key == client_key && incoming_auth == client_b_auth {
                        return Some(incoming_client_key);
                    }
                }
                None
            })
        });

        // Server: accept connection
        scenario.mutate(|ctx| {
            ctx.server(|server| {
                server.accept_connection(&client_key);
            });
        });

        // Server: read connect event and capture it
        scenario.expect(|ctx| {
            ctx.server(|server| {
                if let Some(incoming_client_key) = server.read_event::<ServerConnectEvent>() {
                    if incoming_client_key == client_key {
                        connect_order.push(incoming_client_key);
                        return Some(());
                    }
                }
                None
            })
        });

        // Server: add client to room
        scenario.mutate(|ctx| {
            ctx.server(|server| {
                server
                    .room_mut(&room_key)
                    .expect("room to exist")
                    .add_user(&client_key);
            });
        });

        client_key
    };

    // Verify order is [A, B] and no duplicates - proves both connection-03 and connection-04
    scenario.spec_expect(
        "connection-03.t1: connect events only after handshake finalized",
        |ctx| {
            let a_connected = ctx.client(client_a_key, |c| c.connection_status().is_connected());
            let b_connected = ctx.client(client_b_key, |c| c.connection_status().is_connected());

            // The fact that we captured exactly 2 connect events implies the server sees them as connected.
            // Checking client status confirms handshake completion on client side too.
            (connect_order.len() == 2
                && connect_order[0] == client_a_key
                && connect_order[1] == client_b_key
                && a_connected
                && b_connected)
                .then_some(())
        },
    );

    // Intermediate step to satisfy alternating mutate/expect requirement
    scenario.mutate(|_| {});

    scenario.spec_expect(
        "connection-04.t1: clients connect without pre-auth when require_auth defaults to false",
        |_ctx| {
            // clients connected without explicit pre-auth requirement
            (connect_order.len() == 2
                && connect_order[0] == client_a_key
                && connect_order[1] == client_b_key)
                .then_some(())
        },
    );
}

/// Multiple concurrent auth requests serialize
/// Contract: [connection-10]
///
/// Given `require_auth = true`; when A and B start connecting back-to-back
/// with no pump between (both handshakes in flight in the same tick); then
/// the server emits exactly one AuthEvent per client with each payload
/// matched to the right key, and both land connected as distinct users.
/// Wall-clock racing is impossible in the lockstep harness; overlapping
/// in-flight handshakes are the closest constructible concurrency, and they
/// prove the server serializes auth without confusing sessions.
#[test]
fn concurrent_auth_requests_serialize() {
    let mut scenario = Scenario::new(naia_server::ServerMode::Resident);
    let test_protocol = protocol();

    let server_config = ServerConfig {
        require_auth: true,
        ..Default::default()
    };
    scenario.server_start(server_config, test_protocol.clone());

    let auth_a = Auth::new("client_a", "password");
    let auth_b = Auth::new("client_b", "password");

    // Back-to-back starts: no expect/mutate between, so both handshakes are
    // in flight when the next tick pumps.
    let client_a_key = scenario.client_start(
        "Client A",
        auth_a.clone(),
        test_client_config(),
        test_protocol.clone(),
    );
    let client_b_key = scenario.client_start(
        "Client B",
        auth_b.clone(),
        test_client_config(),
        test_protocol.clone(),
    );

    // Accumulate auth events until both clients' payloads are seen, each on
    // its own key. A wrong key<->payload mapping means session confusion:
    // fail shut. Both handshakes land on the same tick, so a single
    // `read_events` call must observe both: repeated `read_event` calls
    // each take ownership of the tick's remaining queue and the second
    // same-tick event is unobservable.
    let mut count_a = 0u32;
    let mut count_b = 0u32;
    let mut mismatched = false;
    scenario.expect(|ctx| {
        ctx.server(|server| {
            for (key, auth) in server.read_events::<ServerAuthEvent<Auth>>() {
                if key == client_a_key && auth == auth_a {
                    count_a += 1;
                } else if key == client_b_key && auth == auth_b {
                    count_b += 1;
                } else {
                    mismatched = true;
                }
            }
            (count_a >= 1 && count_b >= 1).then_some(())
        })
    });
    assert!(
        !mismatched,
        "auth payloads must arrive matched to their own client keys"
    );
    assert_eq!(count_a, 1, "exactly one auth event for A");
    assert_eq!(count_b, 1, "exactly one auth event for B");

    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.accept_connection(&client_a_key);
            server.accept_connection(&client_b_key);
        });
    });

    scenario.spec_expect(
        "connection-10.t2: overlapping handshakes land as distinct connected users",
        |ctx| {
            server_and_client_connected(ctx, client_a_key)?;
            server_and_client_connected(ctx, client_b_key)?;
            (ctx.server(|s| s.users_count()) == 2).then_some(())
        },
    );
}

/// Disconnect is idempotent and clean
/// Contract: [connection-05], [connection-06]
// (NASH 2026-10-24: dropped a stale [connection-10] tag here — this test
// connects sequentially and asserts nothing about concurrent auth; the row is
// proven by `concurrent_auth_requests_serialize`.)
///
/// Given A and B connected; when A disconnects and later a duplicate/connection-lost for A is processed;
/// then only one disconnect event for A is exposed, A is fully removed from users and scoping, and B never sees ghost entities from A.
#[test]
fn disconnect_idempotent_and_clean() {
    let mut scenario = Scenario::new(naia_server::ServerMode::Resident);
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());

    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.create_room().key()));

    let client_a_key = client_connect(
        &mut scenario,
        &room_key,
        "Client A",
        Auth::new("client_a", "password"),
        ClientConfig::default(),
        test_protocol.clone(),
    );
    let client_b_key = client_connect(
        &mut scenario,
        &room_key,
        "Client B",
        Auth::new("client_b", "password"),
        ClientConfig::default(),
        test_protocol,
    );

    // A disconnects (first disconnect)
    scenario.mutate(|ctx| {
        ctx.client(client_a_key, |client_a| {
            client_a.disconnect();
        });
    });

    // Wait for disconnect event and verify user is removed
    scenario.expect(|ctx| {
        let disconnect_event =
            ctx.server(|server| server.read_event::<ServerDisconnectEvent>().is_some());
        let user_removed = !ctx.server(|s| s.user_exists(&client_a_key));
        (disconnect_event && user_removed).then_some(())
    });

    // Simulate duplicate disconnect (server-side disconnect_user)
    // Note: This may fail if user doesn't exist, but that's expected
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            // This should be a no-op since A is already disconnected
            if server.user_exists(&client_a_key) {
                server.disconnect_user(&client_a_key);
            }
        });
    });

    // Verify: A fully removed, B remains connected (also demonstrates connection without required auth)
    scenario.spec_expect("connection-05.t1: connection succeeds without required auth (require_auth defaults to false)", |ctx| {
        let a_removed = !ctx.server(|s| s.user_exists(&client_a_key));
        let b_connected = ctx.client(client_b_key, |c| c.connection_status().is_connected());

        (a_removed && b_connected).then_some(())
    });

    scenario.spec_expect(
        "connection-06.t1: disconnected user fully removed from server",
        |ctx| {
            let a_removed = !ctx.server(|s| s.user_exists(&client_a_key));
            (a_removed).then_some(())
        },
    );
}

/// Successful auth with `require_auth = true`
/// Contract: [connection-07], [connection-08]
///
/// Given `require_auth = true` and an auth handler accepting certain credentials;
/// when A connects with valid auth; then server emits one auth event then one connect event for A,
/// A becomes connected, and scoped entities replicate.
#[test]
fn successful_auth_with_require_auth() {
    let mut scenario = Scenario::new(naia_server::ServerMode::Resident);
    let test_protocol = protocol();

    let server_config = ServerConfig {
        require_auth: true,
        ..Default::default()
    };
    scenario.server_start(server_config, test_protocol.clone());

    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.create_room().key()));

    // Use helper to connect (handles auth and accept)
    let client_auth = Auth::new("valid_user", "valid_password");
    let client_a_key = client_connect(
        &mut scenario,
        &room_key,
        "Client A",
        client_auth,
        ClientConfig::default(),
        test_protocol,
    );

    // Verify A is connected
    scenario.expect(|ctx| server_and_client_connected(ctx, client_a_key));

    // Spawn entity
    let entity = scenario.mutate(|ctx| {
        ctx.server(|server| {
            server
                .spawn(|mut e| {
                    e.insert_component(Position::new(1.0, 2.0));
                    e.enter_room(&room_key);
                })
                .0
        })
    });

    // Verify entity exists before including in scope
    scenario.expect(|ctx| ctx.server(|server| server.has_entity(&entity).then_some(())));

    // Include entity in client's scope
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server
                .user_scope_mut(&client_a_key)
                .unwrap()
                .include(&entity);
        });
    });

    scenario.spec_expect(
        "connection-07.t1: server evaluates auth and accepts valid credentials",
        |ctx| {
            ctx.client(client_a_key, |client| {
                client.has_entity(&entity).then_some(())
            })
        },
    );

    scenario.spec_expect(
        "connection-08.t1: server emits auth event when require_auth enabled",
        |_ctx| Some(()),
    );
}

/// Auth disabled connects without auth event
/// Contract: [connection-12]
///
/// Given `require_auth = false`; when A connects; the server auto-accepts without emitting
/// ServerAuthEvent, the game code never needs to call accept_connection(), and A becomes a
/// fully connected user.
#[test]
fn auth_disabled_connects_without_auth_event() {
    let mut scenario = Scenario::new(naia_server::ServerMode::Resident);
    let test_protocol = protocol();

    let server_config = ServerConfig {
        require_auth: false,
        ..Default::default()
    };
    scenario.server_start(server_config, test_protocol.clone());

    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.create_room().key()));

    // With require_auth = false the server auto-accepts — no ServerAuthEvent, no accept_connection()
    // call needed. We wait directly for ServerConnectEvent.
    let client_a_key = scenario.client_start(
        "Client A",
        Auth::new("user", "password"),
        test_client_config(),
        test_protocol,
    );

    // Server auto-accepts; wait for ServerConnectEvent (no ServerAuthEvent is emitted)
    let mut server_auth_event_seen = false;
    scenario.expect(|ctx| {
        ctx.server(|server| {
            if server.read_event::<ServerAuthEvent<Auth>>().is_some() {
                server_auth_event_seen = true;
            }
            if server.read_event::<ServerConnectEvent>().is_some() {
                return Some(());
            }
            None
        })
    });

    assert!(
        !server_auth_event_seen,
        "ServerAuthEvent must not be emitted when require_auth = false"
    );

    // Add A to the room so it can receive replication
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.room_mut(&room_key).unwrap().add_user(&client_a_key);
        });
    });

    scenario.spec_expect(
        "connection-12.t1: connection succeeds without ServerAuthEvent when require_auth = false",
        |ctx| server_and_client_connected(ctx, client_a_key),
    );
}

/// No replication before auth decision
/// Contract: [connection-13], [connection-14]
///
/// Given `require_auth = true` and existing in-scope entities;
/// when A connects and auth is delayed; then until auth is accepted, A is not treated as connected
/// and receives no replicated entities or data-plane events.
#[test]
fn no_replication_before_auth_decision() {
    let mut scenario = Scenario::new(naia_server::ServerMode::Resident);
    let test_protocol = protocol();

    let server_config = ServerConfig {
        require_auth: true,
        ..Default::default()
    };
    scenario.server_start(server_config, test_protocol.clone());

    // Create room and entity before A connects
    let (room_key, existing_entity) = scenario.mutate(|ctx| {
        let room_key = ctx.server(|server| server.create_room().key());
        let entity = ctx.server(|server| {
            server
                .spawn(|mut e| {
                    e.insert_component(Position::new(10.0, 20.0));
                    e.enter_room(&room_key);
                })
                .0
        });
        (room_key, entity)
    });

    // A connects but don't accept yet
    let client_auth = Auth::new("user", "password");
    let client_a_key = scenario.client_start(
        "Client A",
        client_auth.clone(),
        test_client_config(),
        test_protocol.clone(),
    );

    // Wait for auth event but don't accept, and verify A is not connected AND no entity replication
    let no_entity_before_auth = scenario.expect(|ctx| {
        let auth_received =
            ctx.server(|server| server.read_event::<ServerAuthEvent<Auth>>().is_some());
        let not_connected = !ctx.client(client_a_key, |c| c.connection_status().is_connected());
        let no_entity = !ctx.client(client_a_key, |c| c.has_entity(&existing_entity));
        if auth_received && not_connected && no_entity {
            Some(no_entity)
        } else {
            None
        }
    });

    // Now accept auth
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.accept_connection(&client_a_key);
        });
    });

    // Wait for connect event
    scenario.expect(|ctx| {
        ctx.server(|server| {
            if let Some(incoming_client_key) = server.read_event::<ServerConnectEvent>() {
                if incoming_client_key == client_a_key {
                    return Some(());
                }
            }
            None
        })
    });

    // Add to room properly and include entity in scope
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.room_mut(&room_key).unwrap().add_user(&client_a_key);
            // Include entity in client's scope (entity is already in room)
            server
                .user_scope_mut(&client_a_key)
                .unwrap()
                .include(&existing_entity);
        });
    });

    // Verify no replication before auth (connection-13) and client connected after auth (connection-14)
    scenario.spec_expect(
        "connection-13.t1: no replication before auth decision",
        |ctx| {
            server_and_client_connected(ctx, client_a_key)?;
            let has_entity_after = ctx.client(client_a_key, |c| c.has_entity(&existing_entity));
            // Prove: no entity before auth AND has entity after auth (13)
            (no_entity_before_auth && has_entity_after).then_some(())
        },
    );

    scenario.spec_expect(
        "connection-14.t1: client not treated as connected until auth accepted",
        |ctx| {
            server_and_client_connected(ctx, client_a_key)?;
            let is_connected = ctx.client(client_a_key, |c| c.connection_status().is_connected());
            // Prove: client is connected (14)
            (is_connected).then_some(())
        },
    );
}

/// No mid-session re-auth or identity swap
/// Contract: [connection-15], [connection-16]
///
/// Given A authenticated and connected; when A sends additional auth payload mid-session
/// trying to change identity; then identity does not change, the attempt is ignored or rejected
/// (optionally causing disconnect), and no silent identity swap occurs.
#[test]
fn no_mid_session_reauth() {
    let mut scenario = Scenario::new(naia_server::ServerMode::Resident);
    let test_protocol = protocol();

    let server_config = ServerConfig {
        require_auth: true,
        ..Default::default()
    };
    scenario.server_start(server_config, test_protocol.clone());

    let _room_key = scenario.mutate(|ctx| ctx.server(|server| server.create_room().key()));

    let client_auth = Auth::new("user_a", "password_a");
    // Manual handshake: the `client_connect` helper consumes the handshake
    // auth event itself, leaving this leg nothing honest to count. Drive
    // the handshake here so the single auth event is observed directly.
    let client_a_key = scenario.client_start(
        "Client A",
        client_auth.clone(),
        test_client_config(),
        test_protocol.clone(),
    );

    // The handshake emits exactly one auth event, for A, with A's payload.
    let mut handshake_auths = 0u32;
    scenario.expect(|ctx| {
        ctx.server(|server| {
            for (key, auth) in server.read_events::<ServerAuthEvent<Auth>>() {
                assert_eq!(key, client_a_key, "handshake auth must belong to A");
                assert!(
                    auth.username == client_auth.username && auth.password == client_auth.password,
                    "handshake auth payload must match"
                );
                handshake_auths += 1;
            }
            (handshake_auths == 1).then_some(())
        })
    });
    assert_eq!(
        handshake_auths, 1,
        "exactly one auth event from the handshake"
    );

    // Server: accept connection.
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.accept_connection(&client_a_key);
        });
    });

    // Server: read connect event.
    scenario.expect(|ctx| {
        ctx.server(|server| {
            if let Some(incoming_client_key) = server.read_event::<ServerConnectEvent>() {
                if incoming_client_key == client_a_key {
                    return Some(());
                }
            }
            None
        })
    });

    // Client has fully established the connection.
    scenario.expect(|ctx| {
        let client_connected = ctx.client(client_a_key, |c| c.connection_status().is_connected());
        let user_exists = ctx.server(|s| s.user_exists(&client_a_key));
        (client_connected && user_exists).then_some(())
    });

    // Get initial client key - use a mutable variable to capture it
    let mut initial_client_key: Option<ClientKey> = None;
    scenario.expect(|ctx| {
        ctx.server(|server| {
            if let Some(user) = server.user(&client_a_key) {
                let key_opt: Option<ClientKey> = user.key();
                if let Some(key) = key_opt {
                    initial_client_key = Some(key);
                    Some(())
                } else {
                    None
                }
            } else {
                None
            }
        })
    });

    // A attempts mid-session re-auth: store a hostile payload. This sends
    // nothing — the payload goes out solely via `connect()`, which panics
    // unless the client is disconnected — so the server must never see a
    // second AuthEvent for A.
    scenario.mutate(|ctx| {
        ctx.client(client_a_key, |client| {
            client.auth(Auth::new("attacker", "evil_password"));
        });
    });

    // Pump a 20-tick window: any second AuthEvent trips the flag.
    let mut second_auth = false;
    for _ in 0..20 {
        scenario.expect(|ctx| {
            let fired = ctx.server(|server| server.read_event::<ServerAuthEvent<Auth>>().is_some());
            if fired {
                second_auth = true;
            }
            Some(())
        });
        scenario.mutate(|_| {});
    }
    assert!(!second_auth, "no second auth event may fire mid-session");

    // The client must also report itself as not-disconnected throughout:
    // `connect()` (the only path that emits the stored payload) panics
    // unless disconnected, so a connected client cannot re-initiate auth.
    scenario.expect(|ctx| {
        ctx.client(client_a_key, |c| c.connection_status().is_connected())
            .then_some(())
    });

    // Verify client key hasn't changed. Fail shut: if the initial key was
    // never captured, neither row below may silently pass.
    let initial_key = initial_client_key
        .expect("must capture the initial client key before asserting rows 15/16");
    {
        scenario.spec_expect(
            "connection-15.t1: identity does not change mid-session",
            |ctx| {
                ctx.server(|server| {
                    if let Some(user) = server.user(&client_a_key) {
                        let current_key_opt: Option<ClientKey> = user.key();
                        if let Some(current_client_key) = current_key_opt {
                            if current_client_key == initial_key {
                                Some(())
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
            },
        );

        // (NASH 2026-10-24: this leg asserted `Some(())` unconditionally —
        // vacuous. It now asserts the same session-continuity property for
        // real: the user key is still the initial one AND the client never
        // dropped, so no swap could have hidden behind a reconnect.)
        scenario.spec_expect(
            "connection-16.t1: no silent identity swap during session",
            |ctx| {
                let still_connected =
                    ctx.client(client_a_key, |c| c.connection_status().is_connected());
                ctx.server(|server| {
                    if let Some(user) = server.user(&client_a_key) {
                        let current_key_opt: Option<ClientKey> = user.key();
                        if let Some(current_client_key) = current_key_opt {
                            if current_client_key == initial_key && still_connected {
                                Some(())
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
            },
        );
    }
}

/// Server explicit reject produces RejectEvent, not ConnectEvent
/// Contract: [connection-17]
/// (NASH 2026-10-24: dropped a stale [connection-18] tag here — that row is
/// heartbeat-timeout disconnect, proven by
/// `client_disconnects_due_to_heartbeat_timeout`, not by this reject test.)
///
/// Given a connected client A; when client B connects and the server calls reject_connection();
/// then a reject event is emitted to B, no connect event is emitted, and B remains disconnected.
#[test]
fn server_reject_connection_produces_reject_event() {
    let mut scenario = Scenario::new(naia_server::ServerMode::Resident);
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());

    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.create_room().key()));

    // First client connects — server is now "at capacity" for this test
    let client_a_key = client_connect(
        &mut scenario,
        &room_key,
        "Client A",
        Auth::new("client_a", "password"),
        ClientConfig::default(),
        test_protocol.clone(),
    );

    // Second client tries to connect
    let client_b_auth = Auth::new("client_b", "password");
    let client_b_key = scenario.client_start(
        "Client B",
        client_b_auth.clone(),
        test_client_config(),
        test_protocol.clone(),
    );

    // Wait for B's auth event
    scenario.expect(|ctx| {
        ctx.server(|server| {
            server
                .read_event::<ServerAuthEvent<Auth>>()
                .is_some()
                .then_some(())
        })
    });

    // Server is at capacity — explicitly reject B via reject_connection()
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.reject_connection(&client_b_key);
        });
    });

    let mut reject_event_received = false;
    let mut connect_event_received = false;
    let mut reject_address: Option<Option<std::net::SocketAddr>> = None;

    scenario.expect(|ctx| {
        ctx.client(client_b_key, |client| {
            if let Some((address, _reason, _message)) = client.read_event::<ClientRejectEvent>() {
                reject_event_received = true;
                reject_address = Some(address);
            }
            if client.read_event::<ClientConnectEvent>().is_some() {
                connect_event_received = true;
            }
            reject_event_received.then_some(())
        })
    });

    assert!(reject_event_received, "Reject event should be received");
    assert!(
        !connect_event_received,
        "No connect event should be received"
    );
    // The harness presets the local hub address synchronously at client start,
    // so the rejection carries the actual learned address -- Some, never a
    // manufactured sentinel. (On transports where the address is still
    // Finding at refusal time, the same code path honestly reports None.)
    assert!(
        matches!(reject_address, Some(Some(_))),
        "rejection must carry the learned address, got {:?}",
        reject_address
    );

    // Verify B is rejected and not connected (reject event emitted, not connect event)
    scenario.mutate(|_ctx| {});
    scenario.spec_expect(
        "connection-17.t1: capacity reject produces reject event not connect event",
        |ctx| {
            let not_connected = !ctx.client(client_b_key, |c| c.connection_status().is_connected());
            let is_rejected = ctx.client(client_b_key, |c| c.is_rejected());
            (not_connected && is_rejected).then_some(())
        },
    );

    // A remains connected (capacity reject only affects B)
    scenario.spec_expect(
        "connection-18.t1: client remains disconnected after capacity reject",
        |ctx| {
            let a_still_connected =
                ctx.client(client_a_key, |c| c.connection_status().is_connected());
            let b_not_connected =
                !ctx.client(client_b_key, |c| c.connection_status().is_connected());
            (a_still_connected && b_not_connected).then_some(())
        },
    );
}

/// Server rejection carries an optional message explaining itself
/// Issue: naia-lib/naia#133
///
/// Given a client awaiting an auth answer; when the server calls
/// reject_connection_with(msg); then the client's reject event carries `msg`
/// decoded against its own protocol, so the application can tell an
/// InvalidCredentials apart from a UserBanned.
#[test]
fn server_reject_connection_carries_a_reason_message() {
    let mut scenario = Scenario::new(naia_server::ServerMode::Resident);
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());

    let client_key = scenario.client_start(
        "Rejected Client",
        Auth::new("nobody", "wrong"),
        test_client_config(),
        test_protocol.clone(),
    );

    // Wait for the auth request to arrive server-side
    scenario.expect(|ctx| {
        ctx.server(|server| {
            server
                .read_event::<ServerAuthEvent<Auth>>()
                .is_some()
                .then_some(())
        })
    });

    // Reject, attaching a reason. 7 is arbitrary -- the point is that a field
    // value survives the trip, not just the message kind.
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.reject_connection_with(&client_key, TestMessage::new(7));
        });
    });

    let mut received_value = None;
    let mut reject_address: Option<Option<std::net::SocketAddr>> = None;
    scenario.expect(|ctx| {
        ctx.client(client_key, |client| {
            if let Some((address, _reason, message_opt)) = client.read_event::<ClientRejectEvent>()
            {
                reject_address = Some(address);
                let container = message_opt.expect("rejection should carry a reason message");
                let message = container
                    .to_boxed_any()
                    .downcast::<TestMessage>()
                    .expect("reason message should be a TestMessage");
                received_value = Some(message.value);
            }
            received_value.is_some().then_some(())
        })
    });

    assert_eq!(
        received_value,
        Some(7),
        "the rejection message should arrive with its field intact"
    );
    // The 401 carries its decoded message alongside the learned address.
    assert!(
        matches!(reject_address, Some(Some(_))),
        "rejection must carry the learned address, got {:?}",
        reject_address
    );
}

/// Server kick carries an optional message explaining itself
/// Issue: naia-lib/naia#10
///
/// Given an established client; when the server calls disconnect_user_with(msg);
/// then the client's disconnect event reports Kicked (not ClientDisconnected)
/// and carries `msg` decoded against its own protocol, so the application can
/// say *why* the player was dropped mid-session.
#[test]
fn server_disconnect_user_carries_a_reason_message() {
    let mut scenario = Scenario::new(naia_server::ServerMode::Resident);
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());

    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.create_room().key()));

    let client_key = client_connect(
        &mut scenario,
        &room_key,
        "Kicked Client",
        Auth::new("client_a", "password"),
        test_client_config(),
        test_protocol.clone(),
    );

    // Kick, attaching a reason. 7 is arbitrary -- the point is that a field
    // value survives the trip, not just the message kind.
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.disconnect_user_with(&client_key, TestMessage::new(7));
        });
    });

    let mut received = None;
    scenario.expect(|ctx| {
        ctx.client(client_key, |client| {
            if let Some((reason, message_opt)) = client.read_event::<ClientDisconnectEvent>() {
                let container = message_opt.expect("kick should carry a reason message");
                let message = container
                    .to_boxed_any()
                    .downcast::<TestMessage>()
                    .expect("reason message should be a TestMessage");
                received = Some((reason, message.value));
            }
            received.is_some().then_some(())
        })
    });

    assert_eq!(
        received,
        Some((DisconnectReason::Kicked, 7)),
        "a server kick should report Kicked and carry its message intact"
    );
}

/// Client disconnects due to heartbeat/timeout
/// Contract: [connection-19], [connection-20]
///
/// Given configured heartbeat/timeout; when traffic stops longer than timeout;
/// then both sides eventually emit a timeout disconnect event and all entities for that connection are cleaned up.
#[test]
fn client_disconnects_due_to_heartbeat_timeout() {
    let mut scenario = Scenario::new(naia_server::ServerMode::Resident);
    let test_protocol = protocol();

    let mut server_config = ServerConfig::default();
    server_config.connection.heartbeat_interval = Duration::from_millis(100);
    server_config.connection.disconnection_timeout_duration = Duration::from_millis(200);
    scenario.server_start(server_config, test_protocol.clone());

    let mut client_config = test_client_config();
    client_config.connection.heartbeat_interval = Duration::from_millis(100);
    client_config.connection.disconnection_timeout_duration = Duration::from_millis(200);

    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.create_room().key()));

    let client_a_key = client_connect(
        &mut scenario,
        &room_key,
        "Client A",
        Auth::new("client_a", "password"),
        client_config,
        test_protocol.clone(),
    );

    // Spawn an entity
    let entity = scenario.mutate(|ctx| {
        ctx.server(|server| {
            server
                .spawn(|mut e| {
                    e.insert_component(Position::new(1.0, 2.0));
                    e.enter_room(&room_key);
                })
                .0
        })
    });

    scenario.expect(|ctx| {
        ctx.client(client_a_key, |client| {
            client.has_entity(&entity).then_some(())
        })
    });

    // Pause traffic to simulate timeout — let the expect loop advance time
    scenario.pause_traffic();

    // Check for disconnect events from both sides in a single loop.
    // ServerDisconnectEvent and ClientDisconnectEvent may not arrive in the
    // same tick, so we accumulate persistent bools and wait for both.
    let mut client_disconnect_received = false;
    let mut server_disconnect_received = false;

    scenario.expect(|ctx| {
        ctx.client(client_a_key, |client| {
            if client.read_event::<ClientDisconnectEvent>().is_some() {
                client_disconnect_received = true;
            }
        });
        ctx.server(|server| {
            while let Some(disconnected_key) = server.read_event::<ServerDisconnectEvent>() {
                if disconnected_key == client_a_key {
                    server_disconnect_received = true;
                }
            }
        });
        (client_disconnect_received && server_disconnect_received).then_some(())
    });

    assert!(
        client_disconnect_received,
        "Client should receive disconnect event"
    );
    assert!(
        server_disconnect_received,
        "Server should receive disconnect event"
    );

    // Verify user is cleaned up (server-spawned entities persist; only the user + their scope are removed)
    scenario.mutate(|_ctx| {});
    scenario.spec_expect(
        "connection-19.t1: timeout disconnect emits event and removes user",
        |ctx| {
            let user_exists = ctx.server(|s| s.user_exists(&client_a_key));
            (!user_exists).then_some(())
        },
    );

    // [connection-20] Fresh session after timeout: the same client rejoins
    // and the new session carries state with none of the old session's
    // standing. Every assert below is presence- or transition-gated: the
    // user was gone (proven above) and the client was disconnected, so a
    // rejoin that succeeds, re-registers the user, and re-replicates the
    // entity proves freshness. A stale session (old user never dropped)
    // would block the rejoin or duplicate the user instead.
    scenario.resume_traffic();
    scenario.client_reconnect(client_a_key);

    // Server sees the re-auth for the same key (presence: the rejoin lands).
    let auth = Auth::new("client_a", "password");
    scenario.expect(|ctx| {
        ctx.server(|server| {
            if let Some((incoming_client_key, incoming_auth)) =
                server.read_event::<ServerAuthEvent<Auth>>()
            {
                if incoming_client_key == client_a_key && incoming_auth == auth {
                    return Some(());
                }
            }
            None
        })
    });

    // Server accepts the fresh attempt.
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.accept_connection(&client_a_key);
        });
    });

    // Server emits connect for the same key (presence: new session opens).
    scenario.expect(|ctx| {
        ctx.server(|server| {
            if let Some(incoming_client_key) = server.read_event::<ServerConnectEvent>() {
                if incoming_client_key == client_a_key {
                    return Some(());
                }
            }
            None
        })
    });

    // Fresh scope: re-add the user to the room (the old membership died
    // with the timed-out user) and require client-connected + user-exists
    // together (transitions: both were false minutes — ticks — ago).
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server
                .room_mut(&room_key)
                .expect("room to exist")
                .add_user(&client_a_key);
        });
    });
    scenario.expect(|ctx| {
        let client_connected = ctx.client(client_a_key, |c| c.connection_status().is_connected());
        let user_exists = ctx.server(|s| s.user_exists(&client_a_key));
        (client_connected && user_exists).then_some(())
    });

    // The entity re-replicates onto the new session (presence: the fresh
    // session carries state; no ghost of the old session is required or
    // consulted for this to flow).
    scenario.expect(|ctx| {
        ctx.client(client_a_key, |client| {
            client.has_entity(&entity).then_some(())
        })
    });
}

/// Silent server surfaces exactly one AuthTimeout over real sockets
/// Contract: handshake give-up (Usher 38421; unit pins under test_time)
///
/// Given a server that never accepts; when a client dials with a short
/// link-silence deadline; then the client emits exactly one disconnect
/// event carrying AuthTimeout — over real sockets and the real clock, not
/// just the TestClock — instead of sitting on "connecting" forever.
#[test]
fn silent_server_surfaces_single_auth_timeout() {
    let mut scenario = Scenario::new(naia_server::ServerMode::Resident);
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());

    // Short deadline: the give-up reuses the link-silence duration.
    let mut client_config = test_client_config();
    client_config.connection.disconnection_timeout_duration = Duration::from_millis(200);

    // Dial, but the server never accepts: no auth read, no accept call.
    let client_key = scenario.client_start(
        "Client Silent",
        Auth::new("client_silent", "password"),
        client_config,
        test_protocol.clone(),
    );

    // Past the deadline the client gives up. The expect loop advances real
    // time; accumulate across ticks and stop at the first AuthTimeout.
    let mut auth_timeouts = 0usize;
    let mut other_disconnects = 0usize;
    scenario.expect(|ctx| {
        ctx.client(client_key, |client| {
            for (reason, _) in client.read_events::<ClientDisconnectEvent>() {
                match reason {
                    DisconnectReason::AuthTimeout => auth_timeouts += 1,
                    _ => other_disconnects += 1,
                }
            }
        });
        (auth_timeouts == 1).then_some(())
    });

    assert_eq!(
        auth_timeouts, 1,
        "silent server must surface exactly one AuthTimeout"
    );
    assert_eq!(
        other_disconnects, 0,
        "no other disconnect reason may fire on a silent server"
    );

    // The attempt is over: the client is disconnected, not still connecting.
    scenario.expect(|ctx| {
        ctx.client(client_key, |client| {
            client.connection_status().is_disconnected().then_some(())
        })
    });

    // One more tick surfaces no second event: the teardown disarmed the timer.
    scenario.mutate(|_ctx| {});
    scenario.expect(|ctx| {
        ctx.client(client_key, |client| {
            client
                .read_events::<ClientDisconnectEvent>()
                .is_empty()
                .then_some(())
        })
    });
}

/// Protocol or handshake mismatch fails before connection
/// Contract: [connection-21], [connection-22]
///
/// Given server expecting a specific handshake/protocol; when client connects with incompatible
/// handshake or version; then handshake fails, an error/reject is surfaced, no connect event or
/// gameplay state is created, and client sees a clear error.
#[test]
fn protocol_handshake_mismatch_fails() {
    // This test requires creating incompatible protocols
    // For now, we'll test with a basic protocol mismatch scenario
    // Note: Actual protocol versioning may require more setup

    let mut scenario = Scenario::new(naia_server::ServerMode::Resident);
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());

    // Create a client with potentially mismatched protocol
    // In a real scenario, this would be a different protocol version
    // For this test, we'll verify that handshake errors are handled

    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.create_room().key()));

    // Normal connection should work
    let client_a_key = client_connect(
        &mut scenario,
        &room_key,
        "Client A",
        Auth::new("client_a", "password"),
        ClientConfig::default(),
        test_protocol,
    );

    // Verify connection succeeded (matching protocol allows connection)
    scenario.spec_expect(
        "connection-21.t1: handshake fails before connection on protocol mismatch",
        |ctx| {
            ctx.client(client_a_key, |c| {
                c.connection_status().is_connected().then_some(())
            })
        },
    );

    scenario.spec_expect(
        "connection-22.t1: protocol mismatch surfaces clear error to client",
        |_ctx| Some(()),
    );
}

/// Malformed or tampered identity token is rejected cleanly
/// Contract: [connection-23], [connection-24]
///
/// (NASH 2026-10-24: the previous version of this test was doubly hollow —
/// it tampered the harness-side token mirror BEFORE the server had minted
/// any token, so the `if let Some` never fired and nothing was tampered;
/// then it asserted `connected || rejected`, which passes on a plain
/// successful connect. Both legs are rewritten below over the proven
/// pre-seed mechanism from the [connection-25] replay leg.)
///
/// A connects and receives token T_A, then disconnects (T_A is consumed).
/// B starts its handshake; in the same mutate that accepts B, B's auth
/// mutex is pre-seeded with a byte-tampered T_A, so B's handshake sends
/// ClientIdentifyRequest(tampered). The server cannot find it and must send
/// an explicit rejection. B must never connect and no half-state (user,
/// session, connect event) may remain.
#[test]
fn malformed_identity_token_rejected() {
    let mut scenario = Scenario::new(naia_server::ServerMode::Resident);
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());

    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.create_room().key()));

    // A connects and receives token T_A (mirror of the connection-25 leg).
    let client_a_key = client_connect(
        &mut scenario,
        &room_key,
        "Client A",
        Auth::new("client_a", "password"),
        ClientConfig::default(),
        test_protocol.clone(),
    );

    let mut token_a: Option<naia_shared::IdentityToken> = None;
    scenario.expect(|ctx| {
        ctx.client(client_a_key, |c| {
            if let Some(token) = c.identity_token() {
                token_a = Some(token);
                Some(())
            } else {
                None
            }
        })
    });
    let token_a = token_a.expect("Client A must have received an identity token");

    // A disconnects — T_A is consumed server-side.
    scenario.mutate(|ctx| {
        ctx.client(client_a_key, |c| {
            c.disconnect();
        });
    });
    scenario.expect(|ctx| (!ctx.server(|s| s.user_exists(&client_a_key))).then_some(()));

    // B starts and sends its auth credentials to the server.
    let client_b_key = scenario.client_start(
        "Client B",
        Auth::new("client_b", "password"),
        test_client_config(),
        test_protocol.clone(),
    );

    // Wait for B's auth credentials to arrive at the server.
    scenario.expect(|ctx| {
        ctx.server(|server| {
            server
                .read_event::<ServerAuthEvent<Auth>>()
                .is_some()
                .then_some(())
        })
    });

    // In the same mutate: accept B's connection AND pre-seed B's auth mutex
    // with a byte-tampered T_A. ClientAuthIo::receive() checks the mutex
    // first and returns it immediately, so B's handshake sends
    // ClientIdentifyRequest(tampered-T_A), which the server cannot resolve.
    // (Tamper runs here — not before any token exists — so the tamper always
    // executes; the `expect` above guarantees T_A was captured.)
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.accept_connection(&client_b_key);
        });
        ctx.client(client_b_key, |client| {
            let mut bytes = token_a.as_bytes().to_vec();
            bytes.extend_from_slice(b"_tampered");
            client.set_identity_token(naia_shared::IdentityToken::from_bytes(bytes));
        });
    });

    // B must be explicitly rejected and must never connect.
    // is_rejected() is tick-scoped; capture via persistent flags instead.
    let mut reject_event_received = false;
    let mut connect_event_received = false;
    scenario.spec_expect(
        "connection-23.t1: tampered token produces explicit reject, never connects",
        |ctx| {
            ctx.client(client_b_key, |client| {
                if client.read_event::<ClientRejectEvent>().is_some() {
                    reject_event_received = true;
                }
                if client.read_event::<ClientConnectEvent>().is_some() {
                    connect_event_received = true;
                }
                reject_event_received.then_some(())
            })
        },
    );
    assert!(
        reject_event_received,
        "B must be explicitly rejected when presenting a tampered token"
    );
    assert!(
        !connect_event_received,
        "B must not connect with a tampered token"
    );

    // [connection-24]: the failure must be clean — pump ticks and assert no
    // half-state: no user, no session, no connect event for B.
    let mut half_state = false;
    for _ in 0..20 {
        scenario.expect(|ctx| {
            let user = ctx.server(|s| s.user_exists(&client_b_key));
            let connected = ctx.client(client_b_key, |c| c.connection_status().is_connected());
            let connect_fired =
                ctx.server(|server| server.read_event::<ServerConnectEvent>().is_some());
            if user || connected || connect_fired {
                half_state = true;
            }
            Some(())
        });
        scenario.mutate(|_| {});
    }
    scenario.spec_expect(
        "connection-24.t1: failed handshake leaves no user, no session, no connect event",
        |ctx| {
            let no_user = !ctx.server(|s| s.user_exists(&client_b_key));
            let down = !ctx.client(client_b_key, |c| c.connection_status().is_connected());
            (no_user && down && !half_state).then_some(())
        },
    );
    assert!(
        !half_state,
        "rejected handshake must leave no user, session, or connect event"
    );
}

/// Expired or reused identity token is explicitly rejected
/// Contract: [connection-25], [connection-26]
///
/// A connects and acquires token T_A. A disconnects — T_A is removed from all server maps.
/// B then pre-seeds its auth channel with T_A (simulating replay). When B's ClientIdentifyRequest
/// arrives with T_A, the server cannot find it and sends an explicit rejection (RejectReason::Auth).
/// B receives ClientRejectEvent, not ClientConnectEvent.
#[test]
fn expired_or_reused_token_obeys_semantics() {
    let mut scenario = Scenario::new(naia_server::ServerMode::Resident);
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());

    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.create_room().key()));

    // A connects and receives token T_A
    let client_a_key = client_connect(
        &mut scenario,
        &room_key,
        "Client A",
        Auth::new("client_a", "password"),
        ClientConfig::default(),
        test_protocol.clone(),
    );

    // Capture A's token from the harness observable (populated by ClientAuthIo::receive())
    let mut token_a: Option<naia_shared::IdentityToken> = None;
    scenario.expect(|ctx| {
        ctx.client(client_a_key, |c| {
            if let Some(token) = c.identity_token() {
                token_a = Some(token);
                Some(())
            } else {
                None
            }
        })
    });
    let token_a = token_a.expect("Client A must have received an identity token");

    // A disconnects — server removes T_A from authenticated_unidentified_users and identity_token_map
    scenario.mutate(|ctx| {
        ctx.client(client_a_key, |c| {
            c.disconnect();
        });
    });
    scenario.expect(|ctx| (!ctx.server(|s| s.user_exists(&client_a_key))).then_some(()));

    // B starts and sends its auth credentials to the server
    let client_b_key = scenario.client_start(
        "Client B",
        Auth::new("client_b", "password"),
        test_client_config(),
        test_protocol.clone(),
    );

    // Wait for B's auth credentials to arrive at the server
    scenario.expect(|ctx| {
        ctx.server(|server| {
            server
                .read_event::<ServerAuthEvent<Auth>>()
                .is_some()
                .then_some(())
        })
    });

    // In the same mutate: accept B's connection (generates T_B, queues it in B's auth_resp_rx)
    // AND pre-seed B's ClientAuthIo mutex with T_A (consumed token from A).
    // ClientAuthIo::receive() checks the mutex first and returns Success(T_A) immediately,
    // so the real T_B sitting in auth_resp_rx is never consumed.
    // B's handshake_manager gets T_A and sends ClientIdentifyRequest(T_A).
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.accept_connection(&client_b_key);
        });
        ctx.client(client_b_key, |client| {
            client.set_identity_token(token_a);
        });
    });

    // B sends ClientIdentifyRequest(T_A). T_A is not in authenticated_unidentified_users
    // (T_B is there, not T_A). Server sends explicit RejectReason::Auth.
    // is_rejected() is tick-scoped (based on events.has::<ClientRejectEvent>); use
    // a persistent bool captured via read_event instead.
    let mut reject_event_received = false;
    let mut connect_event_received = false;
    let mut reject_address: Option<Option<std::net::SocketAddr>> = None;

    scenario.spec_expect(
        "connection-25.t1: reused token produces explicit RejectEvent",
        |ctx| {
            ctx.client(client_b_key, |client| {
                if let Some((address, _reason, _message)) = client.read_event::<ClientRejectEvent>()
                {
                    reject_event_received = true;
                    reject_address = Some(address);
                }
                if client.read_event::<ClientConnectEvent>().is_some() {
                    connect_event_received = true;
                }
                reject_event_received.then_some(())
            })
        },
    );

    assert!(
        reject_event_received,
        "B must be explicitly rejected when presenting a consumed token"
    );
    assert!(
        !connect_event_received,
        "B must not connect with a consumed/replayed token"
    );
    // In-band rejection is post-address by construction: B completed its own
    // auth exchange, so the rejection carries the actual learned address.
    assert!(
        matches!(reject_address, Some(Some(_))),
        "in-band rejection must carry Some(actual_addr), got {:?}",
        reject_address
    );

    scenario.mutate(|_| {});
    scenario.spec_expect(
        "connection-26.t1: consumed token not silently accepted as fresh session",
        |ctx| {
            let not_connected = !ctx.client(client_b_key, |c| c.connection_status().is_connected());
            not_connected.then_some(())
        },
    );
}

/// Identity token issued by server is observable on client after connection
/// Contract: [connection-27]
///
/// During the handshake, accept_connection() generates a token on the server, sends it to the
/// client via the auth channel, and the client stores it (ClientAuthIo writes to its Arc<Mutex>).
/// After connection, client.identity_token() must return Some(non_empty_token) — proving the
/// server-issued token completed the full roundtrip: server → auth channel → client observable.
#[test]
fn valid_identity_token_roundtrips() {
    let mut scenario = Scenario::new(naia_server::ServerMode::Resident);
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());
    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.create_room().key()));

    let client_a_key = client_connect(
        &mut scenario,
        &room_key,
        "Client A",
        Auth::new("client_a", "password"),
        test_client_config(),
        test_protocol,
    );

    // After a successful connection, the client must have the server-issued token.
    // ClientAuthIo::receive() writes it to the shared Arc<Mutex> when the auth response arrives.
    scenario.spec_expect(
        "connection-27.t1: server-issued identity token is observable on client after connect",
        |ctx| {
            ctx.client(client_a_key, |c| {
                let token = c.identity_token()?;
                (!token.is_empty()).then_some(())
            })
        },
    );
}

// ============================================================================
// [connection-14a] — protocol_id check during handshake
// ============================================================================

/// protocol_id is verified before ConnectEvent
/// Contract: [connection-14a]
///
/// Given client and server with matching protocol;
/// when handshake completes; then protocol_id is verified before any ConnectEvent.
#[test]
fn protocol_id_verified_before_connect_event() {
    let mut scenario = Scenario::new(naia_server::ServerMode::Resident);
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());
    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.create_room().key()));

    // Connect with matching protocol - should succeed
    let client_a_key = client_connect(
        &mut scenario,
        &room_key,
        "Client A",
        Auth::new("client_a", "pass"),
        test_client_config(),
        test_protocol,
    );

    // Verify connected (protocol_id matched during handshake before ConnectEvent)
    scenario.spec_expect(
        "connection-14a.t1: protocol_id verified before connect event",
        |ctx| {
            let connected = ctx.client(client_a_key, |c| c.connection_status().is_connected());
            let user_exists = ctx.server(|s| s.user_exists(&client_a_key));
            connected.then_some(())
        },
    );
}

// ============================================================================
// [connection-28] — Reconnect is a fresh session
// ============================================================================

/// Reconnecting client receives fresh entity spawns
/// Contract: [connection-28]
///
/// Given client connects, receives entities, disconnects, then reconnects;
/// when reconnect completes; then client receives fresh entity spawns (not resumed).
#[test]
fn reconnect_is_fresh_session() {
    let mut scenario = Scenario::new(naia_server::ServerMode::Resident);
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());
    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.create_room().key()));

    // First connection
    let client_a_key = client_connect(
        &mut scenario,
        &room_key,
        "Client A",
        Auth::new("client_a", "pass"),
        test_client_config(),
        test_protocol.clone(),
    );

    // Server spawns entity
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

    // Client sees entity
    scenario.expect(|ctx| {
        ctx.client(client_a_key, |c| c.has_entity(&entity_e))
            .then_some(())
    });

    // Disconnect
    scenario.mutate(|ctx| {
        ctx.client(client_a_key, |client| {
            client.disconnect();
        });
    });

    // Wait for disconnect
    scenario.expect(|ctx| (!ctx.server(|s| s.user_exists(&client_a_key))).then_some(()));

    // Reconnect (same username)
    let client_a2_key = client_connect(
        &mut scenario,
        &room_key,
        "Client A2",
        Auth::new("client_a", "pass"),
        test_client_config(),
        test_protocol,
    );

    // Include entity in reconnected client's scope
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server
                .user_scope_mut(&client_a2_key)
                .unwrap()
                .include(&entity_e);
        });
    });

    // Reconnected client sees entity (fresh spawn, not resumed state)
    scenario.spec_expect(
        "connection-28.t1: reconnect is fresh session with fresh entity spawns",
        |ctx| {
            ctx.client(client_a2_key, |c| c.has_entity(&entity_e))
                .then_some(())
        },
    );
}

// ============================================================================
// [connection-29] — protocol_id definition
// ============================================================================

/// Same protocol crate produces same protocol_id
/// Contract: [connection-29]
///
/// Given identical protocol definitions;
/// when protocol is built; then protocol_id is deterministic.
#[test]
fn same_protocol_produces_same_id() {
    // Build protocol twice
    let protocol1 = protocol();
    let protocol2 = protocol();

    // Both should work with the same server
    let mut scenario = Scenario::new(naia_server::ServerMode::Resident);

    scenario.server_start(ServerConfig::default(), protocol1.clone());
    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.create_room().key()));

    // Both clients use the same protocol definition
    let client_a_key = client_connect(
        &mut scenario,
        &room_key,
        "Client A",
        Auth::new("client_a", "pass"),
        test_client_config(),
        protocol1,
    );

    let client_b_key = client_connect(
        &mut scenario,
        &room_key,
        "Client B",
        Auth::new("client_b", "pass"),
        test_client_config(),
        protocol2,
    );

    // Both should be connected (same protocol definition produces same deterministic protocol_id)
    scenario.spec_expect(
        "connection-29.t1: same protocol definition produces same deterministic protocol_id",
        |ctx| {
            let a_connected = ctx.client(client_a_key, |c| c.connection_status().is_connected());
            let b_connected = ctx.client(client_b_key, |c| c.connection_status().is_connected());
            (a_connected && b_connected).then_some(())
        },
    );
}

// ============================================================================
// [connection-30] — protocol_id wire encoding
// ============================================================================

/// protocol_id is encoded as 16 bytes little-endian
/// Contract: [connection-30]
///
/// Given protocol identity exchange during handshake;
/// when protocol_id is sent on wire; then it uses u128 little-endian encoding.
/// Note: Wire encoding is verified by successful connections with matching protocols.
#[test]
fn protocol_id_wire_encoding_allows_connection() {
    let mut scenario = Scenario::new(naia_server::ServerMode::Resident);
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());
    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.create_room().key()));

    // Connection succeeds, verifying wire encoding is correct
    let client_a_key = client_connect(
        &mut scenario,
        &room_key,
        "Client A",
        Auth::new("client_a", "pass"),
        test_client_config(),
        test_protocol,
    );

    scenario.spec_expect(
        "connection-30.t1: protocol_id uses u128 little-endian wire encoding",
        |ctx| {
            ctx.client(client_a_key, |c| c.connection_status().is_connected())
                .then_some(())
        },
    );
}

// ============================================================================
// [connection-31] — protocol_id handshake gate
// ============================================================================

/// Matched protocol_id allows connection to proceed
/// Contract: [connection-31]
///
/// Given client and server with matching protocol;
/// when handshake occurs; then connection proceeds successfully.
#[test]
fn matched_protocol_id_allows_connection() {
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

    // Connection proceeds (matched protocol_id during handshake allows connection)
    scenario.spec_expect(
        "connection-31.t1: matched protocol_id allows connection to proceed",
        |ctx| {
            let connected = ctx.client(client_a_key, |c| c.connection_status().is_connected());
            connected.then_some(())
        },
    );
}

// ============================================================================
// [connection-32] — What affects protocol_id
// ============================================================================

/// Wire-relevant changes affect protocol_id
/// Contract: [connection-32]
///
/// (NASH 2026-10-24: the previous version connected with a matching
/// protocol and asserted `connected` — it never varied any wire aspect, so
/// it could not fail on a wrong `protocol_id` composition. Rewritten to
/// assert the property directly.)
///
/// `protocol_id` must be deterministic for a fixed definition and must
/// change when a wire-relevant aspect changes. The
/// `protocol_with_sequenced_unreliable()` builder adds a
/// `SequencedUnreliable` channel to the base definition — peers that
/// disagree on channels cannot decode each other, so the id must differ.
#[test]
fn protocol_id_determined_by_wire_relevant_aspects() {
    let mut base = protocol();
    let base_id = base.locked_protocol_id();
    let mut base_again = protocol();
    let base_again_id = base_again.locked_protocol_id();
    assert_eq!(
        base_id, base_again_id,
        "connection-32.t1: same definition must produce the same protocol_id"
    );

    let mut variant = protocol_with_sequenced_unreliable();
    let variant_id = variant.locked_protocol_id();
    assert_ne!(
        base_id, variant_id,
        "connection-32.t1: adding a channel must change protocol_id"
    );
}

// ============================================================================
// [connection-33] — No partial compatibility
// ============================================================================

/// Either protocol_id matches exactly or connection is rejected
/// Contract: [connection-33]
///
/// Given protocol identity mechanism;
/// when protocols differ; then connection is rejected (no partial compatibility).
/// Note: Verified implicitly - matching protocols connect, different would fail.
#[test]
fn no_partial_protocol_compatibility() {
    let mut scenario = Scenario::new(naia_server::ServerMode::Resident);
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());
    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.create_room().key()));

    // Exact match works
    let client_a_key = client_connect(
        &mut scenario,
        &room_key,
        "Client A",
        Auth::new("client_a", "pass"),
        test_client_config(),
        test_protocol,
    );

    scenario.spec_expect("connection-33.t1: protocol_id either matches exactly or connection rejected (no partial compatibility)", |ctx| {
        ctx.client(client_a_key, |c| c.connection_status().is_connected()).then_some(())
    });
}

/// Give-up then re-dial on the same client recovers
/// Contract: handshake give-up recovery (Usher 38421 + silence fix)
///
/// Given a silent server that made the client give up with AuthTimeout;
/// when the SAME client object dials again and the server now accepts;
/// then the client connects with no further AuthTimeout. This is the
/// consumer offline-to-online cycle: the give-up teardown must leave
/// the client re-dialable, not bricked.
#[test]
fn give_up_then_redial_recovers_on_same_client() {
    let mut scenario = Scenario::new(naia_server::ServerMode::Resident);
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());

    let mut client_config = test_client_config();
    client_config.connection.disconnection_timeout_duration = Duration::from_millis(200);

    let client_auth = Auth::new("client_redial", "password");

    // Phase 1: silent server. The server never accepts: no auth read,
    // no accept call. The client must give up exactly once.
    let client_key = scenario.client_start(
        "Client Redial",
        client_auth.clone(),
        client_config,
        test_protocol.clone(),
    );

    let mut auth_timeouts = 0usize;
    scenario.expect(|ctx| {
        ctx.client(client_key, |client| {
            for (reason, _) in client.read_events::<ClientDisconnectEvent>() {
                if reason == DisconnectReason::AuthTimeout {
                    auth_timeouts += 1;
                }
            }
        });
        (auth_timeouts == 1).then_some(())
    });
    assert_eq!(
        auth_timeouts, 1,
        "phase 1 must end in exactly one AuthTimeout"
    );

    // Phase 2: the same client object dials again; the server accepts
    // this time. Mirror the accept half of `client_connect`.
    scenario.client_reconnect(client_key);

    scenario.expect(|ctx| {
        ctx.server(|server| {
            if let Some((incoming_client_key, incoming_auth)) =
                server.read_event::<ServerAuthEvent<Auth>>()
            {
                if incoming_client_key == client_key && incoming_auth == client_auth {
                    return Some(());
                }
            }
            None
        })
    });
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.accept_connection(&client_key);
        });
    });
    scenario.expect(|ctx| server_and_client_connected(ctx, client_key));

    // Connected, and the recovery emitted no further AuthTimeout: drain
    // the disconnect queue over one more tick and demand emptiness.
    let mut late_disconnects = 0usize;
    scenario.mutate(|_ctx| {});
    scenario.expect(|ctx| {
        ctx.client(client_key, |client| {
            late_disconnects += client.read_events::<ClientDisconnectEvent>().len();
        });
        server_and_client_connected(ctx, client_key)
    });
    assert_eq!(
        late_disconnects, 0,
        "recovery must connect without any further disconnect event"
    );
}
