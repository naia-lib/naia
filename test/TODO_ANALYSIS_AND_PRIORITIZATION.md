# E2E Test TODO Analysis & Prioritization Report

## Executive Summary

**Total TODOs**: 37 test scenarios marked as TODO or partial implementation  
**Impact**: These TODOs block completion of Domains 5-9 (approximately 28% of all tests)  
**Estimated Effort**: High-value quick wins available; some require deeper infrastructure work

---

## TODO Categories & Blocking Issues

### Category 1: Events API Direct Access (NOT E2E - Should be Unit Tests)

**Tests Affected**: 4 tests in Domain 8.1
- `inserts_updates_removes_are_one_shot_and_non_duplicated`
- `component_update_events_reflect_correct_multiplicity_per_user`
- `message_events_grouped_correctly_by_channel_and_type` (partial)
- `request_response_events_via_events_api_are_drained_and_do_not_reappear` (partial)

**Classification**: These tests verify the **test harness** (`ServerEvents` and `ClientEvents`) behavior, not Naia's actual behavior. They should be **unit tests** for the test harness, not E2E tests.

**Recommendation**: 
- Remove these from E2E test suite
- Create unit tests in `test/src/harness/` to verify `ServerEvents` and `ClientEvents` correctly drain and don't duplicate
- E2E tests should focus on verifying Naia's contractual behavior, not the test harness implementation

**Status**: **REMOVED FROM E2E PRIORITIES** - Should be handled as separate unit test suite

---

### Category 2: Link Conditioner Configuration (High Impact, Medium Complexity)

**Tests Affected**: 7 tests across Domains 6.1, 6.4
- `robustness_under_simulated_packet_loss`
- `out_of_order_packet_handling_does_not_regress_to_older_state`
- `extreme_jitter_and_reordering_preserve_channel_contracts`
- `packet_duplication_does_not_surface_duplicate_events`
- Plus several others that need packet loss/jitter

**What's Needed**:
- Ability to configure `LinkConditionerConfig` in test harness
- Pass link conditioner config to `LocalTransportHub` or connection setup
- Configure packet loss, jitter, reordering rates

**Current State**:
- ✅ `LinkConditionerConfig` exists in `socket/shared/src/link_conditioner_config.rs`
- ✅ Has methods like `average_condition()`, `poor_condition()`, etc.
- ✅ Can create custom configs with `new(incoming_latency, incoming_jitter, incoming_loss)`
- ❌ `LocalTransportHub` doesn't expose link conditioner configuration
- ❌ Test harness doesn't pass link conditioner to connections

**Blocking Issue**: The `LocalTransportHub` used in tests doesn't support link conditioning. The real socket implementations support it via `SocketConfig`, but the test harness uses a simplified local transport.

**Solution**:
1. Extend `LocalTransportHub` to accept optional `LinkConditionerConfig`
2. Apply link conditioning in `LocalTransportHub` packet routing
3. Add method to `Scenario` to configure link conditioner per-client or globally
4. Example API:
   ```rust
   scenario.configure_link_conditioner(&client_a_key, LinkConditionerConfig::new(0, 0, 0.5)); // 50% loss
   ```

**Estimated Effort**: 4-6 hours  
**Priority**: **HIGH** (unblocks 7+ tests, critical for transport testing)

---

### Category 3: Tick-Buffered Channel API (Medium Impact, Low Complexity)

**Tests Affected**: 2 tests in Domain 5.2
- `tick_buffered_channel_groups_messages_by_tick`
- `tick_buffered_channel_discards_messages_for_ticks_that_are_too_old`

**What's Needed**:
- API to send tick-buffered messages from client/server
- API to receive tick-buffered messages grouped by tick
- Ability to verify tick ordering and windowing

**Current State**:
- ✅ `TickBufferedChannel` exists in test protocol
- ✅ Test already has partial implementation using `send_tick_buffer_message`
- ❌ `send_tick_buffer_message` method doesn't exist on `ClientMutateCtx` or `ServerMutateCtx`
- ❌ No API to read tick-buffered messages grouped by tick

