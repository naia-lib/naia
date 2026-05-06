# ============================================================================
# Entity Scopes, Scope-Exit Policy, Scope Propagation, Update Candidate Set — Grouped Contract Suite
# ============================================================================
# Post-A.4 grouping of multiple source feature files. Each source's content
# is preserved verbatim from the @Rule line onward; per-source separators
# (`# === Source: ... ===`) keep the original boundaries greppable. Free-text
# feature-description blocks from sources are stripped (gherkin only allows
# them under the top-level Feature:). @Rule/@Scenario tag numbers are
# renumbered globally within this file (each source's local 01, 02, ...
# becomes a continuous sequence) so namako sees no duplicate-tag collisions.
# ============================================================================

@Feature(visibility)
Feature: Entity Scopes, Scope-Exit Policy, Scope Propagation, Update Candidate Set


  # Auto-applied prelude — every Scenario in this file gets this
  # Given run before its own Givens (idempotent).
  Background:
    Given a server is running

  # ==========================================================================
  # === Source: 06_entity_scopes.feature ===
  # ==========================================================================

  @Rule(01)
  Rule: Rooms gating

    @Scenario(01)
    Scenario: [entity-scopes-01] Entity in shared room is in-scope for user
      Given a server is running
      And a client connects
      And a server-owned entity exists
      And the client and entity share a room
      Then the entity is in-scope for the client

    # [entity-scopes-02] — Entity not in shared room is out-of-scope for user
    # SharesRoom(U,E) MUST be necessary for InScope(U,E); an entity with no
    # shared room MUST remain out-of-scope.
    @Scenario(02)
    Scenario: [entity-scopes-02] Entity not in shared room is out-of-scope for user
      Given a server is running
      And a client connects
      And a server-owned entity exists
      And the client and entity do not share a room
      Then the entity is out-of-scope for the client

  # --------------------------------------------------------------------------
  # Rule: Include/Exclude filter
  # --------------------------------------------------------------------------
  # Per-user include/exclude filter applies after Rooms gate
  # --------------------------------------------------------------------------
  @Rule(02)
  Rule: Include/Exclude filter

    @Scenario(01)
    Scenario: [entity-scopes-03] Exclude removes entity from user's scope
      Given a server is running
      And a client connects
      And a server-owned entity exists
      And the client and entity share a room
      And the entity is in-scope for the client
      When the server excludes the entity for the client
      Then the entity is out-of-scope for the client

    @Scenario(02)
    Scenario: [entity-scopes-04] Include restores entity to user's scope after Exclude
      Given a server is running
      And a client connects
      And a server-owned entity exists
      And the client and entity share a room
      And the server excludes the entity for the client
      And the entity is out-of-scope for the client
      When the server includes the entity for the client
      Then the entity is in-scope for the client

    # [entity-scopes-05] — Last call wins between Include and Exclude
    # The most recently applied include/exclude call for (U,E) determines the
    # effective scope state. Include after Exclude → entity is back in scope.
    @Scenario(03)
    Scenario: [entity-scopes-05] Last call wins between Include and Exclude
      Given a server is running
      And a client connects
      And a server-owned entity exists
      And the client and entity share a room
      And the entity is in-scope for the client
      When the server excludes the entity for the client
      And the server includes the entity for the client
      Then the entity is in-scope for the client

  # --------------------------------------------------------------------------
  # Rule: Owner scope invariant
  # --------------------------------------------------------------------------
  # Owning client always in-scope for its client-owned entities
  # --------------------------------------------------------------------------
  @Rule(03)
  Rule: Owner scope invariant

    @Scenario(01)
    Scenario: [entity-scopes-06] Owning client always sees own entity
      Given a server is running
      And a client connects
      And the client owns an entity
      Then the entity is in-scope for the client

    # [entity-scopes-owner-02] — Exclude on owner's own entity has no effect
    # The owning client MUST always remain in-scope for their entity.
    # server.exclude() on an owner's entity MUST be ignored.
    @Scenario(02)
    Scenario: [entity-scopes-07] Exclude on owner's own entity has no effect
      Given a server is running
      And a client connects
      And the client owns an entity
      When the server excludes the entity for the client
      Then the entity is in-scope for the client

  # --------------------------------------------------------------------------
  # Rule: Roomless entities
  # --------------------------------------------------------------------------
  # Entities in zero rooms are out-of-scope for all non-owners
  # --------------------------------------------------------------------------
  @Rule(04)
  Rule: Roomless entities

    @Scenario(01)
    Scenario: [entity-scopes-08] Roomless entity is out-of-scope for non-owners
      Given a server is running
      And a client connects
      And a server-owned entity exists
      And the entity is not in any room
      Then the entity is out-of-scope for the client

    # [entity-scopes-roomless-02] — Include cannot bypass room gate for roomless entity
    # Include filter MUST NOT bypass the Rooms gate; a roomless entity MUST remain
    # out-of-scope even after explicit include.
    # DEFERRED: naia currently allows scope.include() to bypass the room gate.
    @Deferred
    @Scenario(02)
    Scenario: [entity-scopes-09] Include cannot bypass room gate for roomless entity
      Given a server is running
      And a client connects
      And a server-owned entity exists
      And the entity is not in any room
      When the server includes the entity for the client
      Then the entity is out-of-scope for the client

    # [entity-scopes-08] — Room entry and exit control client visibility lifecycle
    # Entering a room (+ include) makes an entity visible; leaving scope (exclude)
    # removes it from the client without destroying it on the server; true despawn
    # removes it from the server entirely. These three events are distinct.
    @Scenario(03)
    Scenario: [entity-scopes-08] Room entry and exit control client visibility lifecycle
      Given a server is running
      And a client connects
      And a server-owned entity exists with a replicated component
      Then the entity is out-of-scope for the client
      When the server includes the entity for the client
      And the server adds the entity to the client's room
      Then the entity spawns on the client
      When the server excludes the entity for the client
      Then the entity despawns on the client
      When the server globally despawns the entity
      Then the server no longer has the entity

  # --------------------------------------------------------------------------
  # Rule: Scope state effects
  # --------------------------------------------------------------------------
  # Scope transitions trigger observable client-side effects
  # --------------------------------------------------------------------------
  @Rule(05)
  Rule: Scope state effects

    @Scenario(01)
    Scenario: [entity-scopes-10] Entity despawns on client when leaving scope
      Given a server is running
      And a client connects
      And a server-owned entity exists
      And the client and entity share a room
      And the entity is in-scope for the client
      When the server excludes the entity for the client
      Then the entity despawns on the client

    @Scenario(02)
    Scenario: [entity-scopes-11] Entity spawns on client when entering scope
      Given a server is running
      And a client connects
      And a server-owned entity exists
      And the client and entity share a room
      Then the entity spawns on the client

    # [entity-scopes-lifetime-03] — Re-entering scope creates fresh entity lifetime
    # After leaving scope for ≥1 tick, re-entry MUST produce a fresh spawn with
    # the current server snapshot; the client MUST NOT rely on prior state.
    @Scenario(03)
    Scenario: [entity-scopes-12] Re-entering scope creates fresh entity lifetime
      Given a server is running
      And a client connects
      And a server-owned entity exists
      And the client and entity share a room
      And the entity is in-scope for the client
      When the server excludes the entity for the client
      And the entity despawns on the client
      And the server includes the entity for the client
      Then the entity spawns on the client as a new lifetime

    # [entity-scopes-15] — Scope leave is reversible; despawn is permanent
    # When an entity leaves a client's scope it is hidden but still alive on the
    # server. Re-including the entity restores visibility. True despawn removes
    # the entity from the server entirely and cannot be reversed.
    @Scenario(04)
    Scenario: [entity-scopes-15] Scope leave is reversible; true despawn is permanent
      Given a server is running
      And a client connects
      And a server-owned entity exists
      And the client and entity share a room
      And the entity is in-scope for the client
      When the server excludes the entity for the client
      Then the entity is out-of-scope for the client
      When the server includes the entity for the client
      Then the entity is in-scope for the client
      When the server globally despawns the entity
      Then the server no longer has the entity

  # --------------------------------------------------------------------------
  # Rule: Disconnect handling
  # --------------------------------------------------------------------------
  # Disconnect implies OutOfScope for that user for all entities
  # --------------------------------------------------------------------------
  @Rule(06)
  Rule: Disconnect handling

    @Scenario(01)
    Scenario: [entity-scopes-13] Disconnect implies out-of-scope for all entities
      Given a server is running
      And a client connects
      And a server-owned entity exists
      And the client and entity share a room
      And the entity is in-scope for the client
      When the client disconnects
      Then the server stops replicating entities to that client

    @Scenario(02)
    Scenario: [entity-scopes-14] Operations on unknown user are ignored
      Given a server is running
      And a server-owned entity exists
      When the server includes the entity for an unknown client
      Then no error is raised

    # [entity-scopes-error-03] — Operations on unknown entity are ignored
    # scope.include() on a non-existent entity MUST be a silent no-op (no panic).
    @Scenario(03)
    Scenario: [entity-scopes-15] Operations on unknown entity are ignored
      Given a server is running
      And a client connects
      When the server includes an unknown entity for the client
      Then no error is raised

  # ==========================================================================
  # === Source: 15_scope_exit_policy.feature ===
  # ==========================================================================

  @Rule(07)
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
  @Rule(08)
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
  @Rule(09)
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
  @Rule(10)
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
  @Rule(11)
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

  # ==========================================================================
  # === Source: 16_scope_propagation_model.feature ===
  # ==========================================================================

  @Rule(12)
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
  @Rule(13)
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
  @Rule(14)
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
  @Rule(15)
  Rule: Scope API calls for unknown entities are silent no-ops

    @Scenario(01)
    Scenario: scope-propagation-04 — include for unknown entity does not crash
      Given a server is running
      And a client connects
      When the server includes an unknown entity for the client
      Then no error is raised

  # ==========================================================================
  # === Source: 17_update_candidate_set.feature ===
  # ==========================================================================

  @Rule(16)
  Rule: Idle entity produces no dirty update candidates

    @Scenario(01)
    Scenario: update-candidate-01-a — idle tick leaves dirty count at zero
      Given a server is running
      And a client connects
      And a server-owned entity exists with a replicated component
      And the client and entity share a room
      And the entity is in-scope for the client
      When the server performs an idle tick
      Then the total dirty update candidate count is 0

  # --------------------------------------------------------------------------
  # Rule: Mutation candidate drains in tick
  # After a mutation + tick, the dirty set is back at 0 and the update landed.
  # --------------------------------------------------------------------------
  @Rule(17)
  Rule: Mutation candidate drains in tick

    @Scenario(01)
    Scenario: update-candidate-02-a — mutation drains and update reaches client
      Given a server is running
      And a client connects
      And a server-owned entity exists with a replicated component
      And the client and entity share a room
      And the entity is in-scope for the client
      When the server updates the replicated component
      Then the total dirty update candidate count is 0
      And the client observes the component update

  # --------------------------------------------------------------------------
  # Rule: Out-of-scope mutation produces no dirty candidate
  # --------------------------------------------------------------------------
  @Rule(18)
  Rule: Out-of-scope mutation produces no dirty candidate

    @Scenario(01)
    Scenario: update-candidate-03-a — mutation on entity not in shared room produces no candidate
      Given a server is running
      And a client connects
      And a server-owned entity exists with a replicated component
      And the entity is not in the client's room
      When the server updates the replicated component
      Then the total dirty update candidate count is 0

