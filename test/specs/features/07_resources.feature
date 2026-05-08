# ============================================================================
# Replicated Resources — Grouped Contract Suite
# ============================================================================
# Post-A.4 grouping of multiple source feature files. Each source's content
# is preserved verbatim from the @Rule line onward; per-source separators
# (`# === Source: ... ===`) keep the original boundaries greppable. Free-text
# feature-description blocks from sources are stripped (gherkin only allows
# them under the top-level Feature:). @Rule/@Scenario tag numbers are
# renumbered globally within this file (each source's local 01, 02, ...
# becomes a continuous sequence) so namako sees no duplicate-tag collisions.
# ============================================================================

@Feature(resources)
Feature: Replicated Resources

  # ==========================================================================
  # === Source: 21_replicated_resources.feature ===
  # ==========================================================================

  @Rule(01)
  Rule: Registration & basic insert/observe

    @Scenario(01)
    Scenario: server inserts a dynamic resource and a connected client observes it
      Given a Naia protocol with replicated resource type "Score"
      And a server and one connected client
      When the server inserts Score { home: 0, away: 0 } as a dynamic resource
      And one full replication round trip elapses
      Then the client's Score is present
      And the client's Score.home equals 0
      And the client's Score.away equals 0

    # [resource-registration-02] — Static resources use a fixed wire ID
    # The is_static flag in the wire protocol distinguishes static from dynamic
    # resource entities. Requires packet-level introspection to verify.
    @PolicyOnly
    @Scenario(02)
    Scenario: server inserts a static resource and a connected client observes it

    # [resource-registration-03] — Late-joining clients receive pre-inserted resources
    # Both the resource value AND an InsertResourceEvent fire within the first
    # replication packet after handshake. Requires per-packet event counting.
    @PolicyOnly
    @Scenario(03)
    Scenario: client connects after the resource was already inserted

    # [resource-registration-04] — Re-insert is idempotent / rejected
    # Attempting to insert a resource type that is already present returns false
    # and leaves the existing value unchanged.
    @Scenario(04)
    Scenario: re-inserting an already-existing resource is rejected
      Given a server is running
      When the server inserts Score { home: 0, away: 0 } as a dynamic resource
      And the server attempts to re-insert Score { home: 99, away: 99 }
      Then the server's Score.home equals 0
      And the server's Score.away equals 0

  @Rule(02)
  Rule: Per-field diff updates

    # [resource-diff-01] — Only dirty fields are transmitted
    # A single-field mutation must produce exactly one set bit in the Score
    # component's field-mask on the wire. Requires packet-level bit inspection.
    @PolicyOnly
    @Scenario(01)
    Scenario: single field update transmits only the dirty field

    # [resource-diff-02] — Multiple mutations within a tick coalesce
    # Successive mutations to the same field within one tick produce exactly
    # one field-mask bit in the outgoing packet. Requires packet-level inspection.
    @PolicyOnly
    @Scenario(02)
    Scenario: multiple sequential field updates coalesce within a tick

  @Rule(03)
  Rule: Removal and re-insertion

    @Scenario(01)
    Scenario: server removes a resource and the client observes the removal
      Given a Naia protocol with replicated resource type "MatchState"
      And a server with MatchState { phase: 1 } and one connected client
      And the initial replication round trip has elapsed
      When the server removes MatchState
      And one replication round trip elapses
      Then the client's MatchState is absent

    @Scenario(02)
    Scenario: insert remove re-insert with different value
      Given a server and one connected client
      When the server inserts MatchState { phase: 1 } as static
      And one replication round trip elapses
      Then the client's MatchState.phase equals 1
      When the server removes MatchState
      And one replication round trip elapses
      Then the client's MatchState is absent
      When the server inserts MatchState { phase: 2 } as static
      And one replication round trip elapses
      Then the client's MatchState.phase equals 2

  @Rule(04)
  Rule: Authority delegation (V1 client-authoritative)

    @Scenario(01)
    Scenario: client requests authority on a delegable resource and receives it
      Given a Naia protocol with delegable replicated resource type "PlayerSelection"
      And a server with PlayerSelection { selected_id: 0 } and connected client "alice"
      And the initial replication round trip has elapsed
      When alice requests authority on PlayerSelection
      And one replication round trip elapses
      Then alice's authority status for PlayerSelection is "Granted"

    @Scenario(02)
    Scenario: client-held authority allows client mutation that propagates to server
      Given a Naia protocol with delegable replicated resource type "PlayerSelection"
      And a server with PlayerSelection { selected_id: 0 } and connected client "alice"
      And alice holds authority on PlayerSelection
      When alice mutates PlayerSelection.selected_id to 7
      And one replication round trip elapses
      Then the server's PlayerSelection.selected_id equals 7

    # [resource-authority-03] — Server mutation is rejected while client holds authority
    # Attempting server.mutate_resource() when a client holds Granted status must
    # return AuthorityError::ClientHoldsAuthority. Requires error-return shape from
    # the server mutation API (harness wraps to Option, not Result).
    @PolicyOnly
    @Scenario(03)
    Scenario: server-side mutation rejected while client holds authority

    @Scenario(04)
    Scenario: client releases authority and server reclaims
      Given a Naia protocol with delegable replicated resource type "PlayerSelection"
      And a server with PlayerSelection { selected_id: 0 } and connected client "alice"
      And alice holds authority on PlayerSelection
      When alice releases authority on PlayerSelection
      And one replication round trip elapses
      Then the server-side authority status for PlayerSelection is "Available"

    # [resource-authority-05] — Disconnect while holding authority reclaims cleanly
    # Ungraceful disconnect should reset authority to Available and preserve last value.
    # Requires ungraceful disconnect support and server-side timeout detection;
    # neither is exposed by the test harness.
    @PolicyOnly
    @Scenario(05)
    Scenario: client disconnects while holding authority value persists

  @Rule(05)
  Rule: Soft rejection of client writes to server-authoritative resources

    # [resource-soft-reject-01] — Client writes to server-authoritative resource are local-only
    # The mutation updates the client's local mirror but no replication packet is sent.
    # Requires per-packet inspection to verify no outbound Score.home packet from client.
    @PolicyOnly
    @Scenario(01)
    Scenario: client mutation of server-authoritative resource is silently dropped locally

  @Rule(06)
  Rule: Per-resource priority

    # [resource-priority-01] — Priority gain affects send ordering under bandwidth pressure
    # With constrained bandwidth, a resource with gain=10 must be sent before lower-priority
    # entities. Requires simulated bandwidth cap and per-packet ordering inspection.
    @PolicyOnly
    @Scenario(01)
    Scenario: per-resource priority gain affects send ordering under bandwidth pressure

    # [resource-priority-02] — Default priority gain is 1.0.
    # Requires a harness getter for the resource priority gain by type
    # (server_expect_ctx::resource_priority_gain not yet exposed).
    @PolicyOnly
    @Scenario(02)
    Scenario: default priority gain is 1.0

  @Rule(07)
  Rule: Multi-world isolation

    # [resource-multiworld-01] — Resources are scoped per-world
    # Requires server startup with two named worlds (world_a, world_b).
    # The test harness uses a single default world; multi-world isolation
    # is a protocol invariant verified by design.
    @PolicyOnly
    @Scenario(01)
    Scenario: resources in different worlds do not bleed across

  @Rule(08)
  Rule: Late-join InsertResourceEvent firing

    # [resource-latejoin-01] — Late-joining client receives exactly one InsertResourceEvent
    # Requires per-client event-count tracking for InsertResourceEvent, which the
    # harness does not currently expose as a tracked event type.
    @PolicyOnly
    @Scenario(01)
    Scenario: late-joining client receives InsertResourceEvent for pre-existing resource

  @Rule(09)
  Rule: Bevy adapter ergonomics

    # [resource-bevy-01..04] — Bevy-specific adapter tests
    # Rules 09/Scenario(01-04) are Bevy adapter contracts (ResMut, Res, resource events,
    # commands.request_resource_authority). Not covered by the non-Bevy test harness.

    @PolicyOnly
    @Scenario(01)
    Scenario: server-side standard Bevy ResMut mutation replicates

    @PolicyOnly
    @Scenario(02)
    Scenario: client-side resource appears as a standard Bevy Res

    @PolicyOnly
    @Scenario(03)
    Scenario: user receives resource events never SpawnEntityEvent

    @PolicyOnly
    @Scenario(04)
    Scenario: client requests authority via Commands extension
