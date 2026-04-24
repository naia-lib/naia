# ============================================================================
# Scope Propagation Model — Push-based scope-change tracking
# ============================================================================
# Source: contracts/16_scope_propagation_model.spec.md
# Last converted: 2026-04-23
#
# Summary:
#   Regression-guards the push-based scope-change queue (v2_push_pipeline)
#   against the behavioral contract for scope propagation.  All obligations
#   hold on both the legacy full-scan path and the new queue-drain path.
#
# Reused steps (defined in other step files):
#   - "a server is running"                                     (common.rs)
#   - "a client connects"                                       (common.rs)
#   - "a server-owned entity exists with a replicated component" (entity_replication.rs)
#   - "the client and entity share a room"                      (entity_scopes.rs)
#   - "the entity is in-scope for the client"                   (entity_scopes.rs)
#   - "the server excludes the entity for the client"           (entity_scopes.rs)
#   - "the server includes the entity for the client"           (entity_scopes.rs)
#   - "the entity despawns on the client"                       (entity_scopes.rs)
#   - "the server includes an unknown entity for the client"    (entity_scopes.rs)
#   - "no error is raised"                                      (entity_scopes.rs)
#   - "the server advances {int} ticks"                         (scope_exit.rs)
# ============================================================================

@Feature(scope_propagation_model)
Feature: Scope Propagation Model

  # --------------------------------------------------------------------------
  # Rule: Scope-change outcomes are identical to the eager-scan path
  # --------------------------------------------------------------------------
  # [scope-propagation-01.t1]: include/exclude/room-add produce identical
  # outcomes under both the legacy scan path and the push-based queue.
  # --------------------------------------------------------------------------
  @Rule(01)
  Rule: Scope changes produce correct outcomes

    @Scenario(01)
    Scenario: scope-propagation-01-a — include adds entity to client scope
      Given a server is running
      And a client connects
      And a server-owned entity exists with a replicated component
      And the client and entity share a room
      And the server excludes the entity for the client
      When the server includes the entity for the client
      Then the entity is in-scope for the client

    @Scenario(02)
    Scenario: scope-propagation-01-b — exclude removes entity from client scope
      Given a server is running
      And a client connects
      And a server-owned entity exists with a replicated component
      And the client and entity share a room
      And the entity is in-scope for the client
      When the server excludes the entity for the client
      Then the entity despawns on the client

    @Scenario(03)
    Scenario: scope-propagation-01-c — entity added to room becomes visible to existing user
      Given a server is running
      And a client connects
      And a server-owned entity exists with a replicated component
      When the server adds the entity to the client's room
      Then the entity is in-scope for the client

  # --------------------------------------------------------------------------
  # Rule: Idle tick has zero queue depth
  # --------------------------------------------------------------------------
  # [scope-propagation-02.t2]: scope_change_queue is empty after an idle tick.
  # --------------------------------------------------------------------------
  @Rule(02)
  Rule: Idle tick leaves scope change queue empty

    @Scenario(01)
    Scenario: scope-propagation-02 — scope change queue is empty after idle ticks
      Given a server is running
      And a client connects
      And a server-owned entity exists with a replicated component
      And the client and entity share a room
      And the entity is in-scope for the client
      When the server advances 3 ticks
      Then the scope change queue depth is 0

  # --------------------------------------------------------------------------
  # Rule: Queue drains within the tick
  # --------------------------------------------------------------------------
  # [scope-propagation-03.t3]: scope_change_queue is empty after a tick that
  # contained scope-change API calls.
  # --------------------------------------------------------------------------
  @Rule(03)
  Rule: Scope changes drain within the same tick

    @Scenario(01)
    Scenario: scope-propagation-03 — queue is empty after a tick with scope changes
      Given a server is running
      And a client connects
      And a server-owned entity exists with a replicated component
      And the client and entity share a room
      When the server excludes the entity for the client
      Then the scope change queue depth is 0

  # --------------------------------------------------------------------------
  # Rule: Unknown entity is a silent no-op
  # --------------------------------------------------------------------------
  # [scope-propagation-04.t4]: include for non-existent entity is a safe no-op.
  # --------------------------------------------------------------------------
  @Rule(04)
  Rule: Scope API calls for unknown entities are silent no-ops

    @Scenario(01)
    Scenario: scope-propagation-04 — include for unknown entity does not crash
      Given a server is running
      And a client connects
      When the server includes an unknown entity for the client
      Then no error is raised
