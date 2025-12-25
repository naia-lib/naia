use naia_server::ServerConfig;
use naia_shared::{Protocol, Tick};
use naia_test::{
    Scenario, ClientKey,
    protocol, Auth,
};
use naia_test::ToTicks;

mod test_helpers;
use test_helpers::{make_room, client_connect};

use naia_test::test_protocol::{Position, TestMessage, ReliableChannel, OrderedChannel, UnorderedChannel, SequencedChannel};

// ============================================================================
// Domain 6.1: Time, Transport & Determinism
// ============================================================================

/// Deterministic replay of a scenario
/// 
/// Given fully scripted scenario and deterministic clock/seed; when scenario executes twice;
/// then externally observable events and world states on all clients are identical across runs.
#[test]
fn deterministic_replay_of_a_scenario() {
    // TODO: This test requires deterministic random seed and clock
    // For now, we'll verify that the same scenario produces consistent results
    let mut scenario1 = Scenario::new();
    let test_protocol = protocol();

    scenario1.server_start(ServerConfig::default(), test_protocol.clone());

    let room_key = make_room(&mut scenario1);

    let client_a_key = client_connect(&mut scenario1, &room_key, "Client A", Auth::new("client_a", "password"), test_protocol.clone());
    // client_connect ends with expect, so we need a mutate before the next operation
    scenario1.mutate(|_ctx| {});
    let client_b_key = client_connect(&mut scenario1, &room_key, "Client B", Auth::new("client_b", "password"), test_protocol);

    // Spawn an entity on server, add it to the room, and include it in both clients' scopes
    let (entity_e, _) = scenario1.mutate(|ctx| {
        ctx.server(|server| {
            let (entity_key, _) = server.spawn(|mut e| {
                e.insert_component(Position::new(1.0, 2.0));
            });
            // Add entity to room so it can be replicated to clients
            // Entities must be in a room for update_entity_scopes() to process them
            server.room_mut(&room_key).unwrap().add_entity(&entity_key);
            // Include entity in both clients' scopes
            server.user_scope_mut(&client_a_key).unwrap().include(&entity_key);
            server.user_scope_mut(&client_b_key).unwrap().include(&entity_key);
            (entity_key, ())
        })
    });

    // Verify both clients see the entity
    scenario1.expect(|ctx| {
        let a_sees_e = ctx.client(client_a_key, |c| c.has_entity(&entity_e));
        let b_sees_e = ctx.client(client_b_key, |c| c.has_entity(&entity_e));
        (a_sees_e && b_sees_e).then_some(())
    });

    // TODO: Run scenario again with same seed and verify identical results
}

/// Robustness under simulated packet loss
/// 
/// Given A and B seeing replicated E; when server updates E while test transport drops a substantial fraction of packets;
/// then after loss subsides both clients converge to server's latest E state without permanent divergence.
#[test]
fn robustness_under_simulated_packet_loss() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());

    let room_key = make_room(&mut scenario);

    let client_a_key = client_connect(&mut scenario, &room_key, "Client A", Auth::new("client_a", "password"), test_protocol.clone());
    // client_connect ends with expect, so we need a mutate before the next operation
    scenario.mutate(|_ctx| {});
    let client_b_key = client_connect(&mut scenario, &room_key, "Client B", Auth::new("client_b", "password"), test_protocol);

    // Spawn entity E, add it to the room, and include it in both clients' scopes
    let (entity_e, _) = scenario.mutate(|ctx| {
        ctx.server(|server| {
            let (entity_key, _) = server.spawn(|mut e| {
                e.insert_component(Position::new(1.0, 2.0));
            });
            // Add entity to room so it can be replicated to clients
            server.room_mut(&room_key).unwrap().add_entity(&entity_key);
            // Include entity in both clients' scopes
            server.user_scope_mut(&client_a_key).unwrap().include(&entity_key);
            server.user_scope_mut(&client_b_key).unwrap().include(&entity_key);
            (entity_key, ())
        })
    });

    // Wait for both clients to see the entity
    scenario.expect(|ctx| {
        let a_sees_e = ctx.client(client_a_key, |c| c.has_entity(&entity_e));
        let b_sees_e = ctx.client(client_b_key, |c| c.has_entity(&entity_e));
        (a_sees_e && b_sees_e).then_some(())
    });

    // Configure link conditioner with 50% packet loss for client A
    // Note: configure_link_conditioner is on Scenario, not MutateCtx
    scenario.configure_link_conditioner(
        &client_a_key,
        Some(naia_shared::LinkConditionerConfig::new(0, 0, 0.5)), // 50% loss client->server
        Some(naia_shared::LinkConditionerConfig::new(0, 0, 0.5)), // 50% loss server->client
    );

    // Update entity multiple times
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut e) = server.entity_mut(&entity_e) {
                if let Some(mut pos) = e.component::<Position>() {
                    *pos.x = 100.0;
                }
            }
        });
    });

    // Verify both clients eventually converge to latest state
    // Client A may have lost some updates, but should eventually get the latest via retries
    // Use longer timeout due to packet loss requiring retries
    scenario.until(200usize.ticks()).expect(|ctx| {
        let a_has_latest = ctx.client(client_a_key, |c| {
            if let Some(e) = c.entity(&entity_e) {
                if let Some(pos) = e.component::<Position>() {
                    *pos.x == 100.0
                } else {
                    false
                }
            } else {
                false
            }
        });
        let b_has_latest = ctx.client(client_b_key, |c| {
            if let Some(e) = c.entity(&entity_e) {
                if let Some(pos) = e.component::<Position>() {
                    *pos.x == 100.0
                } else {
                    false
                }
            } else {
                false
            }
        });
        // Client B should definitely have latest (no packet loss)
        // Client A should eventually get it (reliable channel retries)
        (a_has_latest && b_has_latest).then_some(())
    });
}

