# ============================================================================
# Common Definitions and Policies — Canonical Contract
# ============================================================================
# Source: contracts/00_common.spec.md
# Last converted: 2026-01-17
#
# Summary:
#   This specification defines cross-cutting concerns that apply to ALL Naia
#   specifications: error handling taxonomy, determinism requirements, test
#   conventions, configuration defaults vs invariants, and observability
#   policies. All other specs MUST reference this document and MUST NOT
#   contradict its policies.
#
# Terminology note:
#   This file is normative; scenarios are executable assertions; comments
#   labeled NORMATIVE are part of the contract.
# ============================================================================

# ============================================================================
# NORMATIVE CONTRACT MIRROR
# ============================================================================
#
# PURPOSE:
#   Define the canonical error handling, determinism, and test policies that
#   govern all Naia specifications.
#
# GLOSSARY:
#   - User-initiated misuse: Error caused by local application code or config
#   - Remote/untrusted input: Data from network or remote endpoints
#   - Framework invariant violation: Internal Naia bug (unreachable condition)
#   - Debug mode: When debug_assertions are enabled
#   - Prod mode: When debug_assertions are disabled
#
# ERROR HANDLING TAXONOMY:
#   | Condition                      | Response                    | Panic? |
#   |--------------------------------|-----------------------------|--------|
#   | Public API misuse              | Return Result::Err          | No     |
#   | Remote/untrusted input         | Drop (optionally warn)      | No     |
#   | Protocol mismatch              | Reject with ProtocolMismatch| No     |
#   | Framework invariant violation  | Panic                       | Yes    |
#
#   Key principle: Panic is reserved for internal invariant violations only.
#   No user action via public API can trigger a panic.
#
# ----------------------------------------------------------------------------
# ERROR HANDLING RULES
# ----------------------------------------------------------------------------
#
# User-initiated misuse MUST return Result::Err:
#   - Invalid channel configuration
#   - Sending on wrong-direction channel
#   - Oversize message payload
#   - Authority request on non-delegated entity
#   - Write attempt without permission
#   - Enqueueing more than MAX_COMMANDS_PER_TICK_PER_CONNECTION commands
#
# Remote/untrusted input MUST NOT panic:
#   - Malformed or oversize inbound packet
#   - Duplicate replication messages
#   - Authority request for out-of-scope entity
#   - Late command for already-processed tick
#   - TickBuffered message for evicted/old tick
#   - EntityProperty referencing unknown entity
#   - In Prod: ignore/drop silently
#   - In Debug: ignore/drop with warning (non-normative text)
#
# Protocol mismatch is a deployment error:
#   - Connection MUST be rejected with ProtocolMismatch
#   - Client MUST receive distinguishable indication
#   - MUST NOT panic
#
# Framework invariant violations MUST panic:
#   - Tick goes backwards in public API
#   - Older state delivered after newer on sequenced channel
#   - Internal send exceeding declared bounds
#   - GlobalEntity counter rollover
#
# Warnings are debug-only and non-normative:
#   - Warning text/format not part of contract
#   - Tests MUST NOT assert on warning content
#   - Warnings MUST NOT affect observable behavior
#
# ----------------------------------------------------------------------------
# DETERMINISM REQUIREMENTS
# ----------------------------------------------------------------------------
#
# Determinism under deterministic inputs:
#   If Time Provider, Network input, and API call sequence are deterministic,
#   then Naia's observable outputs MUST be deterministic:
#     - Event emission order
#     - Entity spawn/despawn order
#     - Component insert/update/remove order
#     - Authority state transitions
#
# Per-tick determinism rule:
#   - Scope operations: last API call wins in server-thread call order
#   - Multiple commands same tick: process in receipt order
#   - Multiple authority requests: first request received wins
#
# ----------------------------------------------------------------------------
# TEST CONVENTIONS
# ----------------------------------------------------------------------------
#
# Tests MUST NOT assert on logs:
#   - No assertions on log message content, presence, or format
#   - Observable behavior MUST be via events, API returns, or world state
#
# Every contract SHOULD include observable signals section
#
# ----------------------------------------------------------------------------
# CONFIGURATION: DEFAULTS VS INVARIANTS
# ----------------------------------------------------------------------------
#
# Fixed invariants (MUST NOT be configurable):
#   | Invariant                          | Value    |
#   |------------------------------------|----------|
#   | MAX_RELIABLE_MESSAGE_FRAGMENTS     | 2^16     |
#   | GlobalEntity rollover behavior     | Panic    |
#   | Tick type                          | u16      |
#   | Wrap-safe half-range               | 32768    |
#   | Request ID uniqueness scope        | Per-conn |
#   | MAX_COMMANDS_PER_TICK_PER_CONNECTION| 64      |
#   | protocol_id wire encoding          | u128 LE  |
#   | Command sequence encoding          | varint   |
#
# Configurable defaults:
#   | Default                            | Value    |
#   |------------------------------------|----------|
#   | Identity token TTL                 | 1 hour   |
#   | ENTITY_PROPERTY_RESOLUTION_TTL     | 60 sec   |
#   | MAX_PENDING_ENTITY_PROPERTY_*      | 4096/128 |
#   | TickBuffered tick_buffer_capacity  | Per-chan |
#   | DEFAULT_REQUEST_TIMEOUT            | 30 sec   |
#
# New constants start as invariants (MAY be promoted to configurable later)
#
# Test tolerance constants (non-normative, test-only):
#   | Constant                   | Value |
#   |----------------------------|-------|
#   | RTT_TOLERANCE_PERCENT      | 20    |
#   | RTT_MIN_SAMPLES            | 10    |
#   | RTT_MAX_VALUE_MS           | 10000 |
#   | THROUGHPUT_TOLERANCE_*     | 15/5  |
#   | LEAD_CONVERGENCE_TICKS     | 60    |
#   | METRIC_WINDOW_DURATION_MS  | 1000  |
#
# ----------------------------------------------------------------------------
# OBSERVABILITY POLICIES
# ----------------------------------------------------------------------------
#
# Internal measurements vs exposed metrics:
#   - Reading metrics MUST NOT influence internal behavior
#   - Internal measurements MAY differ from exposed metrics
#
# Metrics are non-normative for gameplay:
#   - Metrics MUST NOT affect replicated state, authority, scope, or delivery
#
# ----------------------------------------------------------------------------
# CONNECTION SEMANTICS
# ----------------------------------------------------------------------------
#
# Reconnect is a fresh session:
#   - No session resumption
#   - Server treats reconnecting client as new session
#   - Prior entity state, authority, buffered data discarded
#
# ============================================================================

