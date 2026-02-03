# ============================================================================
# Entity Scopes — Canonical Contract
# ============================================================================
# Source: contracts/06_entity_scopes.spec.md
# Last converted: 2026-01-17
#
# Summary:
#   This specification defines whether a given Entity E is in-scope or
#   out-of-scope for a given User/Client U, and the required observable
#   consequences of scope transitions. Scoping uses Rooms as a coarse gate
#   plus per-user include/exclude filters.
#
# Terminology note:
#   This file is normative; scenarios are executable assertions; comments
#   labeled NORMATIVE are part of the contract.
# ============================================================================

# ============================================================================
# NORMATIVE CONTRACT MIRROR
# ============================================================================
#
# PURPOSE:
#   Define scope membership predicate, state machine, tick-level collapse,
#   and required behavior under reordering/illegal states.
#
# GLOSSARY:
#   - User U: Server-identified remote client/user (keyed by user_key)
#   - Entity E: Networked entity tracked by Naia replication
#   - Room: Server-managed grouping for coarse scope gating
#   - SharesRoom(U,E): True iff U and E share at least one common room
#   - Include(U,E): Per-user scope inclusion filter
#   - Exclude(U,E): Per-user scope exclusion filter
#   - Debug mode: debug_assertions enabled
#
# ----------------------------------------------------------------------------
# CORE SCOPE PREDICATE
# ----------------------------------------------------------------------------
#
# Rooms are required coarse gate for non-owners:
#   - SharesRoom(U,E) MUST be necessary precondition for InScope(U,E)
#   - Exception: owning client always in-scope for client-owned entities
#
# Per-user include/exclude is additive filter after Rooms:
#   - If SharesRoom(U,E) == true:
#     * Exclude(U,E) active → OutOfScope(U,E)
#     * Include(U,E) active → InScope(U,E) (subject to publication)
#     * Neither active → InScope(U,E) default (subject to publication)
#
# Include/Exclude ordering: last call wins:
#   - Most recently applied call for (U,E) determines effective state
#
# Roomless entities are out-of-scope for all non-owners:
#   - Entity in zero rooms → OutOfScope for all non-owners
#   - Include does not bypass Rooms gate
#
# ----------------------------------------------------------------------------
# COUPLING TO OWNERSHIP AND PUBLICATION
# ----------------------------------------------------------------------------
#
# Owning client is always in-scope for client-owned entities:
#   - InScope(A,E) MUST always hold while owner A is connected
#   - Publication and scope filters MUST NOT remove from owner's scope
#   - Exclude(owner, owned_entity) MUST be ignored or return error
#
# Publication can force non-owners out-of-scope:
#   - Unpublished/Private client-owned → OutOfScope for all non-owners
#
# ----------------------------------------------------------------------------
# SCOPE STATE MACHINE AND CLIENT-VISIBLE EFFECTS
# ----------------------------------------------------------------------------
#
#   - OutOfScope → despawn on that client
#   - Despawn destroys all components including local-only
#   - OutOfScope → ignore late replication updates
#   - InScope → entity exists in networked entity pool
#
# ----------------------------------------------------------------------------
# TICK SEMANTICS AND COLLAPSE
# ----------------------------------------------------------------------------
#
# Scope resolved per server tick; no intermediate states:
#   - Server collapses to final resolved state
#   - MUST NOT emit intermediate spawn/despawn
#
# Leaving scope for ≥1 tick creates new lifetime on re-entry:
#   - Fresh spawn semantics
#   - Client MUST NOT rely on prior lifetime state
#
# ----------------------------------------------------------------------------
# DISCONNECT AND ERROR HANDLING
# ----------------------------------------------------------------------------
#
# Disconnect implies OutOfScope for all entities:
#   - Server ceases replicating to disconnected client
#
# Illegal/misuse cases:
#   - Include without shared room cannot force scope
#   - Unknown entity/user references are ignored
#
# ============================================================================