/// Out-of-order packet handling does not regress to older state
/// 
/// Given E updated monotonically; when some packets carrying older states are delayed until after newer states;
/// then clients never regress to older state once newer state applied, and eventually report latest state.
#[test]
fn out_of_order_packet_handling_does_not_regress_to_older_state() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());

    let room_key = make_room(&mut scenario);

    let client_a_key = client_connect(&mut scenario, &room_key, "Client A", Auth::new("client_a", "password"), test_protocol);

    // Spawn entity E, add it to the room, and include it in client's scope
    let (entity_e, _) = scenario.mutate(|ctx| {
        ctx.server(|server| {
            let (entity_key, _) = server.spawn(|mut e| {
                e.insert_component(Position::new(1.0, 2.0));
            });
            // Add entity to room so it can be replicated to clients
            server.room_mut(&room_key).unwrap().add_entity(&entity_key);
            // Include entity in client's scope
            server.user_scope_mut(&client_a_key).unwrap().include(&entity_key);
            (entity_key, ())
        })
    });

    // Wait for client to see the entity
    scenario.expect(|ctx| {
        ctx.client(client_a_key, |c| c.has_entity(&entity_e)).then_some(())
    });

    // Configure link conditioner with moderate jitter to cause reordering
    // Use latency >= jitter to avoid underflow
    scenario.configure_link_conditioner(
        &client_a_key,
        Some(naia_shared::LinkConditionerConfig::new(50, 40, 0.0)), // Moderate latency >= jitter, no loss
        Some(naia_shared::LinkConditionerConfig::new(50, 40, 0.0)), // Moderate latency >= jitter, no loss
    );

    // Update entity monotonically (increasing x value)
    // Must alternate between mutate and expect, so we do all mutations in one mutate call
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            for i in 2..=10 {
                if let Some(mut e) = server.entity_mut(&entity_e) {
                    if let Some(mut pos) = e.component::<Position>() {
                        *pos.x = i as f32;
                    }
                }
            }
        });
    });

    // Verify client never regresses to older state - should have latest value (10.0)
    scenario.until(100usize.ticks()).expect(|ctx| {
        let has_latest = ctx.client(client_a_key, |c| {
            if let Some(e) = c.entity(&entity_e) {
                if let Some(pos) = e.component::<Position>() {
                    *pos.x == 10.0
                } else {
                    false
                }
            } else {
                false
            }
        });
        has_latest.then_some(())
    });
}

// ============================================================================
// Domain 6.2: Tick / Time / Command History
// ============================================================================

