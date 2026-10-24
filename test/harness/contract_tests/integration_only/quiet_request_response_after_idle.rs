//! RED 108734 repro: a request/response exchange that starts after a long
//! quiet idle must still complete. `ad00ad34c` ("Send tick data boundaries
//! when replicated state is quiet") makes the server emit one empty Data
//! packet per tick on quiet links; the cyberlith e2e
//! `saved_draft_reopens_in_new_cell_and_reaches_play` then stalls with
//! `LevelEditorPublishResponse not received within 200 ticks` while the game
//! link keeps flowing. This test mirrors that shape at naia level: a long
//! quiet idle, then a client request, a server echo response, and a bounded
//! wait for the client to observe it.

use naia_server::ServerConfig;

use naia_test_harness::{protocol, Auth, ClientKey, Scenario, ToTicks};

// Test protocol types (channels and messages)
use naia_test_harness::test_protocol::{
    OrderedChannel, RequestResponseChannel, TestRequest, TestResponse,
};

mod _helpers;
use _helpers::{
    client_connect, server_and_client_connected, server_and_client_disconnected, test_client_config,
};

/// Control: request/response with no preceding idle completes promptly.
#[test]
fn request_response_without_idle_control() {
    let mut scenario = Scenario::new(naia_server::ServerMode::Resident);
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());
    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.create_room().key()));

    let client_key = client_connect(
        &mut scenario,
        &room_key,
        "Client A",
        Auth::new("client_a", "pass"),
        test_client_config(),
        test_protocol,
    );

    roundtrip_once(&mut scenario, client_key);
}

/// RED 108734: the same exchange after a long quiet idle must also complete
/// within the e2e's 200-tick bound.
#[test]
fn request_response_after_long_quiet_idle() {
    let mut scenario = Scenario::new(naia_server::ServerMode::Resident);
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());
    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.create_room().key()));

    let client_key = client_connect(
        &mut scenario,
        &room_key,
        "Client A",
        Auth::new("client_a", "pass"),
        test_client_config(),
        test_protocol,
    );

    // Long quiet idle: no messages, no entities. Post-ad00 the server emits
    // one empty Data boundary per server tick through all of this.
    // 1000 ticks at 16ms covers the e2e's full authoring+play quiet span.
    for _ in 0..1000 {
        scenario.mutate(|_| {});
    }

    roundtrip_once(&mut scenario, client_key);
}

/// RED 108734 variant: the server takes its time answering (level-server
/// roundtrip in the e2e spans dozens of ticks between request receipt and
/// the echo). The exchange must still complete.
#[test]
fn request_response_after_idle_with_slow_server_echo() {
    let mut scenario = Scenario::new(naia_server::ServerMode::Resident);
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());
    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.create_room().key()));

    let client_key = client_connect(
        &mut scenario,
        &room_key,
        "Client A",
        Auth::new("client_a", "pass"),
        test_client_config(),
        test_protocol,
    );

    for _ in 0..1000 {
        scenario.mutate(|_| {});
    }

    let response_key = scenario.mutate(|ctx| {
        ctx.client(client_key, |client| {
            client
                .send_request::<RequestResponseChannel, TestRequest>(&TestRequest::new("test"))
                .expect("request must be accepted by reliable channel")
        })
    });

    let response_send_key = scenario.expect_msg(
        "slow-echo.t1: server receives request after quiet idle",
        |ctx| {
            ctx.server(|server| {
                for (recv_client_key, resp_id, _request) in
                    server.read_request::<RequestResponseChannel, TestRequest>()
                {
                    if recv_client_key == client_key {
                        return Some(naia_shared::ResponseSendKey::new(resp_id));
                    }
                }
                None
            })
        },
    );

    // Server sits on the request for 50 ticks before answering.
    for _ in 0..50 {
        scenario.mutate(|_| {});
    }

    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.send_response(&response_send_key, &TestResponse::new("ok"));
        });
    });

    scenario.expect_msg(
        "slow-echo.t2: client receives slow response after quiet idle",
        |ctx| {
            ctx.client(client_key, |client| {
                client.has_response(&response_key).then_some(())
            })
        },
    );
}

