# ============================================================================
# Entity Replication, Spawn-with-Components, Immutable Components — Grouped Contract Suite
# ============================================================================
# This file is the post-A.4 grouping of multiple source feature files into
# a single grouped suite per the SDD migration plan. Each `# === Source: ... ===`
# block below corresponds to one of the original 24 .feature files.
# ============================================================================

@Feature(03_replication)
Feature: Entity Replication, Spawn-with-Components, Immutable Components

  # ==========================================================================
  # === Source: 07_entity_replication.feature ===
  # ==========================================================================


  # --------------------------------------------------------------------------
  # Rule: Entity Replication
  # --------------------------------------------------------------------------
  # Core replication semantics: spawn, component sync, identity stability
  # --------------------------------------------------------------------------
  @Rule(01)
  Rule: Entity Replication

    @Scenario(01)
    Scenario: Entity spawns on client when entering scope
      Given a server is running
      And a client connects
      And a server-owned entity exists with a replicated component
      And the client and entity share a room
      Then the entity spawns on the client with the replicated component

    @Scenario(02)
    Scenario: Component updates are replicated to client
      Given a server is running
      And a client connects
      And a server-owned entity exists with a replicated component
      And the client and entity share a room
      And the entity is in-scope for the client
      When the server updates the replicated component
      Then the client observes the component update

    @Scenario(03)
    Scenario: Entity despawns on client when leaving scope
      Given a server is running
      And a client connects
      And a server-owned entity exists with a replicated component
      And the client and entity share a room
      And the entity is in-scope for the client
      When the server excludes the entity for the client
      Then the entity despawns on the client

    # [entity-replication-06] — GlobalEntity identity is stable during entity lifetime
    # The EntityKey (Naia's GlobalEntity abstraction) MUST remain stable while
    # the entity exists; no reassignment during updates.
    @Scenario(04)
    Scenario: entity-replication-06 — GlobalEntity identity is stable during entity lifetime
      Given a server is running
      And a client connects
      And a server-owned entity exists with a replicated component
      And the client and entity share a room
      And the entity is in-scope for the client
      When the server updates the replicated component
      Then the entity GlobalEntity remains unchanged

    # [entity-replication-07] — Server state overwrites client local state on conflict
    # When the server holds authoritative state and the client has a local modification,
    # the next server replication MUST overwrite the client's local value.
    @Scenario(05)
    Scenario: entity-replication-07 — Server state overwrites client local state on conflict
      Given a server is running
      And a client connects
      And a server-owned entity exists with a replicated component
      And the client and entity share a room
      And the entity is in-scope for the client
      And the client modifies the component locally
      When the server updates the replicated component
      Then the client observes the server value




  # ==========================================================================
  # === Source: 18_spawn_with_components.feature ===
  # ==========================================================================


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


  # ==========================================================================
  # === Source: 19_immutable_components.feature ===
  # ==========================================================================


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


