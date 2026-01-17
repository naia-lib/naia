# ============================================================================
# Observability Metrics — Canonical Contract
# ============================================================================
# Source: contracts/05_observability_metrics.spec.md
# Last converted: 2026-01-17
#
# Summary:
#   This specification defines the only valid semantics for observability
#   metrics exposed by Naia (latency/RTT, bandwidth/throughput, counters).
#   Metrics are for monitoring/telemetry only and MUST NOT affect gameplay.
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
#   Define observability metric semantics for Naia monitoring/telemetry.
#
# GLOSSARY:
#   - Metric: Numeric value exposed for monitoring (not gameplay correctness)
#   - Sample: One measurement update contributing to a metric
#   - Window: Time/sample span for aggregation (moving average, EWMA, etc.)
#   - RTT: Round-trip time estimate from request/ack/heartbeat timing
#   - Throughput: Bytes-per-second estimate (send and/or receive)
#   - Steady link: Stable latency/loss/jitter parameters over multiple windows
#
# NORMATIVE METRIC RULES:
#   [observability-01] Metrics MUST NOT affect gameplay
#     - MUST NOT affect replicated state, authority, scope, or delivery
#     - Reading metrics via API MUST NOT influence internal behavior
#
#   [observability-01a] Internal measurements vs exposed metrics
#     - Internal RTT/jitter used for tick lead targeting (internal behavior)
#     - Exposed metrics are read-only snapshots for monitoring
#     - Internal measurements MAY differ from exposed metrics
#
#   [observability-02] Metrics are safe to query at any time
#     - MUST NOT panic
#     - If insufficient data: return well-defined default
#       * RTT: None or 0 until enough samples
#       * Throughput: 0 until enough samples
#
#   [observability-03] RTT must be non-negative and bounded
#     - RTT MUST be >= 0
#     - RTT MUST NOT overflow, become NaN, or Infinity
#     - Under stable conditions, RTT SHOULD converge within tolerance
#
#   [observability-04] RTT behavior under jitter, loss, reordering
#     - RTT MUST remain stable (no negative values)
#     - Duplicates MUST NOT cause unbounded spikes
#     - Reordering MUST NOT cause impossible values
#
#   [observability-05] Throughput must be non-negative
#     - MUST NOT overflow or become NaN/Infinity
#     - Sustained higher traffic SHOULD increase throughput
#     - Sustained idle SHOULD decrease toward 0
#
#   [observability-06] Bandwidth accounting consistency
#     - If both payload and wire bytes exposed, they MUST be distinct
#     - Single metric MUST match documented accounting model
#
#   [observability-07] Metrics reset/cleanup on lifecycle
#     - On disconnect, clean up connection-scoped state
#     - New connections MUST NOT inherit stale samples
#
#   [observability-08] Time source assumptions
#     - Metrics MUST use same monotonic time source as tick/time system
#     - MUST NOT assume wall-clock correctness
#     - Paused time: no negative durations, no division by zero
#
#   [observability-09] Per-direction consistency
#     - Separate send/receive metrics MUST reflect direction correctly
#
#   [observability-10] Metrics are testable; logs are not
#     - RTT, jitter, throughput are guaranteed stable for testing
#     - Tests MUST use inequality-style assertions only:
#       * rtt_ms >= 0
#       * rtt_ms < RTT_MAX_VALUE_MS
#     - Logs are non-normative; tests MUST NOT assert on logs
#
# TEST TOLERANCE CONSTANTS (non-normative, test-only):
#   | Constant                   | Value |
#   |----------------------------|-------|
#   | RTT_TOLERANCE_PERCENT      | 20    |
#   | RTT_MIN_SAMPLES            | 10    |
#   | RTT_MAX_VALUE_MS           | 10000 |
#   | THROUGHPUT_TOLERANCE_*     | 15/5  |
#   | LEAD_CONVERGENCE_TICKS     | 60    |
#   | METRIC_WINDOW_DURATION_MS  | 1000  |
#
# ============================================================================

