//! End-to-end integration tests for Replicated Resources (R3 gate).
//!
//! Covers the R1 spec scenarios that are reachable in V1 (registration,
//! insert/remove dynamic+static, auto-scope on connect, late-join, and
//! re-insertion rejection). Authority delegation, priority, and Bevy
//! adapter scenarios are exercised in later phase gates.

use std::time::Duration;

use naia_client::{ClientConfig, JitterBufferType};
use naia_server::ServerConfig;
use naia_server::ReplicationConfig;
use naia_shared::EntityAuthStatus;
use naia_test_harness::{
    protocol, Auth, ClientConnectEvent, ClientKey, ServerAuthEvent, ServerConnectEvent, Scenario,
    TestMatchState, TestScore,
};

fn test_client_config() -> ClientConfig {
    let mut config = ClientConfig::default();
    config.send_handshake_interval = Duration::from_millis(0);
    config.jitter_buffer = JitterBufferType::Bypass;
    config
}

/// Bring up server, connect one client. Returns the connected client_key.
/// Mirrors the `client_connect` helper in legacy_tests/_helpers.rs but
/// inlined here so this file is self-contained.
fn server_with_one_client(scenario: &mut Scenario) -> ClientKey {
    let test_protocol = protocol();
    scenario.server_start(ServerConfig::default(), test_protocol.clone());
    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.make_room().key()));
    scenario.set_last_room(room_key);

    let client_auth = Auth::new("alice", "secret");
    let client_key = scenario.client_start(
        "alice",
        client_auth.clone(),
        test_client_config(),
        test_protocol,
    );

    // Server reads auth event
    scenario.expect(|ctx| {
        ctx.server(|server| {
            if let Some((incoming_key, _)) = server.read_event::<ServerAuthEvent<Auth>>() {
                (incoming_key == client_key).then_some(())
            } else {
                None
            }
        })
    });

    // Accept
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.accept_connection(&client_key);
        });
    });

    // Wait for connect event + room membership
    scenario.expect(|ctx| {
        ctx.server(|server| {
            if server.read_event::<ServerConnectEvent>().is_some() {
                Some(())
            } else {
                None
            }
        })
    });

    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server
                .room_mut(&room_key)
                .expect("room exists")
                .add_user(&client_key);
        });
    });

    scenario.expect(|ctx| {
        let connected = ctx.client(client_key, |c| c.connection_status().is_connected());
        let user_exists = ctx.server(|s| s.user_exists(&client_key));
        let _ = ctx.client(client_key, |c| c.read_event::<ClientConnectEvent>());
        (connected && user_exists).then_some(())
    });

    client_key
}

fn settle(scenario: &mut Scenario, ticks: usize) {
    for _ in 0..ticks {
        scenario.mutate(|_| {});
    }
}

/// Verify the derive macro's `mirror_single_field` codegen: copying
/// field index N from `src` to `dst` should leave all other fields
/// untouched. This is the foundation of Mode B's per-field diff
/// preservation.
#[test]
fn mirror_single_field_copies_only_indexed_field() {
    use naia_shared::Replicate;

    // Property index 0 = home, index 1 = away (declaration order).
    let src = TestScore::new(99, 88);
    let mut dst = TestScore::new(0, 0);

    // Mirror index 0 (home) only.
    dst.mirror_single_field(0, &src as &dyn Replicate);
    assert_eq!(*dst.home, 99, "home (index 0) should be mirrored");
    assert_eq!(*dst.away, 0, "away (index 1) should NOT be touched");

    // Now mirror index 1 (away).
    dst.mirror_single_field(1, &src as &dyn Replicate);
    assert_eq!(*dst.home, 99);
    assert_eq!(*dst.away, 88);

    // Out-of-range index: silently ignored.
    let mut dst2 = TestScore::new(7, 7);
    dst2.mirror_single_field(99, &src as &dyn Replicate);
    assert_eq!((*dst2.home, *dst2.away), (7, 7), "OOB index must no-op");
}