/// RED 108734 wire-faithful variant: the e2e publish runs on
/// `LevelCatalogChannel`, which is `OrderedReliable` — not the unordered
/// channel the other tests above use. Same long idle, same bounded wait.
#[test]
fn request_response_after_idle_on_ordered_channel() {
    let mut scenario = Scenario::new(naia_server::ServerMode::Resident);
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());
    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.create_room().key()));

    let client_key = client_connect(
        &mut scenario,
        &room_key,
        "Client A",
        Auth::new("client_a", "pass"),
        test_client_config(),
        test_protocol,
    );

    for _ in 0..1000 {
        scenario.mutate(|_| {});
    }

    let response_key = scenario.mutate(|ctx| {
        ctx.client(client_key, |client| {
            client
                .send_request::<OrderedChannel, TestRequest>(&TestRequest::new("test"))
                .expect("request must be accepted by reliable channel")
        })
    });

    let response_send_key = scenario.expect_msg(
        "ordered.t1: server receives request after quiet idle",
        |ctx| {
            ctx.server(|server| {
                for (recv_client_key, resp_id, _request) in
                    server.read_request::<OrderedChannel, TestRequest>()
                {
                    if recv_client_key == client_key {
                        return Some(naia_shared::ResponseSendKey::new(resp_id));
                    }
                }
                None
            })
        },
    );

    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.send_response(&response_send_key, &TestResponse::new("ok"));
        });
    });

    scenario.expect_msg(
        "ordered.t2: client receives response after quiet idle",
        |ctx| {
            ctx.client(client_key, |client| {
                client.has_response(&response_key).then_some(())
            })
        },
    );
}

/// RED 108734 e2e-faithful shape: the failing e2e exchange is the SECOND
/// catalog request on one connection, not a request after pure idle. In
/// `saved_draft_reopens_in_new_cell_and_reaches_play` a create request on
/// `LevelCatalogChannel` (`OrderedReliable`) is answered early, ~600 ticks of
/// authoring/play traffic follow, then the publish request on the same channel
/// and connection never resolves within 200 ticks. Mirror that here: first
/// ordered exchange succeeds, a busy span passes, the second ordered exchange
/// must complete within the e2e's exact 200-tick bound.
#[test]
fn second_ordered_request_after_busy_span_completes_within_200_ticks() {
    let mut scenario = Scenario::new(naia_server::ServerMode::Resident);
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());
    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.create_room().key()));

    let client_key = client_connect(
        &mut scenario,
        &room_key,
        "Client A",
        Auth::new("client_a", "pass"),
        test_client_config(),
        test_protocol,
    );

    // First exchange (create-shape): answered immediately, must succeed.
    let first_key = scenario.mutate(|ctx| {
        ctx.client(client_key, |client| {
            client
                .send_request::<OrderedChannel, TestRequest>(&TestRequest::new("first"))
                .expect("first request must be accepted by reliable channel")
        })
    });
    let first_send_key =
        scenario.expect_msg("twoshot.t1: server receives first ordered request", |ctx| {
            ctx.server(|server| {
                for (recv_client_key, resp_id, _request) in
                    server.read_request::<OrderedChannel, TestRequest>()
                {
                    if recv_client_key == client_key {
                        return Some(naia_shared::ResponseSendKey::new(resp_id));
                    }
                }
                None
            })
        });
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.send_response(&first_send_key, &TestResponse::new("ok"));
        });
    });
    scenario.expect_msg(
        "twoshot.t2: client receives first ordered response",
        |ctx| {
            ctx.client(client_key, |client| {
                client.has_response(&first_key).then_some(())
            })
        },
    );

    // Busy span: both sides tick with no catalog traffic, like the e2e's
    // authoring/save/leave/rejoin/play legs between create and publish.
    for _ in 0..650 {
        scenario.mutate(|_| {});
    }

    // Second exchange (publish-shape): the server answers immediately; the
    // client must observe within the e2e's exact 200-tick bound.
    let second_key = scenario.mutate(|ctx| {
        ctx.client(client_key, |client| {
            client
                .send_request::<OrderedChannel, TestRequest>(&TestRequest::new("second"))
                .expect("second request must be accepted by reliable channel")
        })
    });
    let second_send_key = scenario.until(200.ticks()).expect_msg(
        "twoshot.t3: server receives second ordered request within 200 ticks",
        |ctx| {
            ctx.server(|server| {
                for (recv_client_key, resp_id, _request) in
                    server.read_request::<OrderedChannel, TestRequest>()
                {
                    if recv_client_key == client_key {
                        return Some(naia_shared::ResponseSendKey::new(resp_id));
                    }
                }
                None
            })
        },
    );
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.send_response(&second_send_key, &TestResponse::new("ok"));
        });
    });
    scenario.until(200.ticks()).expect_msg(
        "twoshot.t4: client receives second ordered response within 200 ticks",
        |ctx| {
            ctx.client(client_key, |client| {
                client.has_response(&second_key).then_some(())
            })
        },
    );
}

