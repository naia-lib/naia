# ============================================================================
# Entity Replication, Spawn-with-Components, Immutable Components — Grouped Contract Suite
# ============================================================================
# Post-A.4 grouping of multiple source feature files. Each source's content
# is preserved verbatim from the @Rule line onward; per-source separators
# (`# === Source: ... ===`) keep the original boundaries greppable. Free-text
# feature-description blocks from sources are stripped (gherkin only allows
# them under the top-level Feature:). @Rule/@Scenario tag numbers are
# renumbered globally within this file (each source's local 01, 02, ...
# becomes a continuous sequence) so namako sees no duplicate-tag collisions.
# ============================================================================

@Feature(replication)
Feature: Entity Replication, Spawn-with-Components, Immutable Components


  # Auto-applied prelude — every Scenario in this file gets this
  # Given run before its own Givens (idempotent).
  Background:
    Given a server is running

  # ==========================================================================
  # === Source: 07_entity_replication.feature ===
  # ==========================================================================

  @Rule(01)
  Rule: Entity Replication

    @Scenario(01)
    Scenario: [entity-replication-01] Entity spawns on client when entering scope
      Given a server is running
      And a client connects
      And a server-owned entity exists with a replicated component
      And the client and entity share a room
      Then the entity spawns on the client with the replicated component

    @Scenario(02)
    Scenario: [entity-replication-02] Component updates are replicated to client
      Given a server is running
      And a client connects
      And a server-owned entity exists with a replicated component
      And the client and entity share a room
      And the entity is in-scope for the client
      When the server updates the replicated component
      Then the client observes the component update

    @Scenario(03)
    Scenario: [entity-replication-03] Entity despawns on client when leaving scope
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

  @Rule(02)
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
  @Rule(03)
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

  @Rule(04)
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
  @Rule(05)
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
  @Rule(06)
  Rule: Mixed entity has exactly one receiver for the mutable component

    @Scenario(01)
    Scenario: immutable-03-a — Position and ImmutableLabel yields one diff-handler receiver
      Given a server is running
      And a client connects
      And a server-owned entity exists with Position and ImmutableLabel
      And the client and entity share a room
      And the entity is in-scope for the client
      Then the global diff handler has 1 receiver

  # ──────────────────────────────────────────────────────────────────────
  # Phase D.4 — coverage stubs (deferred)
  # ──────────────────────────────────────────────────────────────────────

  @Rule(07)
  Rule: Coverage stubs for legacy contracts not yet expressed as Scenarios

    @Deferred
    @Scenario(01)
    Scenario: [entity-replication-04] Component insert events fire for in-scope additions

    @Deferred
    @Scenario(02)
    Scenario: [entity-replication-05] Component remove events fire for in-scope removals

    @Deferred
    @Scenario(03)
    Scenario: [entity-replication-08] Replication preserves component-set parity

    @Deferred
    @Scenario(04)
    Scenario: [entity-replication-09] Replication preserves component value parity

    @Deferred
    @Scenario(05)
    Scenario: [entity-replication-10] No replication storms under steady-state

    @Deferred
    @Scenario(06)
    Scenario: [entity-replication-11] Component remove on out-of-scope is safe

    @Deferred
    @Scenario(07)
    Scenario: [entity-replication-12] Concurrent updates resolve to last-writer-wins

