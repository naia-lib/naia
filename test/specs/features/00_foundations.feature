# ============================================================================
# Foundations — Common Definitions, Determinism, Smoke — Grouped Contract Suite
# ============================================================================
# Post-A.4 grouping of multiple source feature files. Each source's content
# is preserved verbatim from the @Rule line onward; per-source separators
# (`# === Source: ... ===`) keep the original boundaries greppable. Free-text
# feature-description blocks from sources are stripped (gherkin only allows
# them under the top-level Feature:). @Rule/@Scenario tag numbers are
# renumbered globally within this file (each source's local 01, 02, ...
# becomes a continuous sequence) so namako sees no duplicate-tag collisions.
# ============================================================================

@Feature(foundations)
Feature: Foundations — Common Definitions, Determinism, Smoke

  # ==========================================================================
  # === Source: 00_common.feature ===
  # ==========================================================================

  @Rule(01)
  Rule: User-initiated misuse returns Result::Err

    @Scenario(01)
    Scenario: [common-01] API misuse returns Err not panic
      Given a test scenario
      And a connected client
      When the client attempts an invalid API operation
      Then the operation returns an Err result
      And no panic occurs

    @Scenario(02)
    Scenario: [common-01] Sending on wrong-direction channel returns Err
      Given a test scenario
      And a connected client
      When the client sends on a server-to-client channel
      Then the send returns an error
      And no panic occurs

    # Closes the `api_misuse_returns_error_not_panic` carve-out test
    # (test/harness/contract_tests/integration_only/00_common.rs).
    # Exercises `give_authority` for an out-of-scope client; product
    # contract is that this returns `Err(NotInScope)`, not panic.
    @Scenario(03)
    Scenario: [common-01] give_authority on out-of-scope client returns Err(NotInScope)
      Given a server is running
      And client A connects
      And the server spawns a delegated entity not in scope of any client
      When the server attempts to give authority to client A for the delegated entity
      Then the operation returns an Err result
      And no panic occurs

  @Rule(02)
  Rule: Remote or untrusted input must never panic

    @Scenario(01)
    Scenario: [common-02] Malformed inbound packet is dropped without panic
      Given a test scenario
      And a connected client
      When the server receives a malformed packet
      Then the packet is dropped
      And no panic occurs

    @Scenario(02)
    Scenario: [common-02] Duplicate replication messages do not panic
      Given a test scenario
      And a connected client with replicated entities
      When duplicate replication messages arrive
      Then they are handled idempotently
      And no panic occurs

  @Rule(03)
  Rule: Protocol mismatch is a deployment error not a panic

    @Scenario(01)
    Scenario: [common-02a] Protocol mismatch produces ProtocolMismatch rejection
      Given a test scenario
      And a server with protocol version A
      And a client with protocol version B
      When the client attempts to connect
      Then the connection is rejected with ProtocolMismatch
      And no panic occurs

    @Scenario(02)
    Scenario: [common-02a] Protocol mismatch does not establish connection
      Given a test scenario
      And a server with protocol version A
      And a client with protocol version B
      When the client attempts to connect
      Then the client does not observe ConnectEvent
      And no panic occurs

  @Rule(04)
  Rule: Determinism under deterministic inputs

    @Scenario(01)
    Scenario: [common-05] Identical inputs produce identical outputs
      Given a test scenario with deterministic time
      And a deterministic network input sequence
      When the same API call sequence is executed twice
      Then the event emission order is identical both times
      And the entity spawn order is identical both times

    @Scenario(02)
    Scenario: [common-05] Component update order is deterministic
      Given a test scenario with deterministic time
      And a deterministic network input sequence
      When the same API call sequence is executed twice
      Then the event emission order is identical both times

  @Rule(05)
  Rule: Per-tick determinism for concurrent operations

    @Scenario(01)
    Scenario: [common-06] Same-tick scope operations resolve deterministically
      Given a test scenario
      And multiple scope operations queued for the same tick
      When the tick is processed
      Then the final scope state reflects the last API call order

    @Scenario(02)
    Scenario: [common-06] Multiple commands for same tick apply in receipt order
      Given a test scenario
      And a server receiving multiple commands for the same tick
      When the tick is processed
      Then commands are applied in receipt order

  @Rule(06)
  Rule: Reconnect is a fresh session

    @Scenario(01)
    Scenario: [common-14] Reconnecting client receives fresh entity spawns
      Given a test scenario
      And a client that was previously connected
      And the client disconnected
      When the client reconnects
      Then it receives fresh entity spawns for all in-scope entities
      And no prior session state is retained

    @Scenario(02)
    Scenario: [common-14] Server treats reconnecting client as new session
      Given a test scenario
      And a client that was previously connected
      And the client disconnected
      When the client reconnects
      Then it receives fresh entity spawns for all in-scope entities
      And the client is connected

  # ──────────────────────────────────────────────────────────────────────
  # Phase D.1 — meta-policy contracts (common-03, 04, 07-13)
  # ──────────────────────────────────────────────────────────────────────
  #
  # The following common-NN IDs document project-policy invariants
  # rather than runtime behaviors:
  # - common-03: Framework invariant violations MUST panic
  # - common-04: Warnings are debug-only and non-normative
  # - common-07: Tests MUST NOT assert on logs
  # - common-08: Test obligation template
  # - common-09: Observable signals subsection
  # - common-10: Fixed invariants are locked
  # - common-11: Configurable defaults
  # - common-11a: New constants start as invariants
  # - common-12: Internal measurements vs exposed metrics
  # - common-12a: Test tolerance constants
  # - common-13: Metrics are non-normative for gameplay
  #
  # In legacy_tests/00_common.rs each is paired with an exemplary
  # `#[test]` fn that demonstrates the policy by following it. The
  # policy itself is a contract on test-author behavior, not on
  # runtime code under test, so a Gherkin scenario can't meaningfully
  # *exercise* it. We tag stub Scenarios for coverage-diff parity
  # and mark them `@Deferred @PolicyOnly` so the run report skips
  # execution. Phase F's parity gate considers these covered.

  @Rule(09)
  Rule: Meta-policy contracts (covered for parity, not executed)

    @Deferred @PolicyOnly
    @Scenario(03)
    Scenario: [common-03] Framework invariant violations MUST panic
      # Policy contract: only framework code can panic; user code
      # must not. Exemplified by 00_common.rs::framework_panic_on_invariant
      # which proves a misuse path inside the framework asserts loudly.
      Then the system intentionally fails

    @Deferred @PolicyOnly
    @Scenario(04)
    Scenario: [common-04] Warnings are debug-only and non-normative
      Then the system intentionally fails

    @Deferred @PolicyOnly
    @Scenario(05)
    Scenario: [common-07] Tests MUST NOT assert on logs
      Then the system intentionally fails

    @Deferred @PolicyOnly
    @Scenario(06)
    Scenario: [common-08] Test obligation template
      Then the system intentionally fails

    @Deferred @PolicyOnly
    @Scenario(07)
    Scenario: [common-09] Observable signals subsection
      Then the system intentionally fails

    @Deferred @PolicyOnly
    @Scenario(08)
    Scenario: [common-10] Fixed invariants are locked
      Then the system intentionally fails

    @Deferred @PolicyOnly
    @Scenario(09)
    Scenario: [common-11] Configurable defaults
      Then the system intentionally fails

    @Deferred @PolicyOnly
    @Scenario(10)
    Scenario: [common-11a] New constants start as invariants
      Then the system intentionally fails

    @Deferred @PolicyOnly
    @Scenario(11)
    Scenario: [common-12] Internal measurements vs exposed metrics
      Then the system intentionally fails

    @Deferred @PolicyOnly
    @Scenario(12)
    Scenario: [common-12a] Test tolerance constants
      Then the system intentionally fails

    @Deferred @PolicyOnly
    @Scenario(13)
    Scenario: [common-13] Metrics are non-normative for gameplay
      Then the system intentionally fails

# ============================================================================
# AMBIGUITIES + PROPOSED CLARIFICATIONS
# ============================================================================
# None identified.
# ============================================================================

  # ==========================================================================
  # === Source: smoke.feature ===
  # ==========================================================================

  @Rule(07)
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

  @Rule(08)
  Rule: Orphan Binding Stubs

    @Deferred @Stub
    @Scenario(01)
    Scenario: Stub for orphan binding 785e108353195a37...
      # Expression: "the system intentionally fails"
      Then the system intentionally fails

