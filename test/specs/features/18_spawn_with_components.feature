# ============================================================================
# SpawnWithComponents Coalesce — Canonical Contract
# ============================================================================
# Source: contracts/18_spawn_with_components.spec.md
#
# Summary:
#   Phase 4 optimization: coalesce Spawn + N InsertComponent into one reliable
#   message. Obligations cover multi-component correctness, initial-value
#   fidelity, and zero-component fallback.
#
# Behavioral transparency:
#   All scenarios pass on both the legacy (separate Spawn + InsertComponent)
#   and Phase 4 (SpawnWithComponents) paths. The optimization is invisible to
#   the client API. Scenarios are regression guards that confirm behavioral
#   equivalence across both paths.
# ============================================================================

@Feature(spawn_with_components)
Feature: SpawnWithComponents Coalesce

  # --------------------------------------------------------------------------
  # Rule: Multi-component entity has all components available after spawn
  # --------------------------------------------------------------------------
  @Rule(01)
  Rule: Multi-component entity has all components after spawn

    @Scenario(01)
    Scenario: spawn-with-components-01-a — entity with two components has both after spawn
      Given a server is running
      And a client connects
      And a server-owned entity exists with Position and Velocity components
      And the client and entity share a room
      Then the entity spawns on the client with Position and Velocity

    @Scenario(02)
    Scenario: spawn-with-components-01-b — initial component values are correct after spawn
      Given a server is running
      And a client connects
      And a server-owned entity exists with Position and Velocity components
      And the client and entity share a room
      Then the entity spawns on the client with correct Position and Velocity values

  # --------------------------------------------------------------------------
  # Rule: Zero-component entity uses legacy Spawn path
  # --------------------------------------------------------------------------
  @Rule(02)
  Rule: Zero-component entity spawns via legacy path

    @Scenario(01)
    Scenario: spawn-with-components-02-a — entity with no components spawns correctly
      Given a server is running
      And a client connects
      And a server-owned entity exists without any replicated components
      And the client and entity share a room
      Then the entity spawns on the client