**Blocking Issue**: The test harness doesn't expose tick-buffered message sending/receiving APIs.

**Solution**:
1. Add `send_tick_buffer_message<C, M>(&self, tick: &Tick, message: &M)` to `ClientMutateCtx` and `ServerMutateCtx`
2. Add `read_tick_buffer_messages<C, M>(&mut self) -> HashMap<Tick, Vec<M>>` to `ClientExpectCtx` and `ServerExpectCtx`
3. Wire through to underlying Naia client/server tick-buffered message APIs

**Estimated Effort**: 2-3 hours  
**Priority**: **MEDIUM** (unblocks 2 tests, relatively straightforward)

---

### Category 4: Server/Client Configuration Limits (Medium Impact, Low Complexity)

**Tests Affected**: 2 tests in Domain 6.6
- `maximum_users_limit_is_enforced_and_observable`
- `maximum_entities_limit_is_enforced_and_observable`

**What's Needed**:
- `ServerConfig` fields for `max_users` and `max_entities`
- Server enforcement of these limits
- Tests to verify rejection when limits exceeded

**Current State**:
- ✅ `ServerConfig` exists and is configurable
- ❌ No `max_users` or `max_entities` fields in `ServerConfig`
- ❌ Server doesn't enforce these limits

**Blocking Issue**: These limits don't exist in the current `ServerConfig` API. This requires adding new fields and server-side enforcement logic.

**Solution**:
1. Add `max_users: Option<usize>` and `max_entities: Option<usize>` to `ServerConfig`
2. Implement limit checking in server connection logic (for max_users)
3. Implement limit checking in server spawn logic (for max_entities)
4. Return appropriate errors when limits exceeded

**Estimated Effort**: 3-4 hours (requires server code changes)  
**Priority**: **MEDIUM** (unblocks 2 tests, but requires server changes)

---

### Category 5: Request Timeout & Disconnect Handling (Medium Impact, Medium Complexity)

**Tests Affected**: 2 tests in Domain 5.3
- `request_timeouts_are_surfaced_and_cleaned_up`
- `requests_fail_cleanly_on_disconnect_mid_flight`

**What's Needed**:
- API to configure request timeout duration
- API to detect request timeout events
- Verification that in-flight requests are cancelled on disconnect

**Current State**:
- ✅ Request/response APIs exist (`send_request`, `receive_response`)
- ❌ No timeout configuration
- ❌ No timeout event detection
- ❌ No verification of request cancellation on disconnect

**Blocking Issue**: Naia's request/response system may have timeouts internally, but they're not exposed to the test harness.

**Solution**:
1. Investigate if Naia has internal request timeout handling
2. If yes, expose timeout events to test harness
3. If no, may need to implement timeout tracking in test harness
4. Add disconnect event handling to cancel in-flight requests

**Estimated Effort**: 4-6 hours (requires investigation + implementation)  
**Priority**: **MEDIUM** (unblocks 2 tests, but may require deeper investigation)

---

### Category 6: Connection Config Timeouts & Retries (Low Impact, Medium Complexity)

**Tests Affected**: 4 tests in Domain 6.6
- `reliable_retry_timeout_settings_produce_defined_failure_behaviour`
- `minimal_retry_reliable_settings_produce_clear_delivery_failure_semantics`
- `very_aggressive_heartbeat_timeout_still_leads_to_clean_disconnect`
- `tiny_tick_buffer_window_behaves_correctly_for_old_ticks`

**What's Needed**:
- `ConnectionConfig` fields for retry limits, reliable message timeouts
- `ServerConfig`/`ClientConfig` fields for tick buffer window size
- Ability to configure aggressive timeouts

**Current State**:
- ✅ `ConnectionConfig` exists with `disconnection_timeout_duration` and `heartbeat_interval`
- ❌ No retry limit configuration
- ❌ No reliable message timeout configuration
- ❌ No tick buffer window configuration

**Blocking Issue**: These configuration options don't exist in the current API.

**Solution**:
1. Add retry/timeout configs to `ConnectionConfig` or `ServerConfig`
2. Add tick buffer window config to appropriate config struct
3. Wire through to underlying implementations