Feature: Observability Metrics

  Background:
    Given a Naia test environment is initialized

  # --------------------------------------------------------------------------
  # Rule: Metrics do not affect gameplay
  # --------------------------------------------------------------------------
  # NORMATIVE: Observability metrics MUST NOT affect replicated state,
  # authority, scope, or message delivery semantics.
  # --------------------------------------------------------------------------
  Rule: Metrics do not affect gameplay

    Scenario: Querying metrics does not influence behavior
      Given a connected client and server
      When metrics are queried every tick during replication
      Then replication behavior is identical to when metrics are not queried

  # --------------------------------------------------------------------------
  # Rule: Metrics are safe to query at any time
  # --------------------------------------------------------------------------
  # NORMATIVE: Metrics APIs MUST be safe to query at any time and MUST NOT
  # panic. Return well-defined defaults if insufficient data.
  # --------------------------------------------------------------------------
  Rule: Metrics are safe to query at any time

    Scenario: Query metrics before connect does not panic
      Given a client that is not yet connected
      When RTT and throughput metrics are queried
      Then no panic occurs
      And RTT returns default value
      And throughput returns zero

    Scenario: Query metrics after disconnect does not panic
      Given a client that was connected and has disconnected
      When RTT and throughput metrics are queried
      Then no panic occurs
      And defined defaults are returned

  # --------------------------------------------------------------------------
  # Rule: RTT must be non-negative and bounded
  # --------------------------------------------------------------------------
  # NORMATIVE: RTT MUST be >= 0, MUST NOT overflow or become NaN/Infinity.
  # --------------------------------------------------------------------------
  Rule: RTT must be non-negative and bounded

    Scenario: RTT converges under stable conditions
      Given a connected client and server
      And fixed-latency low-jitter network conditions
      When sufficient samples are collected
      Then RTT is non-negative
      And RTT converges near expected value

    Scenario: RTT remains finite under jitter and loss
      Given a connected client and server
      And high jitter and moderate loss conditions
      When samples are collected
      Then RTT is non-negative
      And RTT is finite and bounded

  # --------------------------------------------------------------------------
  # Rule: RTT is stable under duplicates and reordering
  # --------------------------------------------------------------------------
  # NORMATIVE: RTT MUST remain stable. Duplicates MUST NOT cause unbounded
  # spikes. Reordering MUST NOT cause impossible values.
  # --------------------------------------------------------------------------
  Rule: RTT is stable under duplicates and reordering

    Scenario: Packet duplication does not cause unbounded RTT spike
      Given a connected client and server
      And a transport conditioner duplicating packets at high rate
      When RTT is measured
      Then RTT does not spike unboundedly

    Scenario: Reordering does not cause negative RTT
      Given a connected client and server
      And a transport conditioner reordering packets
      When RTT is measured
      Then RTT is non-negative

  # --------------------------------------------------------------------------
  # Rule: Throughput is non-negative and responds to traffic
  # --------------------------------------------------------------------------
  # NORMATIVE: Throughput MUST be >= 0. Sustained traffic changes should
  # be reflected in metric direction.
  # --------------------------------------------------------------------------
  Rule: Throughput is non-negative and responds to traffic

    Scenario: Throughput rises during high traffic and decays during idle
      Given a connected client and server
      When high traffic is sustained
      Then throughput increases
      When traffic becomes idle
      Then throughput decreases toward zero

    Scenario: Throughput stabilizes under constant traffic
      Given a connected client and server
      And constant traffic rate
      When sufficient samples are collected
      Then throughput stabilizes near expected rate

  # --------------------------------------------------------------------------
  # Rule: Metrics reset on connection lifecycle
  # --------------------------------------------------------------------------
  # NORMATIVE: On disconnect, clean up connection-scoped metric state.
  # New connections MUST NOT inherit stale samples.
  # --------------------------------------------------------------------------
  Rule: Metrics reset on connection lifecycle

    Scenario: Reconnect does not inherit prior session metrics
      Given a client that connected and established stable RTT
      When the client disconnects
      And the client reconnects
      Then metrics do not start with prior session values

  # --------------------------------------------------------------------------
  # Rule: Metrics use monotonic time source
  # --------------------------------------------------------------------------
  # NORMATIVE: Metrics MUST use same monotonic time source as tick/time.
  # Paused time MUST NOT cause invalid values.
  # --------------------------------------------------------------------------
  Rule: Metrics use monotonic time source

    Scenario: Paused deterministic time does not produce invalid metrics
      Given a deterministic time provider
      And a connected client and server
      When time is paused
      And metrics are queried
      Then no panic occurs
      And no invalid values are produced

# ============================================================================
# AMBIGUITIES + PROPOSED CLARIFICATIONS
# ============================================================================
# None identified. The observability metrics spec is well-defined.
# ============================================================================
