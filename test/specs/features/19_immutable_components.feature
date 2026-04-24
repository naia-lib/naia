# ============================================================================
# Immutable Replicated Components — Canonical Contract
# ============================================================================
# Source: contracts/19_immutable_components.spec.md
#
# Summary:
#   Phase 5 optimization: components marked #[replicate(immutable)] are written
#   once on spawn and never diff-tracked. Obligations cover functional
#   replication correctness, zero diff-handler allocation for immutable-only
#   entities, and correct tracking for mixed mutable+immutable entities.
# ============================================================================

@Feature(immutable_components)
Feature: Immutable Replicated Components

  # --------------------------------------------------------------------------
  # Rule: Immutable component replicates to client
  # --------------------------------------------------------------------------
  @Rule(01)
  Rule: Immutable component replicates to client

    @Scenario(01)
    Scenario: immutable-01-a — entity with ImmutableLabel spawns on client
      Given a server is running
      And a client connects
      And a server-owned entity exists with only ImmutableLabel
      And the client and entity share a room
      And the entity is in-scope for the client
      Then the client entity has ImmutableLabel

  # --------------------------------------------------------------------------
  # Rule: No diff-handler receivers for immutable components
  # --------------------------------------------------------------------------
  @Rule(02)
  Rule: No diff-handler receivers for immutable components

    @Scenario(01)
    Scenario: immutable-02-a — ImmutableLabel creates no diff-handler receiver
      Given a server is running
      And a client connects
      And a server-owned entity exists with only ImmutableLabel
      And the client and entity share a room
      And the entity is in-scope for the client
      Then the global diff handler has 0 receivers

  # --------------------------------------------------------------------------
  # Rule: Mixed entity has exactly one receiver for the mutable component
  # --------------------------------------------------------------------------
  @Rule(03)
  Rule: Mixed entity has exactly one receiver for the mutable component

    @Scenario(01)
    Scenario: immutable-03-a — Position and ImmutableLabel yields one diff-handler receiver
      Given a server is running
      And a client connects
      And a server-owned entity exists with Position and ImmutableLabel
      And the client and entity share a room
      And the entity is in-scope for the client
      Then the global diff handler has 1 receiver