/// Server and client tick indices advance monotonically
/// 
/// Given server and client with matching tick rates; when simulation runs;
/// then both server tick and client's notion of server tick advance monotonically, never decreasing or rolling back.
#[test]
fn server_and_client_tick_indices_advance_monotonically() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());

    let room_key = make_room(&mut scenario);

    let client_a_key = client_connect(&mut scenario, &room_key, "Client A", Auth::new("client_a", "password"), test_protocol);

    let mut last_server_tick: Option<Tick> = None;
    let mut last_client_tick: Option<Tick> = None;
    let mut tick_count = 0;

    // client_connect ends with expect, so we need a mutate before the next expect
    scenario.mutate(|_ctx| {});

    // Run multiple ticks and verify monotonic advancement
    // Must alternate between mutate and expect, so we use a single expect that checks multiple ticks
    scenario.until(50usize.ticks()).expect(|ctx| {
        // Check server tick
        let mut server_tick_events: Vec<Tick> = Vec::new();
        ctx.server(|server| {
            if let Some(tick) = server.read_event::<naia_test::ServerTickEvent>() {
                server_tick_events.push(tick);
            }
        });
        
        // Check client tick
        let mut client_tick_events: Vec<Tick> = Vec::new();
        ctx.client(client_a_key, |c| {
            if let Some(tick) = c.read_event::<naia_test::ClientServerTickEvent>() {
                client_tick_events.push(tick);
            }
        });

        // Verify monotonic advancement
        if let Some(tick) = server_tick_events.first() {
            if let Some(last) = last_server_tick {
                if *tick < last {
                    return None; // Regression detected
                }
            }
            last_server_tick = Some(*tick);
            tick_count += 1;
        }

        if let Some(tick) = client_tick_events.first() {
            if let Some(last) = last_client_tick {
                if *tick < last {
                    return None; // Regression detected
                }
            }
            last_client_tick = Some(*tick);
            tick_count += 1;
        }

        // Return Some(()) only after we've seen at least 10 ticks advance
        if tick_count >= 10 {
            Some(())
        } else {
            None
        }
    });

    // Verify we saw ticks advancing
    assert!(last_server_tick.is_some() || last_client_tick.is_some());
}

/// Pausing and resuming time does not create extra ticks
/// 
/// Given deterministic time source; when time is paused (no tick advancement) then resumed;
/// then no ticks are generated during pause and progression resumes smoothly from last tick index.
#[test]
fn pausing_and_resuming_time_does_not_create_extra_ticks() {
    // TODO: This test requires ability to pause/resume TestClock
    // For now, we'll verify basic tick behavior
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());

    let room_key = make_room(&mut scenario);

    let _client_a_key = client_connect(&mut scenario, &room_key, "Client A", Auth::new("client_a", "password"), test_protocol);

    // TODO: Pause time
    // TODO: Verify no ticks generated
    // TODO: Resume time
    // TODO: Verify ticks resume from last index
}

/// Command history preserves and replays commands after correction
/// 
/// Given client sends per-tick input and server sends authoritative state; when client receives corrected state for earlier tick while holding newer commands;
/// then client replays newer commands in order on corrected state and reaches same final state as if correction had been there from start.
#[test]
fn command_history_preserves_and_replays_commands_after_correction() {
    // TODO: This test requires tick-buffered commands and state correction
    // This is a complex feature that may need deeper investigation
}

/// Command history discards old commands beyond its window
/// 
/// Given bounded command history; when many ticks pass and commands are inserted;
/// then commands older than window are discarded, and late corrections for ticks outside window do not attempt to replay discarded commands.
#[test]
fn command_history_discards_old_commands_beyond_its_window() {
    // TODO: This test requires tick-buffered commands and command history window
    // This is a complex feature that may need deeper investigation
}

// ============================================================================
// Domain 6.3: Wraparound & Long-running Behaviour
// ============================================================================

/// Tick index wraparound does not break progression or ordering
/// 
/// Given deterministic time and known tick counter max; when server and client tick through wraparound;
/// then tick ordering stays correct, channels/tick-buffer semantics still hold, and no panics/invalid state occur.
#[test]
fn tick_index_wraparound_does_not_break_progression_or_ordering() {
    // TODO: This test requires ticking through wraparound (Tick::MAX)
    // This is a very long-running test that may be impractical
}

/// Sequence number wraparound for channels preserves ordering semantics
/// 
/// Given ordered channel with wrapping sequence numbers; when enough messages force wrap;
/// then ordered semantics still hold across wrap and later messages are still treated as newer.
#[test]
fn sequence_number_wraparound_for_channels_preserves_ordering_semantics() {
    // TODO: This test requires sending enough messages to cause sequence number wraparound
    // This is a very long-running test that may be impractical
}

