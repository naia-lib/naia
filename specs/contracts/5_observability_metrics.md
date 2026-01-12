# Observability Metrics

This spec defines the only valid semantics for *observability metrics* exposed by Naia (latency/RTT, bandwidth/throughput, and related counters).  
Normative keywords: **MUST**, **MUST NOT**, **SHOULD**, **MAY**.

---

## Glossary

- **Metric**: A numeric value exposed by Naia intended for monitoring/telemetry (not gameplay correctness).
- **Sample**: One measurement update contributing to a metric over time.
- **Window**: The time span or sample span used to aggregate a metric (moving average, EWMA, rolling sum, etc.).
- **RTT**: Round-trip time estimate (latency) derived from request/ack/heartbeat timing.
- **Throughput**: Bytes-per-second estimate (send and/or receive).
- **Steady link**: A link condition where latency/loss/jitter parameters are stable over multiple windows.
- **Fault model**: Packet loss, duplication, and reordering consistent with Naia transport simulation or real transport.

---

## References

- `2_transport.md` (fault model, heartbeats/acks, ordering/duplication behavior)
- `1_connection_lifecycle.md` (connect/disconnect lifecycle, timeouts, cleanup)
- `5_time_ticks_commands.md` (time source expectations, tick/time monotonicity)

---

## Contracts

### [observability-01] — Metric scope and non-normative gameplay impact
**Rule:** Observability metrics MUST NOT affect replicated state correctness, authority, scope, message delivery semantics, or any other gameplay-visible contract. Metrics are observational only.

**Clarifications:**
- Metrics MAY be used by applications for UI, logging, or adaptive behavior, but Naia's core semantics MUST remain correct regardless of whether metrics are queried.
- Reading metrics via public API MUST NOT influence Naia's internal behavior.

**Observable signals:**
- Metric values accessible via public API
- No change to replication/events based on metric queries

**Test obligations:**
- **observability-01.t1**: Run a representative scenario with metrics queried every tick vs never queried; externally observable replication/events MUST be identical.

---

### [observability-01a] — Internal measurements vs exposed metrics

Naia uses internal RTT/jitter estimates for:
- Client tick lead targeting (see `4_time_ticks_commands.md`)
- Pacing decisions
- Internal timeouts

**Relationship to exposed metrics:**
- Internal measurements MAY differ in precision, timing, or algorithm from exposed metrics
- Internal measurements are for protocol behavior; exposed metrics are for monitoring
- Internal measurement changes MUST NOT be observable via public metric APIs (beyond normal convergence)

**Rule:** Exposed observability metrics are read-only snapshots. They MUST NOT be used as inputs to Naia's internal algorithms. The internal algorithms use their own measurements.

**Clarification:** Tests MUST NOT assert on logs, warnings, or debug output (per `0_common.md`). Metric values are testable; log content is not.

**Observable signals:**
- (Internal measurements are not externally observable)
- Exposed metrics are available via API

**Test obligations:**
- (Internal measurement behavior is not directly testable; covered by tick sync and lead convergence tests)

---

### [observability-02] — Metric query safety and availability
**Rule:** Metrics APIs MUST be safe to query at any time after client/server object construction and MUST NOT panic. If a metric cannot be computed yet (insufficient data), it MUST return a well-defined default.

**Required defaults:**
- RTT: MUST return `None` or a documented sentinel value (e.g., 0) until enough samples exist.
- Throughput: MUST return 0 until enough samples exist.

**Test obligations:**
- **observability-02.t1**: Query metrics before connect, during handshake/auth delay, and immediately after connect; MUST not panic and MUST return defined defaults.
- **observability-02.t2**: Query metrics after disconnect; MUST not panic and MUST return defined defaults (or remain last-known if explicitly documented — choose one and enforce consistently).

---

### [observability-03] — RTT must be non-negative and bounded
**Rule:** RTT estimates MUST be non-negative. RTT MUST NOT overflow or become NaN/Infinity. Under stable link conditions, RTT SHOULD converge within a reasonable tolerance of the configured/true RTT.

**Interpretation:**
- “Reasonable tolerance” is implementation-defined but MUST be testable (e.g., within ±20% after N samples).

**Test obligations:**
- **observability-03.t1**: Under fixed-latency, low-jitter conditions, RTT converges near expected RTT and never negative.
- **observability-03.t2**: Under high jitter and moderate loss, RTT remains finite, non-negative, and bounded (no overflow/NaN).

---

### [observability-04] — RTT behavior under jitter, loss, and reordering
**Rule:** Under the transport fault model, RTT estimates MAY fluctuate but MUST remain stable in the sense that:
- It MUST NOT become negative.
- It MUST NOT oscillate wildly due to duplicate packets alone.
- Reordering MUST NOT cause RTT regression to an impossible value (e.g., negative elapsed time).

**Test obligations:**
- **observability-04.t1**: Enable packet duplication at high rate; RTT MUST not spike unboundedly solely due to duplicates.
- **observability-04.t2**: Enable reordering; RTT MUST remain finite and non-negative.

---

### [observability-05] — Throughput must be non-negative and monotonic per window semantics
**Rule:** Throughput (bytes/sec) MUST be non-negative and MUST NOT overflow or become NaN/Infinity. Throughput computations MUST be consistent with the documented windowing method.

**Clarifications:**
- If Naia uses a moving window/EWMA, then “monotonic” is not required; however values MUST update in the expected direction under sustained traffic changes:
  - Sustained higher traffic SHOULD increase reported throughput.
  - Sustained near-idle SHOULD decrease reported throughput toward 0.