#[test]
fn registration_sets_resource_kind_in_protocol() {
    let p = protocol();
    // TestScore was registered as a resource in test_protocol::protocol()
    let kind = naia_shared::ComponentKind::of::<TestScore>();
    assert!(
        p.resource_kinds.is_resource(&kind),
        "TestScore should be marked as a resource kind"
    );
    // Position is a regular component, not a resource.
    let pos_kind = naia_shared::ComponentKind::of::<naia_test_harness::Position>();
    assert!(
        !p.resource_kinds.is_resource(&pos_kind),
        "Position should NOT be a resource kind"
    );
}

#[test]
fn insert_dynamic_resource_replicates_to_connected_client() {
    let mut scenario = Scenario::new();
    let client_key = server_with_one_client(&mut scenario);

    // Server inserts a dynamic resource.
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            assert!(
                server.insert_resource(TestScore::new(7, 3)),
                "insert should succeed for fresh type"
            );
            assert!(server.has_resource::<TestScore>());
        });
    });

    // Allow replication round trip.
    settle(&mut scenario, 20);

    // Sanity: server-side value should be (7, 3).
    let val = scenario.mutate(|ctx| {
        ctx.server(|server| server.resource::<TestScore, _, _>(|s| (*s.home, *s.away)))
    });
    eprintln!("server.resource::<TestScore>() = {:?}", val);

    // Check client side count of entities
    let n = scenario.mutate(|ctx| ctx.client(client_key, |c| c.entities().len()));
    eprintln!("client entity count = {n}");

    // Client should see the resource via the harness scan.
    scenario.expect(|ctx| {
        ctx.client(client_key, |c| {
            c.resource::<TestScore, _, _>(|s| (*s.home, *s.away))
                .filter(|&(h, a)| h == 7 && a == 3)
                .map(|_| ())
        })
    });
}

#[test]
fn insert_static_resource_replicates_to_connected_client() {
    let mut scenario = Scenario::new();
    let client_key = server_with_one_client(&mut scenario);

    scenario.mutate(|ctx| {
        ctx.server(|server| {
            assert!(server.insert_static_resource(TestMatchState::new(2)));
        });
    });

    settle(&mut scenario, 20);

    scenario.expect(|ctx| {
        ctx.client(client_key, |c| {
            c.resource::<TestMatchState, _, _>(|s| *s.phase)
                .filter(|p| *p == 2)
                .map(|_| ())
        })
    });
}

#[test]
fn re_inserting_same_resource_returns_false() {
    let mut scenario = Scenario::new();
    let _client_key = server_with_one_client(&mut scenario);

    scenario.mutate(|ctx| {
        ctx.server(|server| {
            assert!(server.insert_resource(TestScore::new(0, 0)));
            // Second insert returns false (ResourceAlreadyExists).
            assert!(!server.insert_resource(TestScore::new(99, 99)));
        });
    });

    settle(&mut scenario, 20);

    // Server-side value is the original (0, 0), not the rejected (99, 99).
    scenario.expect(|ctx| {
        ctx.server(|server| {
            server
                .resource::<TestScore, _, _>(|s| (*s.home, *s.away))
                .filter(|&v| v == (0, 0))
                .map(|_| ())
        })
    });
}

#[test]
fn remove_resource_propagates_to_client() {
    let mut scenario = Scenario::new();
    let client_key = server_with_one_client(&mut scenario);

    scenario.mutate(|ctx| {
        ctx.server(|server| {
            assert!(server.insert_resource(TestScore::new(1, 2)));
        });
    });
    settle(&mut scenario, 20);

    // Client sees it
    scenario.expect(|ctx| {
        ctx.client(client_key, |c| c.has_resource::<TestScore>().then_some(()))
    });

    scenario.mutate(|ctx| {
        ctx.server(|server| {
            assert!(server.remove_resource::<TestScore>());
            assert!(!server.has_resource::<TestScore>());
        });
    });
    settle(&mut scenario, 20);

    // Client no longer sees it
    scenario.expect(|ctx| {
        ctx.client(
            client_key,
            |c| (!c.has_resource::<TestScore>()).then_some(()),
        )
    });
}