**Estimated Effort**: 4-6 hours (requires config + implementation changes)  
**Priority**: **LOW-MEDIUM** (unblocks 4 tests, but lower priority than core features)

---

### Category 7: Observability Metrics (Low Impact, High Complexity)

**Tests Affected**: 4 tests in Domain 6.7
- `reported_ping_rtt_converges_under_steady_latency`
- `reported_ping_remains_bounded_under_jitter_and_loss`
- `bandwidth_monitor_reflects_changes_in_traffic_volume`
- `compression_toggling_affects_bandwidth_metrics_but_not_logical_events`

**What's Needed**:
- API to read ping/RTT from client/server
- API to read bandwidth metrics
- Ability to toggle compression and measure bandwidth difference

**Current State**:
- ✅ `ConnectionConfig` has `bandwidth_measure_duration: Option<Duration>`
- ❌ No API to read ping/RTT values
- ❌ No API to read bandwidth metrics
- ❌ No compression toggle API

**Blocking Issue**: Metrics may be collected internally but not exposed to public API.

**Solution**:
1. Investigate if ping/RTT is tracked internally
2. Add getter methods to client/server for ping/RTT
3. Add getter methods for bandwidth metrics
4. Add compression toggle to `ConnectionConfig` or `ServerConfig`

**Estimated Effort**: 6-8 hours (requires investigation + API design)  
**Priority**: **LOW** (unblocks 4 tests, but observability is nice-to-have)

---

### Category 8: Protocol Mismatch Testing (Low Impact, High Complexity)

**Tests Affected**: 5 tests in Domain 7
- `serialization_failures_are_surfaced_without_poisoning_the_connection`
- `protocol_type_order_mismatch_fails_fast_at_handshake`
- `client_missing_a_type_that_the_server_uses`
- `safe_extension_server_knows_extra_type_but_still_interoperates`
- `schema_incompatibility_produces_immediate_clear_failure`

**What's Needed**:
- Ability to create intentionally mismatched protocols
- Force serialization failures
- Test handshake rejection for protocol mismatches

**Current State**:
- ✅ Protocol builder exists
- ❌ No way to create mismatched protocols easily
- ❌ No way to force serialization failures
- ❌ Protocol mismatch detection may already work, but needs testing

**Blocking Issue**: These tests require creating "broken" protocols, which may not be straightforward with current API.

**Solution**:
1. Create helper functions to build mismatched protocols (different type orders, missing types)
2. Create a message/component type that can fail serialization on demand
3. Test that handshake properly rejects mismatches

**Estimated Effort**: 6-8 hours (requires creative test setup)  
**Priority**: **LOW** (unblocks 5 tests, but protocol mismatch is edge case)

---

### Category 9: Transport Comparison (Low Impact, Very High Complexity)

**Tests Affected**: 2 tests in Domain 9
- `core_replication_scenario_behaves_identically_over_udp_and_webrtc`
- `transport_specific_connection_failure_surfaces_cleanly`

**What's Needed**:
- Run same scenario over UDP transport
- Run same scenario over WebRTC transport
- Compare event sequences
- Test WebRTC-specific failures (ICE/signaling)

**Current State**:
- ✅ `LocalTransportHub` exists for testing
- ❌ No way to run tests over real UDP/WebRTC
- ❌ Would require significant infrastructure changes

**Blocking Issue**: This requires running tests over real network transports, which is complex and may not be appropriate for unit tests.

**Solution**:
1. Consider if these should be integration tests, not unit tests
2. May need separate test infrastructure for transport testing
3. Could mock transport behavior instead

**Estimated Effort**: 20+ hours (major infrastructure work)  
**Priority**: **VERY LOW** (may be better as separate integration test suite)

---

### Category 10: Command History & Tick Management (Low Impact, Medium Complexity)

**Tests Affected**: 4 tests in Domain 6.2, 6.3
- `pausing_and_resuming_time_does_not_create_extra_ticks`
- `command_history_preserves_and_replays_commands_after_correction`
- `command_history_discards_old_commands_beyond_its_window`
- `tick_index_wraparound_does_not_break_progression_or_ordering`
- `sequence_number_wraparound_for_channels_preserves_ordering_semantics`

