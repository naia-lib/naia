# ============================================================================
# Priority Accumulator — Canonical Contract
# ============================================================================
# Source: contracts/20_priority_accumulator.spec.md
#
# Summary:
#   Sidequest coverage for the three BDD obligations that require a real
#   server + client round-trip: spawn-burst drainage under bandwidth pressure,
#   gain-override persistence across a send cycle, and per-entity value
#   convergence under cross-entity priority reorder.
#
# Behavioral transparency:
#   All three scenarios are regression guards. The current implementation
#   satisfies every obligation; these specs pin the invariant so future
#   refactors to the priority sort cannot silently violate it.
# ============================================================================

@Feature(priority_accumulator)
Feature: Priority Accumulator

  # --------------------------------------------------------------------------
  # Rule: Spawn-burst drains under budget (AB-BDD-1)
  # A batch of entities spawned in one tick eventually reaches the client.
  # --------------------------------------------------------------------------
  @Rule(01)
  Rule: Spawn-burst drains under budget

    @Scenario(01)
    Scenario: priority-accumulator-01-a — 16-entity spawn burst eventually reaches client
      Given a server is running
      And a client connects
      When the server spawns 16 entities in one tick and scopes them for the client
      Then the client eventually observes all 16 spawned entities

  # --------------------------------------------------------------------------
  # Rule: Gain persistence across send (B-BDD-6)
  # set_gain(N) survives the send cycle.
  # --------------------------------------------------------------------------
  @Rule(02)
  Rule: Gain override persists across send cycle

    @Scenario(01)
    Scenario: priority-accumulator-02-a — global gain stays set after entity replicates
      Given a server is running
      And a client connects
      And a server-owned entity exists with a replicated component
      And the client and entity share a room
      And the entity is in-scope for the client
      When the server sets the global priority gain on the last entity to 5.0
      Then the global priority gain on the last entity is 5.0
      And the client eventually sees the last entity
      And the global priority gain on the last entity is still 5.0

  # --------------------------------------------------------------------------
  # Rule: Per-entity value convergence under cross-entity reorder (B-BDD-8)
  # Clients converge to the latest server value for each entity even when
  # the send-time priority sort reorders bundles across entities.
  # --------------------------------------------------------------------------
  @Rule(03)
  Rule: Per-entity value convergence under cross-entity reorder

    @Scenario(01)
    Scenario: priority-accumulator-03-a — interleaved mutations converge per-entity
      Given a server is running
      And a client connects
      And two server-owned entities A and B exist each with a replicated component in-scope for the client
      When the server mutates entity A's component to x=10 y=20
      And the server mutates entity B's component to x=30 y=40
      And the server mutates entity A's component to x=50 y=60
      Then the client eventually observes entity A at x=50 y=60
      And the client eventually observes entity B at x=30 y=40