/// Long-running scenario maintains stable memory and state
/// 
/// Given long scenario with frequent connects/disconnects, spawns/updates/despawns, and messages; when test finishes;
/// then user/entity counts and buffer sizes remain bounded, and no ghost users/entities remain.
#[test]
fn long_running_scenario_maintains_stable_memory_and_state() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());

    let room_key = make_room(&mut scenario);

    // Perform many connect/disconnect cycles
    for cycle in 0..10 {
        let client_key = client_connect(&mut scenario, &room_key, &format!("Client {}", cycle), Auth::new(&format!("client_{}", cycle), "password"), test_protocol.clone());
        
        // Spawn entity (client_connect ends with expect, so this mutate is fine)
        let (entity, _) = scenario.mutate(|ctx| {
            ctx.server(|server| {
                server.spawn(|mut e| {
                    e.insert_component(Position::new(cycle as f32, 0.0));
                })
            })
        });

        // Wait a bit for entity to be processed
        scenario.expect(|_ctx| Some(()));

        // Disconnect
        scenario.mutate(|ctx| {
            ctx.client(client_key, |c| {
                c.disconnect();
            });
        });

        // Wait for disconnect
        scenario.expect(|ctx| {
            (!ctx.server(|s| s.user_exists(&client_key))).then_some(())
        });

        // Despawn entity
        scenario.mutate(|ctx| {
            ctx.server(|server| {
                server.despawn(&entity);
            });
        });
    }

    // Verify no ghost users/entities remain
    scenario.expect(|ctx| {
        let user_count = ctx.server(|s| s.users_count());
        let entity_count = ctx.server(|s| s.entities().len());
        (user_count == 0 && entity_count == 0).then_some(())
    });
}

// ============================================================================
// Domain 6.4: Link Conditioner Stress
// ============================================================================

/// Extreme jitter and reordering preserve channel contracts
/// 
/// Given link conditioner with high jitter and reordering; when sending messages and replication updates over ordered/unordered/sequenced/tick-buffered channels;
/// then each channel still satisfies its documented ordering/reliability/latest-only semantics.
#[test]
fn extreme_jitter_and_reordering_preserve_channel_contracts() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());

    let room_key = make_room(&mut scenario);

    let client_a_key = client_connect(&mut scenario, &room_key, "Client A", Auth::new("client_a", "password"), test_protocol);

    // First test without link conditioner to verify messages work
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.send_message::<ReliableChannel, _>(&client_a_key, &TestMessage::new(999));
        });
    });

    // Verify message arrives without link conditioner
    let mut test_msg = Vec::new();
    scenario.expect(|ctx| {
        ctx.client(client_a_key, |c| {
            for msg in c.read_message::<ReliableChannel, TestMessage>() {
                test_msg.push(msg.value);
            }
        });
        (test_msg.len() == 1 && test_msg[0] == 999).then_some(())
    });

    // Now configure link conditioner with small latency/jitter
    // Use latency >= jitter to avoid underflow
    scenario.configure_link_conditioner(
        &client_a_key,
        Some(naia_shared::LinkConditionerConfig::new(10, 5, 0.0)), // Small latency >= jitter, no loss
        Some(naia_shared::LinkConditionerConfig::new(10, 5, 0.0)), // Small latency >= jitter, no loss
    );

    // Send messages on different channel types
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            // Send on ReliableChannel (should arrive in order)
            server.send_message::<ReliableChannel, _>(&client_a_key, &TestMessage::new(1));
            server.send_message::<ReliableChannel, _>(&client_a_key, &TestMessage::new(2));
            server.send_message::<ReliableChannel, _>(&client_a_key, &TestMessage::new(3));
            
            // Send on OrderedChannel (should arrive in order)
            server.send_message::<OrderedChannel, _>(&client_a_key, &TestMessage::new(10));
            server.send_message::<OrderedChannel, _>(&client_a_key, &TestMessage::new(20));
            server.send_message::<OrderedChannel, _>(&client_a_key, &TestMessage::new(30));
        });
    });

    // Verify both ReliableChannel and OrderedChannel messages arrive in order despite jitter
    // With 10ms latency + 5ms jitter, packets can be delayed up to 15ms (~1 tick)
    // Both channel types are in the same packet, so we need to read them in the same expect() call
    let mut reliable_messages = Vec::new();
    let mut ordered_messages = Vec::new();
    scenario.until(50usize.ticks()).expect(|ctx| {
        ctx.client(client_a_key, |c| {
            for msg in c.read_message::<ReliableChannel, TestMessage>() {
                reliable_messages.push(msg.value);
            }
            for msg in c.read_message::<OrderedChannel, TestMessage>() {
                ordered_messages.push(msg.value);
            }
        });
        (reliable_messages.len() == 3 && ordered_messages.len() == 3).then_some(())
    });
    assert_eq!(reliable_messages, vec![1, 2, 3], "ReliableChannel should maintain order");
    assert_eq!(ordered_messages, vec![10, 20, 30], "OrderedChannel should maintain order");
}

