# ============================================================================
# Foundations — Common Definitions, Determinism, Smoke — Grouped Contract Suite
# ============================================================================
# This file is the post-A.4 grouping of multiple source feature files into
# a single grouped suite per the SDD migration plan. Each `# === Source: ... ===`
# block below corresponds to one of the original 24 .feature files.
# ============================================================================

@Feature(00_foundations)
Feature: Foundations — Common Definitions, Determinism, Smoke

  # ==========================================================================
  # === Source: 00_common.feature ===
  # ==========================================================================


  # --------------------------------------------------------------------------
  # Deferred scenarios tagged with @Deferred are excluded from the executable
  # plan but are tracked as promotion candidates by `namako review`.
  # --------------------------------------------------------------------------

  @Rule(01)
  Rule: User-initiated misuse returns Result::Err

    @Scenario(01)
    Scenario: API misuse returns Err not panic
      Given a test scenario
      And a connected client
      When the client attempts an invalid API operation
      Then the operation returns an Err result
      And no panic occurs

    @Scenario(02)
    Scenario: Sending on wrong-direction channel returns Err
      Given a test scenario
      And a connected client
      When the client sends on a server-to-client channel
      Then the send returns an error
      And no panic occurs

  @Rule(02)
  Rule: Remote or untrusted input must never panic

    @Scenario(01)
    Scenario: Malformed inbound packet is dropped without panic
      Given a test scenario
      And a connected client
      When the server receives a malformed packet
      Then the packet is dropped
      And no panic occurs

    @Scenario(02)
    Scenario: Duplicate replication messages do not panic
      Given a test scenario
      And a connected client with replicated entities
      When duplicate replication messages arrive
      Then they are handled idempotently
      And no panic occurs

  @Rule(03)
  Rule: Protocol mismatch is a deployment error not a panic

    @Scenario(01)
    Scenario: Protocol mismatch produces ProtocolMismatch rejection
      Given a test scenario
      And a server with protocol version A
      And a client with protocol version B
      When the client attempts to connect
      Then the connection is rejected with ProtocolMismatch
      And no panic occurs

    @Scenario(02)
    Scenario: Protocol mismatch does not establish connection
      Given a test scenario
      And a server with protocol version A
      And a client with protocol version B
      When the client attempts to connect
      Then the client does not observe ConnectEvent
      And no panic occurs

  @Rule(04)
  Rule: Determinism under deterministic inputs

    @Scenario(01)
    Scenario: Identical inputs produce identical outputs
      Given a test scenario with deterministic time
      And a deterministic network input sequence
      When the same API call sequence is executed twice
      Then the event emission order is identical both times
      And the entity spawn order is identical both times

    @Scenario(02)
    Scenario: Component update order is deterministic
      Given a test scenario with deterministic time
      And a deterministic network input sequence
      When the same API call sequence is executed twice
      Then the event emission order is identical both times

  @Rule(05)
  Rule: Per-tick determinism for concurrent operations

    @Scenario(01)
    Scenario: Same-tick scope operations resolve deterministically
      Given a test scenario
      And multiple scope operations queued for the same tick
      When the tick is processed
      Then the final scope state reflects the last API call order

    @Scenario(02)
    Scenario: Multiple commands for same tick apply in receipt order
      Given a test scenario
      And a server receiving multiple commands for the same tick
      When the tick is processed
      Then commands are applied in receipt order

  @Rule(06)
  Rule: Reconnect is a fresh session

    @Scenario(01)
    Scenario: Reconnecting client receives fresh entity spawns
      Given a test scenario
      And a client that was previously connected
      And the client disconnected
      When the client reconnects
      Then it receives fresh entity spawns for all in-scope entities
      And no prior session state is retained

    @Scenario(02)
    Scenario: Server treats reconnecting client as new session
      Given a test scenario
      And a client that was previously connected
      And the client disconnected
      When the client reconnects
      Then it receives fresh entity spawns for all in-scope entities
      And the client is connected

# ============================================================================
# AMBIGUITIES + PROPOSED CLARIFICATIONS
# ============================================================================
# None identified.
# ============================================================================



  # ==========================================================================
  # === Source: smoke.feature ===
  # ==========================================================================

  Verifies the core Namako v1 pipeline works end-to-end.

  @Rule(01)
  Rule: Namako Smoke Test

    @Scenario(01)
    Scenario: Server starts and accepts a connecting client
      Given a server is running
      When a client connects
      Then the server has 1 connected client

    @Scenario(02)
    Scenario: Server can disconnect a client
      Given a server is running
      And a client connects
      When the server disconnects the client
      Then the server has 0 connected clients
    @Scenario(03)
    Scenario: Multiple clients can connect to server
      Given a server is running
      When a client connects
      And a client connects
      And a client connects
      Then the server has 3 connected clients

    @Scenario(04)
    Scenario: Server tracks client count accurately
      Given a server is running
      Then the server has 0 connected clients
      When a client connects
      Then the server has 1 connected client
      When a client connects
      Then the server has 2 connected clients

    @Scenario(05)
    Scenario: Connecting client is in connected state
      Given a server is running
      When a client connects
      Then the client is connected

    @Scenario(06)
    Scenario: Disconnecting client is no longer connected
      Given a server is running
      And a client connects
      When the server disconnects the client
      Then the client is not connected

    @Scenario(07)
    Scenario: Server and client observe connect events
      Given a server is running
      When a client connects
      Then the server has observed ConnectEvent
      And the client has observed ConnectEvent
    @Scenario(08)
    Scenario: Server and client observe disconnect events
      Given a server is running
      And a client connects
      When the server disconnects the client
      Then the server has observed DisconnectEvent
      And the client has observed DisconnectEvent

    @Scenario(09)
    Scenario: Event ordering is correct on disconnect
      Given a server is running
      And a client connects
      When the server disconnects the client
      Then the server observed ConnectEvent before DisconnectEvent
      And the client observed ConnectEvent before DisconnectEvent



  # ==========================================================================
  # === Source: _orphan_stubs.feature ===
  # ==========================================================================

  Placeholder scenarios for bindings not yet used by real specifications.

  @Rule(01)
  Rule: Orphan Binding Stubs

    @Deferred @Stub
    @Scenario(01)
    Scenario: Stub for orphan binding 785e108353195a37...
      # Expression: "the system intentionally fails"
      Then the system intentionally fails


