# ============================================================================
# Replicated Resources — Grouped Contract Suite
# ============================================================================
# This file is the post-A.4 grouping of multiple source feature files into
# a single grouped suite per the SDD migration plan. Each `# === Source: ... ===`
# block below corresponds to one of the original 24 .feature files.
# ============================================================================

@Feature(07_resources)
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

    @Deferred
    @Scenario(02)
    Scenario: server inserts a static resource and a connected client observes it
      Given a Naia protocol with replicated resource type "MatchState"
      And a server and one connected client
      When the server inserts MatchState { phase: 1 } as a static resource
      And one full replication round trip elapses
      Then the client's MatchState is present
      And the client's MatchState.phase equals 1
      And the wire ID for the MatchState resource entity has is_static set to true

    @Deferred
    @Scenario(03)
    Scenario: client connects after the resource was already inserted
      Given a Naia protocol with replicated resource type "Score"
      And a server with Score { home: 5, away: 2 } already inserted at startup
      When a client connects and the handshake completes
      Then the client's Score is present within the first replication packet
      And the client's Score.home equals 5
      And the client's InsertResourceEvent for Score fired exactly once

    @Deferred
    @Scenario(04)
    Scenario: re-inserting an already-existing resource is rejected
      Given a Naia protocol with replicated resource type "Score"
      And a server with Score { home: 0, away: 0 } already inserted
      When the server attempts to insert Score again
      Then the operation returns a ResourceAlreadyExists error
      And the existing Score value is unchanged

  @Rule(02)
  Rule: Per-field diff updates

    @Deferred
    @Scenario(01)
    Scenario: single field update transmits only the dirty field
      Given a Naia protocol with replicated resource type "Score"
      And a server with Score { home: 0, away: 0 } and one connected client
      And the initial replication round trip has elapsed
      When the server mutates Score.home to 3
      And one replication round trip elapses
      Then the client's Score.home equals 3
      And the client's Score.away equals 0
      And the most recent server-to-client packet contains exactly one Score field update bit set

    @Deferred
    @Scenario(02)
    Scenario: multiple sequential field updates coalesce within a tick
      Given a Naia protocol with replicated resource type "Score"
      And a server with Score { home: 0, away: 0 } and one connected client
      And the initial replication round trip has elapsed
      When the server mutates Score.home to 1, then 2, then 3 within the same tick
      And one tick elapses
      Then the most recent server-to-client packet contains exactly one Score.home update
      And the client's Score.home equals 3

  @Rule(03)
  Rule: Removal and re-insertion

    @Deferred
    @Scenario(01)
    Scenario: server removes a resource and the client observes the removal
      Given a Naia protocol with replicated resource type "MatchState"
      And a server with MatchState { phase: 1 } and one connected client
      And the initial replication round trip has elapsed
      When the server removes MatchState
      And one replication round trip elapses
      Then the client's MatchState is absent
      And the client's RemoveResourceEvent for MatchState fired exactly once

    @Deferred
    @Scenario(02)
    Scenario: insert remove re-insert with different value
      Given a Naia protocol with replicated resource type "MatchState"
      And a server with one connected client
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

    @Deferred
    @Scenario(02)
    Scenario: client-held authority allows client mutation that propagates to server
      Given a Naia protocol with delegable replicated resource type "PlayerSelection"
      And a server with PlayerSelection { selected_id: 0 } and connected client "alice"
      And alice holds authority on PlayerSelection
      When alice mutates PlayerSelection.selected_id to 7
      And one replication round trip elapses
      Then the server's PlayerSelection.selected_id equals 7

    @Deferred
    @Scenario(03)
    Scenario: server-side mutation rejected while client holds authority
      Given a Naia protocol with delegable replicated resource type "PlayerSelection"
      And a server with PlayerSelection { selected_id: 0 } and connected client "alice"
      And alice holds authority on PlayerSelection
      When the server attempts to mutate PlayerSelection.selected_id to 99
      Then the attempt returns AuthorityError ClientHoldsAuthority
      And the value remains 0

    @Deferred
    @Scenario(04)
    Scenario: client releases authority and server reclaims
      Given a Naia protocol with delegable replicated resource type "PlayerSelection"
      And a server with PlayerSelection { selected_id: 0 } and connected client "alice"
      And alice holds authority on PlayerSelection
      And alice has set selected_id to 5
      When alice releases authority on PlayerSelection
      And one replication round trip elapses
      Then the server-side authority status for PlayerSelection is "Available"
      And subsequent client mutations from alice are rejected with AuthorityError ServerHoldsAuthority

    @Deferred
    @Scenario(05)
    Scenario: client disconnects while holding authority value persists
      Given a Naia protocol with delegable replicated resource type "PlayerSelection"
      And a server with PlayerSelection { selected_id: 0 } and connected client "alice"
      And alice holds authority on PlayerSelection
      And alice has set selected_id to 5
      When alice disconnects ungracefully
      And the server's disconnect-detection elapses
      Then the server's authority status for PlayerSelection is "Available"
      And the resource value remains the last value alice committed
      And the resource is not despawned

  @Rule(05)
  Rule: Soft rejection of client writes to server-authoritative resources

    @Deferred
    @Scenario(01)
    Scenario: client mutation of server-authoritative resource is silently dropped locally
      Given a Naia protocol with replicated resource type "Score"
      And a server with Score { home: 0, away: 0 } and connected client "alice"
      And the initial replication round trip has elapsed
      When alice mutates Score.home to 99 via ResMut Score
      Then no replication packet is sent from alice carrying the Score.home change
      And alice's local Score.home immediately reads as 99
      When the server later mutates Score.home to 1
      And one replication round trip elapses
      Then alice's local Score.home equals 1
      And no AuthorityError was returned at any step

  @Rule(06)
  Rule: Per-resource priority

    @Deferred
    @Scenario(01)
    Scenario: per-resource priority gain affects send ordering under bandwidth pressure
      Given a Naia protocol with replicated resource type "Score"
      And a server with Score and 5000 dynamic entities each with Position
      And the server has set the priority gain for Score to 10.0
      And one connected client with constrained outbound bandwidth of 8 KB/tick
      And the initial replication round trip has elapsed
      When the server mutates Score.home and Position on every entity in the same tick
      Then the next outbound packet contains the Score update before any Position update

    @Deferred
    @Scenario(02)
    Scenario: default priority gain is 1.0
      Given a Naia protocol with replicated resource type "Score"
      And a server with Score
      Then the server's reported priority gain for Score is 1.0

  @Rule(07)
  Rule: Multi-world isolation

    @Deferred
    @Scenario(01)
    Scenario: resources in different worlds do not bleed across
      Given a Naia protocol with replicated resource type "Score"
      And a server with worlds "world_a" and "world_b" both registering Score
      When the server inserts Score { home: 1, away: 0 } in world_a
      And the server inserts Score { home: 100, away: 0 } in world_b
      Then world_a's Score.home equals 1
      And world_b's Score.home equals 100
      And mutating world_a's Score does not change world_b's Score

  @Rule(08)
  Rule: Late-join InsertResourceEvent firing

    @Deferred
    @Scenario(01)
    Scenario: late-joining client receives InsertResourceEvent for pre-existing resource
      Given a Naia protocol with replicated resource type "Score"
      And a server with Score { home: 7, away: 3 } already inserted at startup
      And client "alice" already connected
      When client "bob" connects after the resource was inserted
      And the connection handshake completes
      And bob's first replication packet arrives
      Then bob receives exactly one InsertResourceEvent for Score
      And bob's Score.home equals 7
      And alice did NOT receive a duplicate InsertResourceEvent for Score on bob's connection

  @Rule(09)
  Rule: Bevy adapter ergonomics

    @Deferred
    @Scenario(01)
    Scenario: server-side standard Bevy ResMut mutation replicates
      Given a Bevy server App with add_resource_events for Score registered
      And commands.replicate_resource was called with Score::new(0, 0)
      And one connected client
      And the initial replication round trip has elapsed
      When a server system runs ResMut Score home = 10
      And one replication round trip elapses
      Then the client's Res Score home equals 10

    @Deferred
    @Scenario(02)
    Scenario: client-side resource appears as a standard Bevy Res
      Given a Bevy client App with add_resource_events for Score registered
      And the server has inserted Score { home: 5, away: 2 }
      When the client connects and the initial replication round trip elapses
      Then a client system reading Res Score sees home=5, away=2

    @Deferred
    @Scenario(03)
    Scenario: user receives resource events never SpawnEntityEvent
      Given a Bevy server App and connected Bevy client with Score replicated
      When the server inserts mutates then removes Score
      And replication completes
      Then the client received exactly one InsertResourceEvent for Score
      And the client received at least one UpdateResourceEvent for Score
      And the client received exactly one RemoveResourceEvent for Score
      And the client received zero SpawnEntityEvent attributable to Score
      And the client received zero DespawnEntityEvent attributable to Score
      And the client received zero InsertComponentEvent attributable to Score

    @Deferred
    @Scenario(04)
    Scenario: client requests authority via Commands extension
      Given a Bevy server App with delegable PlayerSelection and connected Bevy client "alice"
      When alice's Bevy system runs commands.request_resource_authority for PlayerSelection
      And one replication round trip elapses
      Then alice's commands.resource_authority for PlayerSelection returns Some Granted
      And alice can mutate ResMut PlayerSelection and the change replicates to the server