**Test obligations:**
- **observability-05.t1**: Alternate between high-traffic and idle phases; throughput rises during high-traffic and decays during idle.
- **observability-05.t2**: Under constant traffic rate, throughput stabilizes near expected rate (within tolerance).

---

### [observability-06] — Bandwidth accounting includes retries/overhead if documented
**Rule:** If Naia exposes both “payload bytes” and “wire bytes” (or equivalent), then:
- Payload bytes MUST count only application payload (messages/components).
- Wire bytes MUST include protocol overhead and retransmissions.

If only one throughput metric exists, the spec MUST declare which accounting model it uses, and the implementation MUST match that model.

**Test obligations:**
- **observability-06.t1**: With reliable channel retries induced (loss), wire throughput increases relative to payload throughput (if both exist), or the single metric matches the documented accounting model.

---

### [observability-07] — Metrics reset/cleanup on connection lifecycle
**Rule:** On disconnect, Naia MUST clean up connection-scoped metric state so that:
- New connections do not inherit stale samples from prior connections.
- Metrics for a disconnected session MUST not continue accumulating samples.

**Allowed behaviors (pick one per metric and document consistently):**
- **Reset-to-default**: metrics revert to defaults immediately on disconnect.
- **Freeze-last-known**: metrics retain last-known value but do not update until reconnect; upon reconnect, metrics MUST reset or explicitly start a new session.

**Test obligations:**
- **observability-07.t1**: Connect, establish stable RTT, disconnect, reconnect; metrics MUST not “start” with prior session’s converged value unless Freeze-last-known is explicitly chosen AND reconnect resets correctly.

---

### [observability-08] — Time source assumptions
**Rule:** Metrics computations MUST rely on the same monotonic time source used by Naia’s tick/time system. Metrics MUST NOT assume wall-clock correctness. If the time source is paused (per deterministic test clock), metrics MUST behave consistently:
- No negative durations.
- No division by zero.
- Either no updates occur during pause or updates are well-defined (documented).

**Test obligations:**
- **observability-08.t1**: Pause deterministic time, keep querying metrics; MUST not panic and MUST not produce invalid values.
- **observability-08.t2**: Resume time; metrics continue updating normally.

---

### [observability-09] — Per-direction and per-transport consistency (if applicable)
**Rule:** If Naia exposes separate send/receive metrics, they MUST reflect direction correctly (send counts bytes sent, receive counts bytes received). If multiple transports exist, semantics MUST be consistent across transports (modulo known transport overhead differences).

**Test obligations:**
- **observability-09.t1**: Server sends heavy traffic, client sends minimal; send/receive metrics reflect asymmetry correctly.
- **observability-09.t2**: Run the same scenario over two transports; metrics remain within expected relative differences and do not violate invariants.

---

### [observability-10] — Metrics are testable; logs are not

**Metrics are normative and testable:**
- The following metrics are **guaranteed stable** and E2E tests MAY assert on them:
  - RTT estimate (non-negative, converges under stable conditions)
  - Throughput estimate (non-negative, converges under stable conditions)
  - Bandwidth counters (if exposed)
- Metrics MUST be available in the test harness **without requiring feature flags**
- Metric values MUST be queryable via public API

**Logs are non-normative:**
- Debug warnings, log messages, and diagnostic output are **non-normative**
- Tests MUST NOT assert on log output content, presence, or format
- Log output MAY change between versions without being considered a breaking change
- Any "debug warn" wording in specs is explicitly non-testable and MUST NOT gate correctness

**Feature flag rule:**
- Metrics do NOT require special feature flags to be available
- Debug logging MAY be gated by feature flags, but correctness MUST NOT depend on it

**Observable signals:**
- Metrics are queryable at runtime
- (Logs are intentionally not observable in specs)

**Test obligations:**
- `observability-10.t1`: Metrics are queryable without special feature flags
- `observability-10.t2`: Tests can assert on RTT/throughput convergence

---

## Notes for implementers

- This spec does not mandate a particular estimator (EWMA vs rolling window), but it DOES mandate:
  - Non-negative, finite outputs
  - Defined behavior with insufficient samples
  - Correct lifecycle cleanup
  - Convergence under stable conditions
- Any exposed metric MUST be documented in terms of:
  - Units
  - Window/estimator
  - Reset/freeze behavior on disconnect

---

## Appendix: Test Tolerance Constants

These constants define acceptable tolerances for E2E test assertions:

| Constant | Value | Description |
|----------|-------|-------------|
| `RTT_TOLERANCE_PERCENT` | 20 | Acceptable deviation from expected RTT |
| `RTT_MIN_SAMPLES` | 10 | Minimum samples before asserting RTT convergence |
| `RTT_MAX_VALUE_MS` | 10000 | Maximum valid RTT (sanity bound) |
| `THROUGHPUT_TOLERANCE_PERCENT` | 15 | Acceptable deviation from expected throughput |
| `THROUGHPUT_MIN_SAMPLES` | 5 | Minimum samples before asserting throughput |
| `LEAD_CONVERGENCE_TICKS` | 60 | Ticks to allow client tick lead to stabilize |
| `METRIC_WINDOW_DURATION_MS` | 1000 | Default metric aggregation window |

**Usage in tests:**
```rust
// Assert RTT within tolerance
assert!(
    (measured_rtt - expected_rtt).abs() <= expected_rtt * RTT_TOLERANCE_PERCENT / 100,
    "RTT {} not within {}% of expected {}",
    measured_rtt, RTT_TOLERANCE_PERCENT, expected_rtt
);
```

## Test obligations

TODO: Define test obligations for this specification.
