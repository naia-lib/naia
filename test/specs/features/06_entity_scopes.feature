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

Feature: Entity Scopes

  Background:
    Given a Naia test environment is initialized

  # --------------------------------------------------------------------------
  # Rule: Rooms are required coarse gate for non-owners
  # --------------------------------------------------------------------------
  # NORMATIVE: SharesRoom(U,E) is necessary precondition for InScope(U,E)
  # for non-owners.
  # --------------------------------------------------------------------------
  Rule: Rooms are required coarse gate for non-owners

    Scenario: Entity not in any room is out-of-scope for non-owners
      Given a server with an entity in zero rooms
      And a non-owner client
      Then the entity is out-of-scope for the non-owner

    Scenario: Entity in shared room is in-scope for non-owner
      Given a server with an entity in a room
      And a non-owner client in the same room
      Then the entity is in-scope for the non-owner

  # --------------------------------------------------------------------------
  # Rule: Include and Exclude filters work additively after Rooms
  # --------------------------------------------------------------------------
  # NORMATIVE: Include/Exclude apply only if SharesRoom is true.
  # Last call wins.
  # --------------------------------------------------------------------------
  Rule: Include and Exclude filters work additively after Rooms

    Scenario: Exclude removes entity from scope even in shared room
      Given a server with an entity in a room
      And a non-owner client in the same room
      When the server excludes the entity for the client
      Then the entity is out-of-scope for the client

    Scenario: Last call wins between include and exclude
      Given a server with an entity and client in same room
      When the server excludes then includes the entity for the client
      Then the entity is in-scope for the client

  # --------------------------------------------------------------------------
  # Rule: Include without shared room cannot force scope
  # --------------------------------------------------------------------------
  # NORMATIVE: Include does not bypass the Rooms gate.
  # --------------------------------------------------------------------------
  Rule: Include without shared room cannot force scope

    Scenario: Include has no effect without shared room
      Given a server with an entity in room A
      And a client in room B
      When the server includes the entity for the client
      Then the entity is still out-of-scope for the client

  # --------------------------------------------------------------------------
  # Rule: Owning client is always in-scope for its client-owned entities
  # --------------------------------------------------------------------------
  # NORMATIVE: InScope(owner, entity) MUST always hold while connected.
  # Publication and scope filters MUST NOT remove from owner's scope.
  # --------------------------------------------------------------------------
  Rule: Owning client is always in-scope for its client-owned entities

    Scenario: Owning client retains visibility across scope operations
      Given a client-owned entity
      When the server attempts various scope operations
      Then the owning client never loses scope of the entity

    Scenario: Exclude on owner-owned entity has no effect
      Given a client-owned entity
      When the server excludes the entity for the owning client
      Then the entity remains in-scope for the owning client

  # --------------------------------------------------------------------------
  # Rule: OutOfScope causes despawn on client
  # --------------------------------------------------------------------------
  # NORMATIVE: When client becomes OutOfScope(U,E), entity MUST despawn
  # and all components (including local-only) MUST be destroyed.
  # --------------------------------------------------------------------------
  Rule: OutOfScope causes despawn on client

    Scenario: Leaving scope causes entity despawn
      Given an entity in-scope for a client
      When the entity leaves scope for that client
      Then the client observes entity despawn
      And all components including local-only are destroyed

  # --------------------------------------------------------------------------
  # Rule: Same-tick scope changes collapse to final state
  # --------------------------------------------------------------------------
  # NORMATIVE: Server resolves final scope state per tick. No intermediate
  # spawn/despawn transitions for flip-flops within one tick.
  # --------------------------------------------------------------------------
  Rule: Same-tick scope changes collapse to final state

    Scenario: Same-tick flip-flops collapse to final state
      Given an entity in-scope for a client
      When the server excludes then includes within the same tick
      Then the client observes no spawn or despawn

  # --------------------------------------------------------------------------
  # Rule: Leaving scope for at least one tick creates new lifetime on re-entry
  # --------------------------------------------------------------------------
  # NORMATIVE: If OutOfScope for ≥1 tick, re-entry is fresh spawn lifetime.
  # --------------------------------------------------------------------------
  Rule: Leaving scope for at least one tick creates new lifetime on re-entry

    Scenario: Re-entry after tick produces fresh spawn
      Given an entity in-scope for a client
      When the entity leaves scope
      And at least one tick passes
      And the entity re-enters scope
      Then the client receives a fresh spawn event

  # --------------------------------------------------------------------------
  # Rule: Disconnect implies OutOfScope for that user
  # --------------------------------------------------------------------------
  # NORMATIVE: On disconnect, OutOfScope(U,E) holds for all entities.
  # Server ceases replicating.
  # --------------------------------------------------------------------------
  Rule: Disconnect implies OutOfScope for that user

    Scenario: Disconnect stops all replication to that user
      Given a client with entities in scope
      When the client disconnects
      Then the server treats the user as out-of-scope for all entities
      And replication to that user ceases

# ============================================================================
# DEFERRED TESTS
# ============================================================================
# Items that cannot be tested with current harness capabilities.
# ============================================================================
#
# Rule: Late replication updates ignored for out-of-scope entities
#   Assertions:
#     - Updates for out-of-scope entities are dropped
#     - No partial state applied
#   Harness needs: Packet injection after scope change
#
# ============================================================================

# ============================================================================
# AMBIGUITIES + PROPOSED CLARIFICATIONS
# ============================================================================
# None identified.
# ============================================================================