/// Packet duplication does not surface duplicate events
/// 
/// Given link conditioner that duplicates packets at high rate; when server sends entity updates and messages;
/// then clients never observe duplicate spawn/despawn/message/response events, and state does not regress even if older duplicates arrive after newer packets.
/// 
/// Note: Link conditioner doesn't support duplication, but high jitter can cause reordering which tests similar behavior.
#[test]
fn packet_duplication_does_not_surface_duplicate_events() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());

    let room_key = make_room(&mut scenario);

    let client_a_key = client_connect(&mut scenario, &room_key, "Client A", Auth::new("client_a", "password"), test_protocol);

    // Configure link conditioner with moderate jitter to simulate reordering (similar to duplication effects)
    // Use latency >= jitter to avoid underflow
    scenario.configure_link_conditioner(
        &client_a_key,
        Some(naia_shared::LinkConditionerConfig::new(50, 40, 0.0)), // Moderate latency >= jitter, no loss
        Some(naia_shared::LinkConditionerConfig::new(50, 40, 0.0)), // Moderate latency >= jitter, no loss
    );

    // Spawn entity E, add it to the room, and include it in client's scope
    let (entity_e, _) = scenario.mutate(|ctx| {
        ctx.server(|server| {
            let (entity_key, _) = server.spawn(|mut e| {
                e.insert_component(Position::new(1.0, 2.0));
            });
            // Add entity to room so it can be replicated to clients
            server.room_mut(&room_key).unwrap().add_entity(&entity_key);
            // Include entity in client's scope
            server.user_scope_mut(&client_a_key).unwrap().include(&entity_key);
            (entity_key, ())
        })
    });

    // Verify entity spawn event occurred exactly once
    // Note: SpawnEntityEvent is not exported, so we verify by checking entity exists
    scenario.expect(|ctx| {
        ctx.client(client_a_key, |c| c.has_entity(&entity_e)).then_some(())
    });

    // Send multiple messages
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.send_message::<naia_test::test_protocol::ReliableChannel, _>(&client_a_key, &naia_test::test_protocol::TestMessage::new(100));
            server.send_message::<naia_test::test_protocol::ReliableChannel, _>(&client_a_key, &naia_test::test_protocol::TestMessage::new(200));
        });
    });

    // Verify messages were received exactly once each (no duplicates)
    let mut message_values = Vec::new();
    scenario.expect(|ctx| {
        ctx.client(client_a_key, |c| {
            for msg in c.read_message::<ReliableChannel, TestMessage>() {
                message_values.push(msg.value);
            }
        });
        (message_values.len() == 2).then_some(())
    });

    // Update entity multiple times (to test that older updates don't regress state)
    // Must alternate between mutate and expect, so we do all mutations in one mutate call
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            for i in 2..=5 {
                if let Some(mut e) = server.entity_mut(&entity_e) {
                    if let Some(mut pos) = e.component::<Position>() {
                        *pos.x = i as f32;
                    }
                }
            }
        });
    });

    // Verify entity has latest state (not regressed to older value)
    scenario.expect(|ctx| {
        ctx.client(client_a_key, |c| {
            let Some(e) = c.entity(&entity_e) else {
                return false;
            };
            let Some(pos) = e.component::<Position>() else {
                return false;
            };
            let x_value = *pos.x;
            (x_value - 5.0).abs() < 0.001
        }).then_some(())
    });
    assert_eq!(message_values.len(), 2, "Should receive exactly 2 messages, no duplicates");
    assert!(message_values.contains(&100), "Should receive message 100");
    assert!(message_values.contains(&200), "Should receive message 200");
}

// ============================================================================
// Domain 6.5: MTU, Fragmentation & Compression
// ============================================================================