@Feature(entity_scopes)
Feature: Entity Scopes

  # --------------------------------------------------------------------------
  # Rule: Rooms gating
  # --------------------------------------------------------------------------
  # SharesRoom(U,E) is a required precondition for InScope(U,E)
  # --------------------------------------------------------------------------
  @Rule(01)
  Rule: Rooms gating

    @Scenario(01)
    Scenario: Entity in shared room is in-scope for user
      Given a server is running
      And a client connects
      And a server-owned entity exists
      And the client and entity share a room
      Then the entity is in-scope for the client

    @Deferred
    @Scenario(02)
    Scenario: Entity not in shared room is out-of-scope for user
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
    Scenario: Exclude removes entity from user's scope
      Given a server is running
      And a client connects
      And a server-owned entity exists
      And the client and entity share a room
      And the entity is in-scope for the client
      When the server excludes the entity for the client
      Then the entity is out-of-scope for the client

    @Deferred
    @Scenario(02)
    Scenario: Include restores entity to user's scope after Exclude
      Given a server is running
      And a client connects
      And a server-owned entity exists
      And the client and entity share a room
      And the server excludes the entity for the client
      And the entity is out-of-scope for the client
      When the server includes the entity for the client
      Then the entity is in-scope for the client

    @Deferred
    @Scenario(03)
    Scenario: Last call wins between Include and Exclude
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
    Scenario: Owning client always sees own entity
      Given a server is running
      And a client connects
      And the client owns an entity
      Then the entity is in-scope for the client

    @Deferred
    @Scenario(02)
    Scenario: Exclude on owner's own entity has no effect
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
    Scenario: Roomless entity is out-of-scope for non-owners
      Given a server is running
      And a client connects
      And a server-owned entity exists
      And the entity is not in any room
      Then the entity is out-of-scope for the client

    @Deferred
    @Scenario(02)
    Scenario: Include cannot bypass room gate for roomless entity
      Given a server is running
      And a client connects
      And a server-owned entity exists
      And the entity is not in any room
      When the server includes the entity for the client
      Then the entity is out-of-scope for the client

  # --------------------------------------------------------------------------
  # Rule: Scope state effects
  # --------------------------------------------------------------------------
  # Scope transitions trigger observable client-side effects
  # --------------------------------------------------------------------------
  @Rule(05)
  Rule: Scope state effects

    @Scenario(01)
    Scenario: Entity despawns on client when leaving scope
      Given a server is running
      And a client connects
      And a server-owned entity exists
      And the client and entity share a room
      And the entity is in-scope for the client
      When the server excludes the entity for the client
      Then the entity despawns on the client

    @Deferred
    @Scenario(02)
    Scenario: Entity spawns on client when entering scope
      Given a server is running
      And a client connects
      And a server-owned entity exists
      And the client and entity share a room
      Then the entity spawns on the client

    @Deferred
    @Scenario(03)
    Scenario: Re-entering scope creates fresh entity lifetime
      Given a server is running
      And a client connects
      And a server-owned entity exists
      And the client and entity share a room
      And the entity is in-scope for the client
      When the server excludes the entity for the client
      And the entity despawns on the client
      And the server includes the entity for the client
      Then the entity spawns on the client as a new lifetime

  # --------------------------------------------------------------------------
  # Rule: Disconnect handling
  # --------------------------------------------------------------------------
  # Disconnect implies OutOfScope for that user for all entities
  # --------------------------------------------------------------------------
  @Rule(06)
  Rule: Disconnect handling

    @Scenario(01)
    Scenario: Disconnect implies out-of-scope for all entities
      Given a server is running
      And a client connects
      And a server-owned entity exists
      And the client and entity share a room
      And the entity is in-scope for the client
      When the client disconnects
      Then the server stops replicating entities to that client

    @Deferred
    @Scenario(02)
    Scenario: Operations on unknown user are ignored
      Given a server is running
      And a server-owned entity exists
      When the server includes the entity for an unknown client
      Then no error is raised

    @Deferred
    @Scenario(03)
    Scenario: Operations on unknown entity are ignored
      Given a server is running
      And a client connects
      When the server includes an unknown entity for the client
      Then no error is raised
