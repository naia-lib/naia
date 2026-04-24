# ============================================================================
# Scope Exit Policy — ScopeExit::Persist
# ============================================================================
# Source: contracts/15_scope_exit_policy.spec.md
# Last converted: 2026-04-23
#
# Summary:
#   Verifies that ScopeExit::Persist keeps the entity in the client's networked
#   entity pool when it leaves scope, freezes updates during absence, and
#   delivers accumulated deltas on re-entry.  Also verifies backward-compat:
#   the default ScopeExit is Despawn.
#
# Reused steps (defined in other step files):
#   - "a server is running"                              (common.rs)
#   - "a client connects"                               (common.rs)
#   - "the client and entity share a room"              (entity_scopes.rs)
#   - "the entity is in-scope for the client"           (entity_scopes.rs)
#   - "the server excludes the entity for the client"   (entity_scopes.rs)
#   - "the server includes the entity for the client"   (entity_scopes.rs)
#   - "the entity despawns on the client"               (entity_scopes.rs)
#   - "the client disconnects"                          (observability.rs)
#   - "the server stops replicating entities to that client" (entity_scopes.rs)
# ============================================================================

@Feature(scope_exit_policy)
Feature: Scope Exit Policy

  # --------------------------------------------------------------------------
  # Rule: Backward compatibility — default is Despawn
  # --------------------------------------------------------------------------
  # [scope-exit-01.t1]: ReplicationConfig::public() with no ScopeExit specified
  # MUST despawn entity on scope exit, preserving prior behavior.
  # --------------------------------------------------------------------------
  @Rule(01)
  Rule: Default ScopeExit is Despawn

    @Scenario(01)
    Scenario: scope-exit-01 — Default config entity is despawned when leaving scope
      Given a server is running
      And a client connects
      And a server-owned entity exists with a replicated component
      And the client and entity share a room
      And the entity is in-scope for the client
      When the server excludes the entity for the client
      Then the entity despawns on the client

  # --------------------------------------------------------------------------
  # Rule: Persist scope exit behavior
  # --------------------------------------------------------------------------
  # [scope-exit-02.t1]: entity with Persist must remain on client after scope loss
  # [scope-exit-03.t1]: no updates forwarded while Paused
  # [scope-exit-04.t1]: accumulated deltas delivered on re-entry
  # [scope-exit-04.t2]: no-mutation re-entry — entity present, no new spawn
  # --------------------------------------------------------------------------
  @Rule(02)
  Rule: Persist keeps entity on client when scope is lost

    @Scenario(01)
    Scenario: scope-exit-02 — Persist entity is not despawned when leaving scope
      Given a server is running
      And a client connects
      And a server-owned entity exists with ScopeExit::Persist configured
      And the client and entity share a room
      And the entity is in-scope for the client
      When the server excludes the entity for the client
      Then the client still has the entity

    @Scenario(02)
    Scenario: scope-exit-03-and-04 — Persist entity accumulates updates and delivers on re-entry
      Given a server is running
      And a client connects
      And a server-owned entity exists with ScopeExit::Persist configured
      And the client and entity share a room
      And the entity is in-scope for the client
      When the server excludes the entity for the client
      And the server updates the entity position to 100 100
      And the server advances 5 ticks
      Then the client entity position is still 0.0
      When the server includes the entity for the client
      Then the client entity position becomes 100.0

    @Scenario(03)
    Scenario: scope-exit-04-t2 — Persist entity re-entering scope with no mutations is still present
      Given a server is running
      And a client connects
      And a server-owned entity exists with ScopeExit::Persist configured
      And the client and entity share a room
      And the entity is in-scope for the client
      When the server excludes the entity for the client
      And the server advances 5 ticks
      When the server includes the entity for the client
      Then the client still has the entity

  # --------------------------------------------------------------------------
  # Rule: Global despawn during Persist absence
  # --------------------------------------------------------------------------
  # [scope-exit-05.t1]: global server despawn while Paused must reach client
  # --------------------------------------------------------------------------
  @Rule(03)
  Rule: Global despawn while Paused propagates to client

    @Scenario(01)
    Scenario: scope-exit-05 — Global despawn while Paused reaches client
      Given a server is running
      And a client connects
      And a server-owned entity exists with ScopeExit::Persist configured
      And the client and entity share a room
      And the entity is in-scope for the client
      When the server excludes the entity for the client
      And the server advances 2 ticks
      And the server globally despawns the entity
      Then the entity despawns on the client

  # --------------------------------------------------------------------------
  # Rule: Component lifecycle during Persist absence
  # --------------------------------------------------------------------------
  # [scope-exit-06.t1]: insert during absence visible on re-entry
  # [scope-exit-07.t1]: remove during absence absent on re-entry
  # --------------------------------------------------------------------------
  @Rule(04)
  Rule: Component lifecycle during absence is applied on re-entry

    @Scenario(01)
    Scenario: scope-exit-06 — Component inserted during Persist absence appears on re-entry
      Given a server is running
      And a client connects
      And a server-owned entity exists with ScopeExit::Persist configured without ImmutableLabel
      And the client and entity share a room
      And the entity is in-scope for the client
      When the server excludes the entity for the client
      And the server inserts ImmutableLabel on the entity
      When the server includes the entity for the client
      Then the client entity has ImmutableLabel

    @Scenario(02)
    Scenario: scope-exit-07 — Component removed during Persist absence is gone on re-entry
      Given a server is running
      And a client connects
      And a server-owned entity exists with ScopeExit::Persist configured with ImmutableLabel
      And the client and entity share a room
      And the entity is in-scope for the client
      When the server excludes the entity for the client
      And the server removes ImmutableLabel from the entity
      When the server includes the entity for the client
      Then the client entity does not have ImmutableLabel

  # --------------------------------------------------------------------------
  # Rule: Disconnect while Paused
  # --------------------------------------------------------------------------
  # [scope-exit-08.t1]: disconnect while Paused must not panic; state cleaned up
  # --------------------------------------------------------------------------
  @Rule(05)
  Rule: Disconnect while Paused cleans up without error

    @Scenario(01)
    Scenario: scope-exit-08 — Disconnect while Persist entity is Paused does not crash
      Given a server is running
      And a client connects
      And a server-owned entity exists with ScopeExit::Persist configured
      And the client and entity share a room
      And the entity is in-scope for the client
      When the server excludes the entity for the client
      And the server advances 2 ticks
      And the client disconnects
      Then the server stops replicating entities to that client