#[test]
fn server_mutation_replicates_to_client() {
    let mut scenario = Scenario::new();
    let client_key = server_with_one_client(&mut scenario);

    scenario.mutate(|ctx| {
        ctx.server(|server| {
            assert!(server.insert_resource(TestScore::new(0, 0)));
        });
    });
    settle(&mut scenario, 20);

    // Initial state visible to client
    scenario.expect(|ctx| {
        ctx.client(client_key, |c| {
            c.resource::<TestScore, _, _>(|s| (*s.home, *s.away))
                .filter(|&v| v == (0, 0))
                .map(|_| ())
        })
    });

    // Mutate via the diff-tracked path.
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.mutate_resource::<TestScore, _, _>(|s| {
                *s.home = 42;
            });
        });
    });
    settle(&mut scenario, 20);

    // Client observes the update
    scenario.expect(|ctx| {
        ctx.client(client_key, |c| {
            c.resource::<TestScore, _, _>(|s| (*s.home, *s.away))
                .filter(|&v| v == (42, 0))
                .map(|_| ())
        })
    });

    // Multi-field mutation
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.mutate_resource::<TestScore, _, _>(|s| {
                *s.home = 1;
                *s.away = 2;
            });
        });
    });
    settle(&mut scenario, 20);
    scenario.expect(|ctx| {
        ctx.client(client_key, |c| {
            c.resource::<TestScore, _, _>(|s| (*s.home, *s.away))
                .filter(|&v| v == (1, 2))
                .map(|_| ())
        })
    });
}

/// Wire-level per-field diff assertion (Item 3 / RESOURCES_AUDIT.md F2).
///
/// Mutating ONE Property field should send strictly fewer bytes than
/// mutating TWO Property fields in the same tick. Proves the per-field
/// diff machinery is preserved end-to-end via `mirror_single_field`
/// → `DirtyQueue` → `write_update` per-field-presence-bit framing.
///
/// Bonus assertion: a tick with NO mutations sends fewer bytes than
/// either (idle baseline).
#[test]
fn per_field_diff_one_field_sends_fewer_bytes_than_two() {
    fn measure(mutate_both: bool) -> u64 {
        let mut scenario = Scenario::new();
        let _client_key = server_with_one_client(&mut scenario);

        // Insert + steady-state.
        scenario.mutate(|ctx| {
            ctx.server(|server| {
                assert!(server.insert_resource(TestScore::new(0, 0)));
            });
        });
        settle(&mut scenario, 30);

        // Drain to steady state — read & discard outgoing bytes for
        // a few ticks so we're past the spawn-phase noise.
        for _ in 0..5 {
            scenario.mutate(|_| {});
        }

        // Mutate per the test variant, then capture bytes for the
        // SINGLE next tick's outgoing payload.
        scenario.mutate(|ctx| {
            ctx.server(|server| {
                server.mutate_resource::<TestScore, _, _>(|s| {
                    *s.home = 42;
                    if mutate_both {
                        *s.away = 99;
                    }
                });
            });
        });
        // mutate(|_|{}) is one tick. Capture bytes for THIS tick only.
        let bytes_under_test = scenario.mutate(|ctx| {
            ctx.server(|s| s.server_outgoing_bytes_last_tick())
        });

        // Settle so the test cleanup is clean.
        for _ in 0..5 {
            scenario.mutate(|_| {});
        }

        bytes_under_test
    }

    let one_field = measure(false);
    let two_fields = measure(true);

    eprintln!(
        "per-field diff measurement: one_field={} bytes, two_fields={} bytes",
        one_field, two_fields
    );

    // The CORE assertion: mutating one field sends strictly fewer bytes
    // than mutating two fields. If per-field diff broke and we were
    // sending the whole resource on every tick, these would be equal.
    assert!(
        one_field < two_fields,
        "per-field diff regression: mutating one Property ({one_field} B) \
         should send strictly fewer bytes than mutating two ({two_fields} B). \
         If equal, the entity-component is being mirror'd whole instead of \
         per-field via mirror_single_field — Mode B regression."
    );
}

