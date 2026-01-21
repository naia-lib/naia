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

  @Rule(01)
  Rule: Entity Scopes

    # All executable scenarios deferred until step bindings implemented.


