# ============================================================================
# Connection Lifecycle, Transport, Time/Ticks, Observability — Grouped Contract Suite
# ============================================================================
# Post-A.4 grouping of multiple source feature files. Each source's content
# is preserved verbatim from the @Rule line onward; per-source separators
# (`# === Source: ... ===`) keep the original boundaries greppable. Free-text
# feature-description blocks from sources are stripped (gherkin only allows
# them under the top-level Feature:). @Rule/@Scenario tag numbers are
# renumbered globally within this file (each source's local 01, 02, ...
# becomes a continuous sequence) so namako sees no duplicate-tag collisions.
# ============================================================================

@Feature(lifecycle)
Feature: Connection Lifecycle, Transport, Time/Ticks, Observability


  # Auto-applied prelude — every Scenario in this file gets this
  # Given run before its own Givens (idempotent).
  Background:
    Given a server is running

  # ==========================================================================
  # === Source: 01_connection_lifecycle.feature ===
  # ==========================================================================

  @Rule(01)
  Rule: Event ordering

    @Scenario(01)
    Scenario: [connection-03] Server observes ConnectEvent when client connects
      Given a server is running
      When a client connects
      Then the server has observed ConnectEvent

    @Scenario(02)
    Scenario: [connection-03] Client observes ConnectEvent when connected
      Given a server is running
      When a client connects
      Then the client has observed ConnectEvent

    @Scenario(03)
    Scenario: [connection-21] Client observes DisconnectEvent after disconnect
      Given a server is running
      And a client connects
      When the server disconnects the client
      Then the client has observed DisconnectEvent

    @Scenario(04)
    Scenario: [connection-21] DisconnectEvent occurs only after ConnectEvent on server
      Given a server is running
      And a client connects
      When the server disconnects the client
      Then the server observed ConnectEvent before DisconnectEvent

    @Scenario(05)
    Scenario: [connection-21] DisconnectEvent occurs only after ConnectEvent on client
      Given a server is running
      And a client connects
      When the server disconnects the client
      Then the client observed ConnectEvent before DisconnectEvent

    @Scenario(06)
    Scenario: [connection-21] Client observes DisconnectEvent only after ConnectEvent (polling)
      Given a server is running
      And a client connects
      When the server disconnects the client
      Then the client observes DisconnectEvent after ConnectEvent

    @Scenario(07)
    Scenario: [connection-03] Client observes ConnectEvent (polling variant)
      Given a server is running
      When a connected client
      Then the client observes ConnectEvent

  # --------------------------------------------------------------------------
  # Rule: Disconnect semantics
  # --------------------------------------------------------------------------
  # DisconnectEvent only after ConnectEvent.
  # --------------------------------------------------------------------------
  @Rule(02)
  Rule: Disconnect semantics

    @Scenario(01)
    Scenario: [connection-05] Server observes DisconnectEvent when client disconnects
      Given a server is running
      And a client connects
      When the server disconnects the client
      Then the server has observed DisconnectEvent

    @Scenario(02)
    Scenario: [connection-01] Connected client count decreases after disconnect
      Given a server is running
      And a client connects
      Then the server has 1 connected client
      When the server disconnects the client
      Then the server has 0 connected clients

    @Scenario(03)
    Scenario: [connection-01] Server can connect multiple clients
      Given a server is running
      When a client connects
      And a client connects
      Then the server has 2 connected clients

    @Scenario(04)
    Scenario: [connection-05] Server can disconnect one of multiple clients
      Given a server is running
      And a client connects
      And a client connects
      When the server disconnects the client
      Then the server has 1 connected client

    @Scenario(05)
    Scenario: [connection-01] Client is connected after successful connection
      Given a server is running
      When a client connects
      Then the client is connected

    @Scenario(06)
    Scenario: [connection-05] Client is not connected after disconnect
      Given a server is running
      And a client connects
      When the server disconnects the client
      Then the client is not connected

    @Scenario(07)
    Scenario: [connection-05] Server has no connected users after all clients disconnect
      Given a server is running
      And a client connects
      When the server disconnects the client
      Then the server has no connected users

    # The client sends a token-authenticated disconnect packet; the server verifies
    # the session token and fires DisconnectEvent immediately. This is the positive
    # case for the identity-token disconnect-authentication mechanism: a valid token
    # produces a clean disconnect without waiting for a heartbeat timeout.
    @Scenario(08)
    Scenario: [connection-22] Server observes DisconnectEvent on client-initiated graceful disconnect
      Given a server is running
      And a client connects
      When the client disconnects gracefully
      Then the server has observed DisconnectEvent

  # --------------------------------------------------------------------------
  # Rule: Auth-required event ordering
  # --------------------------------------------------------------------------
  # require_auth=true: AuthEvent → ConnectEvent → DisconnectEvent
  # --------------------------------------------------------------------------
  @Rule(03)
  Rule: Auth-required event ordering

    @Scenario(01)
    Scenario: [connection-07] Server observes AuthEvent before ConnectEvent
      Given a server is running with auth required
      When a client authenticates and connects
      Then the server observes AuthEvent before ConnectEvent

    @Scenario(02)
    Scenario: [connection-02] Rejected client observes RejectEvent not ConnectEvent
      Given a server is running with auth required
      When a client attempts to connect but is rejected
      Then the client observes RejectEvent
      And the client does not observe ConnectEvent
      And the client does not observe DisconnectEvent

    @Scenario(03)
    Scenario: [connection-07] Server full event ordering with disconnect
      Given a server is running with auth required
      When a client authenticates and connects
      When the server disconnects the client
      Then the server observes DisconnectEvent after ConnectEvent