**What's Needed**:
- API to pause/resume `TestClock`
- API to access client command history
- API to trigger tick wraparound
- API to trigger sequence number wraparound

**Current State**:
- ✅ `TestClock` exists and is used
- ❌ No pause/resume API
- ❌ No command history API exposed
- ❌ Wraparound testing requires forcing large tick/sequence values

**Blocking Issue**: These features may exist internally but aren't exposed for testing.

**Solution**:
1. Add `TestClock::pause()` and `TestClock::resume()` methods
2. Investigate if command history exists in client
3. Add APIs to access command history
4. Create helpers to force wraparound scenarios

**Estimated Effort**: 6-8 hours  
**Priority**: **LOW-MEDIUM** (unblocks 5 tests, but edge cases)

---

### Category 11: MTU, Fragmentation & Compression (Low Impact, High Complexity)

**Tests Affected**: 3 tests in Domain 6.5
- `large_entity_update_that_exceeds_mtu_is_correctly_reassembled`
- `fragment_loss_causes_older_state_until_a_full_later_update`
- `compression_on_off_does_not_change_observable_semantics`

**What's Needed**:
- Configure MTU size
- Force fragmentation
- Toggle compression
- Verify reassembly

**Current State**:
- ✅ Fragmentation likely handled internally
- ❌ No MTU configuration API
- ❌ No compression toggle API
- ❌ No way to verify fragmentation/reassembly

**Blocking Issue**: These are low-level transport details that may not be easily testable without deeper access.

**Solution**:
1. Add MTU config to `ConnectionConfig`
2. Add compression toggle to `ConnectionConfig` or `ServerConfig`
3. Create large entity updates to trigger fragmentation
4. Verify reassembly through state observation

**Estimated Effort**: 6-8 hours  
**Priority**: **LOW** (unblocks 3 tests, but low-level details)

---

### Category 12: World Integration Verification (Low Impact, Medium Complexity)

**Tests Affected**: 3 tests in Domain 8.3
- `server_world_integration_receives_every_insert_update_remove_exactly_once`
- `client_world_integration_stays_in_lockstep_with_naias_view`
- `world_integration_cleans_up_completely_on_disconnect_and_reconnect` (partial)

**What's Needed**:
- Access to `TestWorld` state to verify operations
- Compare `TestWorld` state with Naia's internal state
- Verify cleanup on disconnect

**Current State**:
- ✅ `TestWorld` exists and is used
- ❌ No API to inspect `TestWorld` state from tests
- ❌ No API to compare with Naia's internal state

**Blocking Issue**: `TestWorld` state isn't easily accessible from test code.

**Solution**:
1. Add getter methods to access `TestWorld` entities/components
2. Add comparison helpers
3. Expose through `ServerExpectCtx` and `ClientExpectCtx`

**Estimated Effort**: 3-4 hours  
**Priority**: **LOW** (unblocks 3 tests, but world integration is already tested indirectly)

---

### Category 13: Client API Mutation Safety (Low Impact, Low Complexity)

**Tests Affected**: 1 test in Domain 8.4
- `mutating_out_of_scope_entity_for_a_given_user_is_ignored_or_errors_predictably`

**What's Needed**:
- Attempt to mutate entity via client API when out of scope
- Verify error or ignore behavior

**Current State**:
- ✅ Client mutation APIs exist
- ❌ Test needs to verify error handling

**Blocking Issue**: Test structure exists, just needs implementation.

**Solution**:
1. Complete the test implementation
2. Verify that client mutation of out-of-scope entity returns error or is ignored

**Estimated Effort**: 1-2 hours  
**Priority**: **LOW** (unblocks 1 test, straightforward)

---

### Category 14: Deterministic Replay (Low Impact, Medium Complexity)

**Tests Affected**: 1 test in Domain 6.1
- `deterministic_replay_of_a_scenario`