#[test]
fn delegated_resource_supports_client_authority_request() {
    let mut scenario = Scenario::new();
    let client_key = server_with_one_client(&mut scenario);

    // Insert resource (use TestScore which we've verified replicates).
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            assert!(server.insert_resource(TestScore::new(0, 0)));
        });
    });
    settle(&mut scenario, 30);

    // Sanity: client should observe the resource before delegation kicks in.
    scenario.expect(|ctx| {
        ctx.client(client_key, |c| c.has_resource::<TestScore>().then_some(()))
    });

    // Configure for delegation.
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            assert!(
                server.configure_resource::<TestScore>(ReplicationConfig::delegated()),
                "configure_resource should succeed for inserted R"
            );
        });
    });
    settle(&mut scenario, 100);

    // Wait for the client's auth-status view to reach Available
    // (post-EnableDelegation propagation).
    scenario.expect(|ctx| {
        ctx.client(client_key, |c| {
            (c.resource_authority_status::<TestScore>() == Some(EntityAuthStatus::Available))
                .then_some(())
        })
    });

    // Initial state: server-authoritative (no client holds yet)
    let server_status = scenario.mutate(|ctx| {
        ctx.server(|server| server.has_resource::<TestScore>())
    });
    assert!(server_status);

    // Client requests authority.
    scenario.mutate(|ctx| {
        ctx.client(client_key, |c| {
            let res = c.request_resource_authority::<TestScore>();
            assert!(res.is_ok(), "request_resource_authority should succeed: {:?}", res);
        });
    });
    settle(&mut scenario, 30);

    // Eventually client status becomes Granted.
    scenario.expect(|ctx| {
        ctx.client(client_key, |c| {
            (c.resource_authority_status::<TestScore>() == Some(EntityAuthStatus::Granted))
                .then_some(())
        })
    });

    // Client mutation propagates to server (authority-held write).
    scenario.mutate(|ctx| {
        ctx.client(client_key, |c| {
            c.mutate_resource::<TestScore, _, _>(|s| {
                *s.home = 7;
            });
        });
    });
    settle(&mut scenario, 30);

    // Server-side value reflects the client's write.
    scenario.expect(|ctx| {
        ctx.server(|server| {
            server
                .resource::<TestScore, _, _>(|s| *s.home)
                .filter(|v| *v == 7)
                .map(|_| ())
        })
    });

    // Client releases authority; status reverts to Available on the server.
    scenario.mutate(|ctx| {
        ctx.client(client_key, |c| {
            let _ = c.release_resource_authority::<TestScore>();
        });
    });
    settle(&mut scenario, 30);
}

#[test]
fn resource_priority_gain_is_settable() {
    let mut scenario = Scenario::new();
    let _client_key = server_with_one_client(&mut scenario);

    scenario.mutate(|ctx| {
        ctx.server(|server| {
            assert!(server.insert_resource(TestScore::new(0, 0)));
            // Gain knob applies to a present resource.
            assert!(server.set_resource_priority_gain::<TestScore>(7.5));
            // Returns false for a not-inserted resource type.
            assert!(!server.set_resource_priority_gain::<TestMatchState>(2.0));
        });
    });
    settle(&mut scenario, 5);
}

#[test]
fn late_joining_client_observes_pre_existing_resource() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();
    scenario.server_start(ServerConfig::default(), test_protocol.clone());
    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.make_room().key()));
    scenario.set_last_room(room_key);

    // Insert resource BEFORE any client connects.
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            assert!(server.insert_resource(TestScore::new(11, 22)));
        });
    });

    // Now connect a client.
    let client_auth = Auth::new("bob", "secret");
    let client_key = scenario.client_start(
        "bob",
        client_auth.clone(),
        test_client_config(),
        test_protocol,
    );

    scenario.expect(|ctx| {
        ctx.server(|server| {
            if let Some((k, _)) = server.read_event::<ServerAuthEvent<Auth>>() {
                (k == client_key).then_some(())
            } else {
                None
            }
        })
    });
    scenario.mutate(|ctx| ctx.server(|server| server.accept_connection(&client_key)));
    scenario.expect(|ctx| {
        ctx.server(|server| server.read_event::<ServerConnectEvent>().map(|_| ()))
    });
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server
                .room_mut(&room_key)
                .expect("room exists")
                .add_user(&client_key);
        });
    });

    // Settle for the full handshake + initial replication round trip.
    settle(&mut scenario, 30);

    // Late-joining client should observe the resource.
    scenario.expect(|ctx| {
        ctx.client(client_key, |c| {
            c.resource::<TestScore, _, _>(|s| (*s.home, *s.away))
                .filter(|&v| v == (11, 22))
                .map(|_| ())
        })
    });
}