@Feature(common_definitions_and_policies)
Feature: Common Definitions and Policies

  # --------------------------------------------------------------------------
  # Deferred scenarios tagged with @Deferred are excluded from the executable
  # plan but are tracked as promotion candidates by `namako review`.
  # --------------------------------------------------------------------------

  @Rule_01
  Rule: User-initiated misuse returns Result::Err

    @Scenario_01
    Scenario: API misuse returns Err not panic
      Given a test scenario
      And a connected client
      When the client attempts an invalid API operation
      Then the operation returns an Err result
      And no panic occurs

  @Rule_02
  Rule: Remote or untrusted input must never panic

    @Scenario_01
    Scenario: Malformed inbound packet is dropped without panic
      Given a test scenario
      And a connected client
      When the server receives a malformed packet
      Then the packet is dropped
      And no panic occurs

    @Scenario_02
    Scenario: Duplicate replication messages do not panic
      Given a test scenario
      And a connected client with replicated entities
      When duplicate replication messages arrive
      Then they are handled idempotently
      And no panic occurs

  @Rule_03
  Rule: Protocol mismatch is a deployment error not a panic

  @Scenario_01
  Scenario: Protocol mismatch produces ProtocolMismatch rejection
      Given a test scenario
      And a server with protocol version A
      And a client with protocol version B
      When the client attempts to connect
      Then the connection is rejected with ProtocolMismatch
      And no panic occurs

  @Rule_04
  Rule: Determinism under deterministic inputs

    @Scenario_01
    Scenario: Identical inputs produce identical outputs
      Given a test scenario with deterministic time
      And a deterministic network input sequence
      When the same API call sequence is executed twice
      Then the event emission order is identical both times
      And the entity spawn order is identical both times

  @Rule_05
  Rule: Per-tick determinism for concurrent operations

    @Scenario_01
    Scenario: Same-tick scope operations resolve deterministically
      Given a test scenario
      And multiple scope operations queued for the same tick
      When the tick is processed
      Then the final scope state reflects the last API call order

    @Scenario_02
    Scenario: Multiple commands for same tick apply in receipt order
      Given a test scenario
      And a server receiving multiple commands for the same tick
      When the tick is processed
      Then commands are applied in receipt order

  @Rule_06
  Rule: Reconnect is a fresh session

    @Scenario_01
    Scenario: Reconnecting client receives fresh entity spawns
      Given a test scenario
      And a client that was previously connected
      And the client disconnected
      When the client reconnects
      Then it receives fresh entity spawns for all in-scope entities
      And no prior session state is retained

# ============================================================================
# AMBIGUITIES + PROPOSED CLARIFICATIONS
# ============================================================================
# None identified.
# ============================================================================