# ============================================================================
# DEFERRED TESTS
# ============================================================================
# All other scenarios deferred until step bindings are implemented.
# See contracts/01_connection_lifecycle.spec.md for full scenario list.
# ============================================================================

  # ==========================================================================
  # === Source: 02_transport.feature ===
  # ==========================================================================

  @Rule(04)
  Rule: MTU boundary enforcement for outbound packets

    @Scenario(01)
    Scenario: [transport-04] Server can send packet within MTU limit
      Given a server is running
      And a client connects
      When the server sends a packet within the MTU limit
      Then the operation succeeds

    @Scenario(02)
    Scenario: [transport-04] Client can send packet within MTU limit
      Given a server is running
      And a client connects
      When the client sends a packet within the MTU limit
      Then the operation succeeds

    @Scenario(03)
    Scenario: [transport-04] Server rejects outbound packet exceeding MTU
      Given a server is running
      And a client connects
      When the server attempts to send a packet exceeding MTU
      Then the operation returns an Err result
      And the transport adapter is not called

    @Scenario(04)
    Scenario: [transport-04] Client rejects outbound packet exceeding MTU
      Given a server is running
      And a client connects
      When the client attempts to send a packet exceeding MTU
      Then the operation returns an Err result
      And the transport adapter is not called

  # --------------------------------------------------------------------------
  # Rule: Inbound packet handling for oversize and malformed packets
  # --------------------------------------------------------------------------
  # Packets exceeding MTU_SIZE_BYTES or malformed MUST be dropped.
  # In prod: silent drop. In debug: drop with warning (non-normative text).
  # --------------------------------------------------------------------------
  @Rule(05)
  Rule: Inbound packet handling for oversize and malformed packets

    @Scenario(01)
    Scenario: [transport-04] Server drops inbound packet exceeding MTU
      Given a server is running
      And a client connects
      When the server receives a packet exceeding MTU
      Then the packet is dropped
      And no panic occurs
      And no connection disruption occurs

    @Scenario(02)
    Scenario: [transport-04] Client drops inbound packet exceeding MTU
      Given a server is running
      And a client connects
      When the client receives a packet exceeding MTU
      Then the packet is dropped
      And no panic occurs
      And no connection disruption occurs

    @Scenario(03)
    Scenario: [transport-02] Server drops malformed inbound packet
      Given a server is running
      And a client connects
      When the server receives a malformed packet
      Then the packet is dropped
      And no panic occurs
      And no connection disruption occurs

    @Scenario(04)
    Scenario: [transport-02] Client drops malformed inbound packet
      Given a server is running
      And a client connects
      When the client receives a malformed packet
      Then the packet is dropped
      And no panic occurs
      And no connection disruption occurs

  # --------------------------------------------------------------------------
  # Rule: Transport unreliability tolerance
  # --------------------------------------------------------------------------
  # Naia MUST tolerate packet loss, duplication, and reordering without panic.
  # Higher-layer semantics (reliability, ordering) belong to messaging layer.
  # --------------------------------------------------------------------------
  @Rule(06)
  Rule: Transport unreliability tolerance

    @Scenario(01)
    Scenario: [transport-01] Server tolerates packet loss
      Given a server is running
      And a client connects
      When packets from the client are dropped by the transport
      Then the server continues operating normally
      And no panic occurs

    @Scenario(02)
    Scenario: [transport-01] Client tolerates packet loss
      Given a server is running
      And a client connects
      When packets from the server are dropped by the transport
      Then the client continues operating normally
      And no panic occurs

    @Scenario(03)
    Scenario: [transport-03] Server tolerates duplicate packets
      Given a server is running
      And a client connects
      When the server receives duplicate packets
      Then the server handles them without panic
      And no connection disruption occurs

    @Scenario(04)
    Scenario: [transport-03] Client tolerates duplicate packets
      Given a server is running
      And a client connects
      When the client receives duplicate packets
      Then the client handles them without panic
      And no connection disruption occurs

    @Scenario(05)
    Scenario: [transport-03] Server tolerates reordered packets
      Given a server is running
      And a client connects
      When the server receives packets in a different order than sent
      Then the server handles them without panic
      And no connection disruption occurs

    @Scenario(06)
    Scenario: [transport-03] Client tolerates reordered packets
      Given a server is running
      And a client connects
      When the client receives packets in a different order than sent
      Then the client handles them without panic
      And no connection disruption occurs

  # --------------------------------------------------------------------------
  # Rule: Transport abstraction independence
  # --------------------------------------------------------------------------
  # Higher layers MUST behave identically regardless of transport quality.
  # Transport-specific guarantees MUST NOT leak to application layer.
  # --------------------------------------------------------------------------
  @Rule(07)
  Rule: Transport abstraction independence

    @Scenario(01)
    Scenario: [transport-05] Application behavior is identical across transport types
      Given multiple transport adapters with different quality characteristics
      When the same application logic runs on each transport
      Then observable application behavior is identical
      And no transport-specific guarantees are exposed

    @Scenario(02)
    Scenario: [transport-05] Client layer abstracts transport reordering from application
      Given a server is running
      And a client connects
      When the client receives packets in a different order than sent
      Then the client handles them without panic
      And no connection disruption occurs

    @Scenario(03)
    Scenario: [transport-05] Server layer abstracts transport duplication from application
      Given a server is running
      And a client connects
      When the server receives duplicate packets
      Then the server handles them without panic
      And no connection disruption occurs

  # ==========================================================================
  # === Source: 04_time_ticks_commands.feature ===
  # ==========================================================================

  @Rule(08)
  Rule: Command ordering

    # Tests that multiple commands queued for the same tick are applied in
    # deterministic order (sequence order = send order).
    @Scenario(01)
    Scenario: Multiple commands for same tick are applied in sequence order
      Given a test scenario
      And a server receiving multiple commands for the same tick
      When the tick is processed
      Then commands are applied in receipt order

    # Tests that command processing does not panic regardless of processing
    # complexity. Per contract: "Remote/untrusted anomalies MUST NOT panic"
    @Scenario(02)
    Scenario: Command ordering processing does not cause panic
      Given a test scenario
      And a server receiving multiple commands for the same tick
      When the tick is processed
      Then commands are applied in receipt order
      And no panic occurs

    # Tests that out-of-order arrivals are buffered and applied in sequence order.
    # Per contract: "Apply in ascending sequence order regardless of arrival order"
    # and "Buffer out-of-order until earlier sequences arrive"
    @Scenario(03)
    Scenario: Out-of-order command arrivals are reordered by sequence number
      Given a test scenario
      And a server receiving commands arriving out of order for the same tick
      When the tick is processed
      Then commands are applied in ascending sequence order

    # [time-ticks-03] — ConnectEvent implies tick sync complete
    # Client MUST NOT emit ConnectEvent until tick sync complete.
    # After connection, client_tick() MUST return Some (tick is known).
    @Scenario(04)
    Scenario: time-ticks-03 — Client tick is known after connection
      Given a server is running
      And a client connects
      Then the client tick is available

    # [time-ticks-04] — Client knows server tick at connect time
    # The client knows the server's current tick at connection time (via tick sync).
    @Scenario(05)
    Scenario: time-ticks-04 — Server tick is known to client after connection
      Given a server is running
      And a client connects
      Then the server tick is known to the client

  # ==========================================================================
  # === Source: 05_observability_metrics.feature ===
  # ==========================================================================

  @Rule(09)
  Rule: Metric query safety

    @Scenario(01)
    Scenario: [observability-02] Metrics can be queried before connection without panic
      Given a server is running
      And a client is created but not connected
      When the client queries RTT metric
      Then no panic occurs
      And the RTT returns a defined default value

    @Scenario(02)
    Scenario: [observability-02] Metrics can be queried during handshake without panic
      Given a server is running
      And a client begins connecting
      When the client queries RTT metric during handshake
      Then no panic occurs
      And the RTT returns a defined default value

    @Scenario(03)
    Scenario: [observability-02] Metrics can be queried after disconnect without panic
      Given a server is running
      And a client connects
      And the client disconnects
      When the client queries RTT metric after disconnect
      Then no panic occurs
      And the RTT returns a defined default value

  # --------------------------------------------------------------------------
  # Rule: RTT must be non-negative and bounded
  # --------------------------------------------------------------------------
  # RTT estimates MUST be non-negative. RTT MUST NOT overflow or become
  # NaN/Infinity. Under stable link conditions, RTT SHOULD converge.
  # --------------------------------------------------------------------------
  @Rule(10)
  Rule: RTT must be non-negative and bounded

    @Scenario(01)
    Scenario: [observability-04] RTT converges under stable conditions
      Given a server is running
      And a client connects
      And the link has stable fixed-latency conditions
      When sufficient samples have been collected
      Then the RTT metric is non-negative
      And the RTT metric is within tolerance of expected latency

    @Scenario(02)
    Scenario: [observability-03] RTT remains bounded under jitter and loss
      Given a server is running
      And a client connects
      And the link has high jitter and moderate packet loss
      When traffic is exchanged for multiple metric windows
      Then the RTT metric is non-negative
      And the RTT metric is finite
      And the RTT metric is less than RTT_MAX_VALUE_MS

  # --------------------------------------------------------------------------
  # Rule: Metrics reset on connection lifecycle
  # --------------------------------------------------------------------------
  # New connections MUST NOT inherit stale samples from prior connections.
  # --------------------------------------------------------------------------
  @Rule(11)
  Rule: Metrics reset on connection lifecycle

    @Scenario(01)
    Scenario: [observability-07] Reconnection does not inherit stale RTT samples
      Given a server is running
      And a client connects with latency 50ms
      And RTT has converged near 100ms round-trip
      When the client disconnects
      And the client reconnects with latency 200ms
      And sufficient samples have been collected
      Then the RTT metric does not reflect the prior session value
      And the RTT metric converges toward the new latency

    @Scenario(02)
    Scenario: [observability-07] Metrics return defaults after disconnect before reconnection
      Given a server is running
      And a client connects with latency 50ms
      And RTT has converged near 100ms round-trip
      When the client disconnects
      And the client queries RTT metric after disconnect
      Then no panic occurs
      And the RTT returns a defined default value

  # ──────────────────────────────────────────────────────────────────────
  # Q5.B — coverage stubs (partially converted)
  # ──────────────────────────────────────────────────────────────────────
  #
  # 13 stubs converted to real Scenarios; 5 marked @PolicyOnly (Category A);
  # 17 remain @Deferred (Category B/C — bindings missing or untestable trigger).

  @Rule(12)
  Rule: Coverage stubs for legacy contracts not yet expressed as Scenarios

    @Scenario(01)
    Scenario: [connection-12] Anonymous connection allowed when auth disabled
      Given a server is running
      And a client connects
      Then the server has 1 connected client

    @Deferred
    @Scenario(02)
    Scenario: [connection-13] No replication before auth decision
      Then the system intentionally fails

    @Scenario(03)
    Scenario: [connection-14a] Protocol_id checked during handshake
      Given a server with protocol version A
      And a client with protocol version B
      When the client attempts to connect
      Then the connection is rejected with ProtocolMismatch

    @Deferred
    @Scenario(04)
    Scenario: [connection-15] No mid-session reauth
      Then the system intentionally fails

    @Scenario(05)
    Scenario: [connection-17] Server capacity reject produces RejectEvent
      Given a server is running with auth required
      When a client attempts to connect but is rejected
      Then the client observes RejectEvent

    @Deferred
    @Scenario(06)
    Scenario: [connection-19] Client disconnects on heartbeat timeout
      Then the system intentionally fails

    @Deferred
    @Scenario(07)
    Scenario: [connection-23] Malformed identity token rejected
      Then the system intentionally fails

    @Deferred
    @Scenario(08)
    Scenario: [connection-25] Expired or reused tokens obey lifetime semantics
      Then the system intentionally fails

    @Deferred
    @Scenario(09)
    Scenario: [connection-27] Valid identity token round-trips
      Then the system intentionally fails

    @Scenario(10)
    Scenario: [connection-28] Reconnect is fresh session (lifecycle parity)
      Given a server is running
      And a client connects
      When the client disconnects
      And the client reconnects
      Then the server has 1 connected client

    @Scenario(11)
    Scenario: [connection-29] Same protocol produces same protocol_id
      Given a server with protocol version A
      And a client with protocol version A
      When the client attempts to connect
      Then the client is connected

    @Scenario(12)
    Scenario: [connection-30] Protocol_id wire encoding allows connection
      Given a server with protocol version A
      And a client with protocol version A
      When the client attempts to connect
      Then the client is connected

    @Scenario(13)
    Scenario: [connection-31] Matched protocol_id allows connection
      Given a server with protocol version A
      And a client with protocol version A
      When the client attempts to connect
      Then the client is connected

    @Deferred @PolicyOnly
    @Scenario(14)
    Scenario: [connection-32] Protocol_id determined by wire-relevant aspects only

    @Scenario(15)
    Scenario: [connection-33] No partial protocol compatibility
      Given a server with protocol version A
      And a client with protocol version B
      When the client attempts to connect
      Then the connection is rejected with ProtocolMismatch

    @Deferred @PolicyOnly
    @Scenario(16)
    Scenario: [observability-01] Metrics do not affect replicated state

    @Deferred @PolicyOnly
    @Scenario(17)
    Scenario: [observability-01a] Querying metrics does not affect tick pacing

    @Deferred
    @Scenario(18)
    Scenario: [observability-05] Throughput must be non-negative
      Then the system intentionally fails

    @Deferred
    @Scenario(19)
    Scenario: [observability-06] Bandwidth exposes both directions
      Then the system intentionally fails

    @Deferred @PolicyOnly
    @Scenario(20)
    Scenario: [observability-08] Time source monotonic consistency

    @Deferred
    @Scenario(21)
    Scenario: [observability-09] Per-direction metrics consistency
      Then the system intentionally fails

    @Deferred @PolicyOnly
    @Scenario(22)
    Scenario: [observability-10] Metrics are testable without feature flags

    @Scenario(23)
    Scenario: [connection-04] Reject path: client receives RejectEvent
      Given a server is running with auth required
      When a client attempts to connect but is rejected
      Then the client observes RejectEvent

    @Scenario(24)
    Scenario: [connection-06] Auth event ordering on accepted handshake
      Given a server is running with auth required
      When a client authenticates and connects
      Then the server observes AuthEvent before ConnectEvent

    @Scenario(25)
    Scenario: [connection-08] AuthEvent fires before ConnectEvent server-side
      Given a server is running with auth required
      When a client authenticates and connects
      Then the server observes AuthEvent before ConnectEvent

    @Deferred
    @Scenario(26)
    Scenario: [connection-09] Disconnect during auth is handled cleanly
      Then the system intentionally fails

    @Deferred
    @Scenario(27)
    Scenario: [connection-10] Multiple concurrent auth requests serialize
      Then the system intentionally fails

    @Deferred
    @Scenario(28)
    Scenario: [connection-11] Auth payload survives handshake roundtrip
      Then the system intentionally fails

    @Scenario(29)
    Scenario: [connection-14] Protocol mismatch produces ProtocolMismatch reject
      Given a server with protocol version A
      And a client with protocol version B
      When the client attempts to connect
      Then the connection is rejected with ProtocolMismatch

    @Scenario(30)
    Scenario: [connection-16] Server cap enforced via RejectEvent
      Given a server is running with auth required
      When a client attempts to connect but is rejected
      Then the client observes RejectEvent

    @Deferred
    @Scenario(31)
    Scenario: [connection-18] Heartbeat liveness triggers timeout disconnect
      Then the system intentionally fails

    @Deferred
    @Scenario(32)
    Scenario: [connection-20] Reconnect after timeout is fresh session
      Then the system intentionally fails

    @Deferred
    @Scenario(33)
    Scenario: [connection-22] Disconnect propagates to scoped entities
      Then the system intentionally fails

    @Deferred
    @Scenario(34)
    Scenario: [connection-24] Identity token format validation
      Then the system intentionally fails

    @Deferred
    @Scenario(35)
    Scenario: [connection-26] Token reuse blocked across sessions
      Then the system intentionally fails
