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
# ----------------------------------------------------------------------------
# CORE METRIC RULES
# ----------------------------------------------------------------------------
#
# Metrics MUST NOT affect gameplay:
#   - MUST NOT affect replicated state, authority, scope, or delivery
#   - Reading metrics via API MUST NOT influence internal behavior
#
# Internal measurements vs exposed metrics:
#   - Internal RTT/jitter used for tick lead targeting (internal behavior)
#   - Exposed metrics are read-only snapshots for monitoring
#   - Internal measurements MAY differ from exposed metrics
#
# Metrics are safe to query at any time:
#   - MUST NOT panic
#   - If insufficient data: return well-defined default
#     * RTT: None or 0 until enough samples
#     * Throughput: 0 until enough samples
#
# ----------------------------------------------------------------------------
# RTT SEMANTICS
# ----------------------------------------------------------------------------
#
# RTT must be non-negative and bounded:
#   - RTT MUST be >= 0
#   - RTT MUST NOT overflow, become NaN, or Infinity
#   - Under stable conditions, RTT SHOULD converge within tolerance
#
# RTT behavior under jitter, loss, reordering:
#   - RTT MUST remain stable (no negative values)
#   - Duplicates MUST NOT cause unbounded spikes
#   - Reordering MUST NOT cause impossible values
#
# ----------------------------------------------------------------------------
# THROUGHPUT SEMANTICS
# ----------------------------------------------------------------------------
#
# Throughput must be non-negative:
#   - MUST NOT overflow or become NaN/Infinity
#   - Sustained higher traffic SHOULD increase throughput
#   - Sustained idle SHOULD decrease toward 0
#
# Bandwidth accounting consistency:
#   - If both payload and wire bytes exposed, they MUST be distinct
#   - Single metric MUST match documented accounting model
#
# ----------------------------------------------------------------------------
# LIFECYCLE AND TIME SOURCE
# ----------------------------------------------------------------------------
#
# Metrics reset/cleanup on lifecycle:
#   - On disconnect, clean up connection-scoped state
#   - New connections MUST NOT inherit stale samples
#
# Time source assumptions:
#   - Metrics MUST use same monotonic time source as tick/time system
#   - MUST NOT assume wall-clock correctness
#   - Paused time: no negative durations, no division by zero
#
# Per-direction consistency:
#   - Separate send/receive metrics MUST reflect direction correctly
#
# ----------------------------------------------------------------------------
# TESTABILITY
# ----------------------------------------------------------------------------
#
# Metrics are testable; logs are not:
#   - RTT, jitter, throughput are guaranteed stable for testing
#   - Tests MUST use inequality-style assertions only:
#     * rtt_ms >= 0
#     * rtt_ms < RTT_MAX_VALUE_MS
#   - Logs are non-normative; tests MUST NOT assert on logs
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

  # All executable scenarios deferred until step bindings implemented.