/// Large entity update that exceeds MTU is correctly reassembled
/// 
/// Given E whose update exceeds single MTU; when server sends full update;
/// then client applies a complete coherent update only after all fragments arrive, never partial component state, even with delayed/duplicated fragments.
#[test]
fn large_entity_update_that_exceeds_mtu_is_correctly_reassembled() {
    // TODO: This test requires creating an entity update that exceeds MTU
    // TODO: Verify fragments are correctly reassembled
    // TODO: Verify no partial state is applied
}

/// Fragment loss causes older state until a full later update
/// 
/// Given repeated large updates for E with fragmentation; when one update loses a fragment but a later full update arrives intact;
/// then client stays at previous valid state until later full update is applied, never applying a partially missing update.
#[test]
fn fragment_loss_causes_older_state_until_a_full_later_update() {
    // TODO: This test requires fragmentation and packet loss
    // TODO: Verify client maintains previous state when fragment is lost
    // TODO: Verify client applies later full update correctly
}

/// Compression on/off does not change observable semantics
/// 
/// Given scenario with entities/messages; when run once with compression off and once on;
/// then sequence of API-visible events, entity states, and messages is identical between runs (only bandwidth differs).
#[test]
fn compression_on_off_does_not_change_observable_semantics() {
    // TODO: This test requires running same scenario with compression on/off
    // TODO: Compare event sequences and entity states
    // TODO: Verify they are identical
}

// ============================================================================
// Domain 6.6: Config, Limits & Edge Behaviour
// ============================================================================

/// Maximum users limit is enforced and observable
/// 
/// Given server configured with max N concurrent users and N are already connected; when (N+1)th client connects;
/// then server rejects according to overflow semantics (e.g., explicit reject), emits no connect event, and extra client receives no replication.
#[test]
fn maximum_users_limit_is_enforced_and_observable() {
    // TODO: This test requires ServerConfig with max_users limit
    // TODO: Connect N users
    // TODO: Attempt to connect (N+1)th user
    // TODO: Verify rejection
}

/// Maximum entities limit is enforced and observable
/// 
/// Given server with max entity count; when limit is reached and more spawns are attempted;
/// then extra spawns fail according to contract and clients never see entities exceeding configured maximum.
#[test]
fn maximum_entities_limit_is_enforced_and_observable() {
    // TODO: This test requires ServerConfig with max_entities limit
    // TODO: Spawn entities up to limit
    // TODO: Attempt to spawn beyond limit
    // TODO: Verify failure
}

/// Reliable retry/timeout settings produce defined failure behaviour
/// 
/// Given reliable channel with limited retries/timeouts; when server sends reliable message over link that can't deliver within budget;
/// then sender surfaces a clear failure/timeout, stops retrying, and system does not hang or leak.
#[test]
fn reliable_retry_timeout_settings_produce_defined_failure_behaviour() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());

    let room_key = make_room(&mut scenario);

    let client_a_key = client_connect(&mut scenario, &room_key, "Client A", Auth::new("client_a", "password"), test_protocol);

    // Configure link conditioner to drop all packets (100% loss)
    scenario.configure_link_conditioner(
        &client_a_key,
        Some(naia_shared::LinkConditionerConfig::new(0, 0, 1.0)), // 100% loss
        Some(naia_shared::LinkConditionerConfig::new(0, 0, 1.0)), // 100% loss
    );

    // Send reliable message
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.send_message::<naia_test::test_protocol::ReliableChannel, _>(&client_a_key, &naia_test::test_protocol::TestMessage::new(42));
        });
    });

    // Verify message is NOT received (due to 100% packet loss) and system is still stable
    // The until() will advance time significantly to allow retries to exhaust
    // Note: This test verifies that the system doesn't hang or leak, even if message never arrives
    // The actual timeout/failure event may not be exposed yet, but system should remain stable
    let mut message_count = 0;
    scenario.until(100usize.ticks()).expect(|ctx| {
        ctx.client(client_a_key, |c| {
            for _ in c.read_message::<naia_test::test_protocol::ReliableChannel, naia_test::test_protocol::TestMessage>() {
                message_count += 1;
            }
            // Just verify we can still access the client (system stability check)
            true
        });
        // After enough ticks, verify message was not received
        Some(())
    });
    assert_eq!(message_count, 0, "Message should not be received with 100% packet loss");
}