**What's Needed**:
- Deterministic random seed
- Run scenario twice with same seed
- Compare results

**Current State**:
- ✅ `TestClock` provides deterministic time
- ❌ No deterministic random seed
- ❌ No scenario replay mechanism

**Blocking Issue**: Random number generation may not be deterministic.

**Solution**:
1. Add deterministic RNG seed to `Scenario`
2. Use seeded RNG for all random operations
3. Add scenario replay helper

**Estimated Effort**: 3-4 hours  
**Priority**: **LOW** (unblocks 1 test, nice-to-have)

---

## Prioritization Matrix

### Tier 1: High Impact, Quick Wins (Do First)
1. **Link Conditioner Configuration** (7 tests, 4-6 hours) - HIGH PRIORITY
2. **Tick-Buffered Channel API** (2 tests, 2-3 hours) - MEDIUM PRIORITY

**Total**: 9 tests, 6-9 hours

**Note**: Events API tests removed - these should be unit tests for test harness, not E2E tests

### Tier 2: Medium Impact, Moderate Effort (Do Next)
4. **Server/Client Configuration Limits** (2 tests, 3-4 hours) - MEDIUM PRIORITY
5. **Request Timeout & Disconnect Handling** (2 tests, 4-6 hours) - MEDIUM PRIORITY
6. **Client API Mutation Safety** (1 test, 1-2 hours) - LOW PRIORITY (but quick)

**Total**: 5 tests, 8-12 hours

### Tier 3: Lower Priority (Do Later)
7. **Connection Config Timeouts & Retries** (4 tests, 4-6 hours)
8. **Command History & Tick Management** (5 tests, 6-8 hours)
9. **World Integration Verification** (3 tests, 3-4 hours)
10. **Deterministic Replay** (1 test, 3-4 hours)

**Total**: 13 tests, 16-22 hours

### Tier 4: Low Priority / Complex (Consider Deferring)
11. **Observability Metrics** (4 tests, 6-8 hours)
12. **Protocol Mismatch Testing** (5 tests, 6-8 hours)
13. **MTU, Fragmentation & Compression** (3 tests, 6-8 hours)
14. **Transport Comparison** (2 tests, 20+ hours) - Consider separate integration tests

**Total**: 14 tests, 38+ hours

---

## Recommended Implementation Order

### Phase 1: Quick Wins (1-2 days)
1. ✅ Link Conditioner Configuration
2. ✅ Tick-Buffered Channel API
3. ✅ Client API Mutation Safety

**Expected Result**: Unblock 10 tests with ~7-11 hours of work

### Phase 2: Core Infrastructure (2-3 days)
4. ✅ Link Conditioner Configuration
5. ✅ Server/Client Configuration Limits
6. ✅ Request Timeout & Disconnect Handling

**Expected Result**: Unblock 11 more tests with ~11-16 hours of work

### Phase 3: Advanced Features (1-2 weeks)
7. Connection Config Timeouts & Retries
8. Command History & Tick Management
9. World Integration Verification
10. Deterministic Replay

**Expected Result**: Unblock 13 more tests with ~16-22 hours of work

### Phase 4: Edge Cases & Observability (Ongoing)
11. Observability Metrics
12. Protocol Mismatch Testing
13. MTU, Fragmentation & Compression
14. Transport Comparison (separate integration tests)

**Expected Result**: Complete remaining 14 tests as time permits

---

## Summary

**Best Bang for Buck**: Focus on Tier 1 (Link Conditioner, Tick-Buffered Channels)
- **9 tests unblocked** with **6-9 hours** of work
- These are foundational features that enable many other tests
- Relatively straightforward implementation

**Total Effort Estimate**:
- Tier 1: 8-13 hours → 13 tests
- Tier 2: 8-12 hours → 5 tests  
- Tier 3: 16-22 hours → 13 tests
- Tier 4: 38+ hours → 14 tests

**Grand Total**: ~70-85 hours to complete all TODOs, but **~14-20 hours** gets you **14 tests** (nearly 40% of TODOs).

**Note**: Events API tests (4 tests) should be moved to unit test suite for test harness.