/// One client request, server echo, bounded client wait. Shared by the
/// control and the idle repro so only the idle differs.
fn roundtrip_once(scenario: &mut Scenario, client_key: ClientKey) {
    let response_key = scenario.mutate(|ctx| {
        ctx.client(client_key, |client| {
            client
                .send_request::<RequestResponseChannel, TestRequest>(&TestRequest::new("test"))
                .expect("request must be accepted by reliable channel")
        })
    });

    let response_send_key = scenario.expect_msg(
        "idle-reqresp.t1: server receives request after quiet idle",
        |ctx| {
            ctx.server(|server| {
                for (recv_client_key, resp_id, _request) in
                    server.read_request::<RequestResponseChannel, TestRequest>()
                {
                    if recv_client_key == client_key {
                        return Some(naia_shared::ResponseSendKey::new(resp_id));
                    }
                }
                None
            })
        },
    );

    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.send_response(&response_send_key, &TestResponse::new("ok"));
        });
    });

    scenario.expect_msg(
        "idle-reqresp.t2: client receives response after quiet idle",
        |ctx| {
            ctx.client(client_key, |client| {
                client.has_response(&response_key).then_some(())
            })
        },
    );

    scenario.mutate(|ctx| {
        ctx.client(client_key, |client| {
            let response = client
                .receive_response::<TestResponse>(&response_key)
                .expect("response must be available after has_response returned true");
            assert_eq!(response.result, "ok");
        });
    });
}

/// RED 108734 reconnect leg: the failing e2e exchange is the FIRST request on
/// a FRESH post-rejoin connection, not a second request on a live one. In
/// `saved_draft_reopens_in_new_cell_and_reaches_play` the old editor leaves,
/// a new cell connects, floor replicates, Play is reached, and then the
/// publish request is accepted client-side but never serialized: naia
/// counters showed `cli_queued` 8->9 with `req_written` frozen, 200 ticks,
/// panic. Mirror that here: a first ordered exchange succeeds on the old
/// connection, the same client leaves and reconnects, and the first ordered
/// request on the fresh connection must serialize and be answered within the
/// e2e's exact 200-tick bound.
#[test]
fn request_after_same_client_reconnect_completes_within_200_ticks() {
    let mut scenario = Scenario::new(naia_server::ServerMode::Resident);
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());
    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.create_room().key()));

    let auth = Auth::new("client_a", "pass");
    let client_key = client_connect(
        &mut scenario,
        &room_key,
        "Client A",
        auth.clone(),
        test_client_config(),
        test_protocol,
    );

    // First exchange succeeds on the old connection (create-shape).
    roundtrip_once(&mut scenario, client_key);

    // Dirty the old leg like the e2e's retransmit storm: flood several
    // requests and tear down immediately, leaving real unacked in-flight
    // backlog behind (the e2e showed 29 writes for 5 requests with one
    // envelope never acked at rejoin).
    for i in 0..6 {
        let query = format!("flood-{i}");
        scenario.mutate(|ctx| {
            ctx.client(client_key, |client| {
                client
                    .send_request::<OrderedChannel, TestRequest>(&TestRequest::new(query.as_str()))
                    .expect("flood request must be accepted by reliable channel");
            });
        });
    }

    // Old editor leaves: SERVER-initiated disconnect (cell teardown drops the
    // session user server-side, like the e2e's "old editor leave"), both sides
    // agree. This exercises the server-drop reconnect path, where a stale
    // disconnect flag could poison the fresh connection after the request is
    // already queued on it.
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            assert!(server.disconnect_user(&client_key));
        });
    });
    scenario.expect(|ctx| server_and_client_disconnected(ctx, client_key));

    // Same client reconnects to the new cell.
    scenario.client_reconnect(client_key);
    scenario.expect(|ctx| {
        ctx.server(|server| {
            if let Some((incoming_client_key, incoming_auth)) =
                server.read_event::<naia_test_harness::ServerAuthEvent<Auth>>()
            {
                if incoming_client_key == client_key && incoming_auth == auth {
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
    scenario.expect(|ctx| {
        ctx.server(|server| {
            if let Some(incoming_client_key) =
                server.read_event::<naia_test_harness::ServerConnectEvent>()
            {
                if incoming_client_key == client_key {
                    return Some(());
                }
            }
            None
        })
    });
    scenario.expect(|ctx| server_and_client_connected(ctx, client_key));

    // Publish-shape: first ordered request on the fresh connection. The
    // server answers immediately; the client must observe within 200 ticks.
    let response_key = scenario.mutate(|ctx| {
        ctx.client(client_key, |client| {
            client
                .send_request::<OrderedChannel, TestRequest>(&TestRequest::new("post-rejoin"))
                .expect("post-rejoin request must be accepted by reliable channel")
        })
    });

    let response_send_key = scenario.until(200.ticks()).expect_msg(
        "reconnect.t1: server receives post-rejoin request within 200 ticks",
        |ctx| {
            ctx.server(|server| {
                for (recv_client_key, resp_id, _request) in
                    server.read_request::<OrderedChannel, TestRequest>()
                {
                    if recv_client_key == client_key {
                        return Some(naia_shared::ResponseSendKey::new(resp_id));
                    }
                }
                None
            })
        },
    );
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.send_response(&response_send_key, &TestResponse::new("ok"));
        });
    });
    scenario.until(200.ticks()).expect_msg(
        "reconnect.t2: client receives post-rejoin response within 200 ticks",
        |ctx| {
            ctx.client(client_key, |client| {
                client.has_response(&response_key).then_some(())
            })
        },
    );
}