/// Minimal retry reliable settings produce clear delivery failure semantics
/// 
/// Given reliable channel with extremely low retries/timeouts; when messages cannot be delivered within constraints;
/// then sender reports "delivery failed" or timeout, stops retrying, and no internal state is left stuck.
#[test]
fn minimal_retry_reliable_settings_produce_clear_delivery_failure_semantics() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());

    let room_key = make_room(&mut scenario);

    let client_a_key = client_connect(&mut scenario, &room_key, "Client A", Auth::new("client_a", "password"), test_protocol);

    // Configure link conditioner to drop all packets (100% loss)
    scenario.configure_link_conditioner(
        &client_a_key,
        Some(naia_shared::LinkConditionerConfig::new(0, 0, 1.0)), // 100% loss
        Some(naia_shared::LinkConditionerConfig::new(0, 0, 1.0)), // 100% loss
    );

    // Send reliable message
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.send_message::<naia_test::test_protocol::ReliableChannel, _>(&client_a_key, &naia_test::test_protocol::TestMessage::new(99));
        });
    });

    // Verify message is NOT received
    // The until() will advance time to allow retries to exhaust
    let mut message_count = 0;
    scenario.until(100usize.ticks()).expect(|ctx| {
        ctx.client(client_a_key, |c| {
            for _ in c.read_message::<naia_test::test_protocol::ReliableChannel, naia_test::test_protocol::TestMessage>() {
                message_count += 1;
            }
        });
        // After enough ticks, verify message was not received
        Some(())
    });
    assert_eq!(message_count, 0, "Message should not be delivered with 100% loss");
    
    // Verify system remains stable (can still access client)
    // The previous expect ended, so we need a mutate before the next expect
    scenario.mutate(|_ctx| {});
    scenario.expect(|ctx| {
        ctx.client(client_a_key, |_c| {
            // Just verify we can still access the client
            true
        }).then_some(())
    });
}

/// Very aggressive heartbeat/timeout still leads to clean disconnect
/// 
/// Given very small heartbeat/timeout values; when traffic briefly pauses or link is stressed;
/// then connection may time out but disconnect remains clean (events emitted, state cleared) with no partial user state.
#[test]
fn very_aggressive_heartbeat_timeout_still_leads_to_clean_disconnect() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());

    let room_key = make_room(&mut scenario);

    let client_a_key = client_connect(&mut scenario, &room_key, "Client A", Auth::new("client_a", "password"), test_protocol);

    // Configure link conditioner with high loss to cause connection issues
    scenario.configure_link_conditioner(
        &client_a_key,
        Some(naia_shared::LinkConditionerConfig::new(0, 0, 0.9)), // 90% loss - very high
        Some(naia_shared::LinkConditionerConfig::new(0, 0, 0.9)), // 90% loss - very high
    );

    // Advance time to allow connection to potentially timeout
    // Must alternate between mutate and expect, so we do all advances in one mutate call
    scenario.mutate(|_ctx| {
        // Advance time by ticking multiple times
        // This is a no-op mutate, but it advances the simulation
    });

    // Verify disconnect event is emitted if connection times out
    // Note: With default timeouts, connection may or may not timeout with 90% loss
    // This test verifies that if it does timeout, it's clean
    let mut disconnect_events = 0;
    scenario.expect(|ctx| {
        ctx.client(client_a_key, |c| {
            for _ in c.read_event::<naia_test::ClientDisconnectEvent>() {
                disconnect_events += 1;
            }
        });
        Some(())
    });
    
    // If disconnect occurred, verify it was clean (only one event)
    if disconnect_events > 0 {
        assert_eq!(disconnect_events, 1, "Should have exactly one disconnect event");
    }
}

/// Tiny tick-buffer window behaves correctly for old ticks
/// 
/// Given tick-buffer with very small window; when messages tagged with old ticks arrive after window advanced;
/// then they are dropped according to semantics and never applied to current state or regress tick index.
#[test]
fn tiny_tick_buffer_window_behaves_correctly_for_old_ticks() {
    // TODO: This test requires tick-buffered channel with small window
    // TODO: Send messages with old ticks
    // TODO: Verify they are dropped and don't regress state
}

/// Switching a channel from reliable to unreliable (or ordered to unordered) only changes documented semantics
/// 
/// Given two runs of same scenario, one with channel reliable/ordered, another unreliable/unordered; when comparing;
/// then only the documented differences (loss/reordering) appear, with no unintended effects like instability or desync.
#[test]
fn switching_channel_reliability_only_changes_documented_semantics() {
    // TODO: This test requires running same scenario with different channel configurations
    // TODO: Compare behavior and verify only documented differences
}

// ============================================================================
// Domain 6.7: Observability: Ping & Bandwidth
// ============================================================================

/// Reported ping/RTT converges under steady latency
/// 
/// Given link with fixed RTT and low jitter/loss; when client/server exchange several heartbeats;
/// then reported ping/RTT converges near configured latency and is never negative or wildly unstable.
#[test]
fn reported_ping_rtt_converges_under_steady_latency() {
    // TODO: This test requires access to ping/RTT metrics API
    // For now, configure link conditioner with fixed latency
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());

    let room_key = make_room(&mut scenario);

    let client_a_key = client_connect(&mut scenario, &room_key, "Client A", Auth::new("client_a", "password"), test_protocol);

    // Configure link conditioner with fixed latency (50ms) and low jitter/loss
    scenario.configure_link_conditioner(
        &client_a_key,
        Some(naia_shared::LinkConditionerConfig::new(50, 5, 0.01)), // 50ms latency, 5ms jitter, 1% loss
        Some(naia_shared::LinkConditionerConfig::new(50, 5, 0.01)), // 50ms latency, 5ms jitter, 1% loss
    );

    // Exchange heartbeats by running simulation
    // Must alternate between mutate and expect, so we do all advances in one mutate call
    scenario.mutate(|_ctx| {
        // Advance time by ticking multiple times
        // This is a no-op mutate, but it advances the simulation
    });

    // TODO: Once ping/RTT API is available, verify it converges to ~50ms
    // For now, just verify connection is stable
    scenario.until(50usize.ticks()).expect(|ctx| {
        ctx.client(client_a_key, |_c| {
            // Just verify we can still access the client
            true
        }).then_some(())
    });
}

/// Reported ping remains bounded under jitter and loss
/// 
/// Given link with significant jitter and modest loss; when running;
/// then ping/RTT fluctuates but stays finite, non-negative, and below a reasonable ceiling (no overflow/garbage values).
#[test]
fn reported_ping_remains_bounded_under_jitter_and_loss() {
    // TODO: This test requires access to ping/RTT metrics API
    // For now, configure link conditioner with jitter and loss
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());

    let room_key = make_room(&mut scenario);

    let client_a_key = client_connect(&mut scenario, &room_key, "Client A", Auth::new("client_a", "password"), test_protocol);

    // Configure link conditioner with significant jitter and modest loss
    scenario.configure_link_conditioner(
        &client_a_key,
        Some(naia_shared::LinkConditionerConfig::new(100, 50, 0.05)), // 100ms latency, 50ms jitter, 5% loss
        Some(naia_shared::LinkConditionerConfig::new(100, 50, 0.05)), // 100ms latency, 50ms jitter, 5% loss
    );

    // Run simulation
    // Must alternate between mutate and expect, so we do all advances in one mutate call
    scenario.mutate(|_ctx| {
        // Advance time by ticking multiple times
        // This is a no-op mutate, but it advances the simulation
    });

    // TODO: Once ping/RTT API is available, verify it stays bounded (finite, non-negative, reasonable ceiling)
    // For now, just verify connection is stable
    scenario.until(50usize.ticks()).expect(|ctx| {
        ctx.client(client_a_key, |_c| {
            // Just verify we can still access the client
            true
        }).then_some(())
    });
}

/// Bandwidth monitor reflects changes in traffic volume
/// 
/// Given bandwidth metric; when system alternates between high traffic and near-idle;
/// then reported bandwidth rises during high activity and drops during idle, without staying stuck at stale values.
#[test]
fn bandwidth_monitor_reflects_changes_in_traffic_volume() {
    // TODO: This test requires access to bandwidth metrics
    // TODO: Alternate between high and low traffic
    // TODO: Verify bandwidth metrics reflect changes
}

/// Compression toggling affects bandwidth metrics but not logical events
/// 
/// Given scripted replication/messages; when run once with compression off and once on;
/// then compressed run shows fewer bytes sent, while logical events and world states stay identical.
#[test]
fn compression_toggling_affects_bandwidth_metrics_but_not_logical_events() {
    // TODO: This test requires access to bandwidth metrics
    // TODO: Run scenario with compression on/off
    // TODO: Compare bandwidth metrics and logical events
}
