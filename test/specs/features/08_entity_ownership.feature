# ============================================================================
# Entity Ownership — Canonical Contract
# ============================================================================
# Source: contracts/08_entity_ownership.spec.md
# Last converted: 2026-01-17
#
# Summary:
#   This specification defines Entity Ownership: which actor is permitted to
#   write replicated state for an Entity. Ownership is per-entity, exclusive,
#   and distinct from Delegation and Authority.
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
#   Define the coarse per-entity "who may write replicated updates" rule.
#
# GLOSSARY:
#   - Mutate: Change local world state (insert/remove/update components)
#   - Write: Cause a mutation to be replicated over the wire
#   - Replicated component: Component type registered for replication
#   - Local-only component: Component present only locally
#   - Owner: Per-entity, exclusive, queryable via entity(...).owner()
#   - EntityOwner::Server: Server-owned entity
#   - EntityOwner::Client(UserKey): Client-owned entity
#   - EntityOwner::Local: Local-only entity (never networked)
#
# ----------------------------------------------------------------------------
# CORE OWNERSHIP RULES
# ----------------------------------------------------------------------------
#
# Ownership is per-entity, exclusive, not per-component:
#   - Entity MUST have exactly one owner at any moment
#
# Server accepts writes only from owning client:
#   - For client-owned entity, server MUST accept writes only from owner
#   - Server MAY ignore unauthorized writes silently
#
# Server rejects writes for non-delegated server-owned:
#   - For non-delegated server-owned entity, no client writes accepted
#
# Ownership alone does not emit authority events:
#   - Authority events are part of Delegation/Authority, not Ownership
#
# ----------------------------------------------------------------------------
# CLIENT-SIDE WRITE PERMISSION
# ----------------------------------------------------------------------------
#
# Client write permission rules:
#   - Client MUST NOT write unless:
#     * owner(E) == Client(this_client), OR
#     * replication_config(E) == Delegated AND authority ∈ {Granted, Releasing}
#   - API returns Err for unauthorized write attempts
#   - Internal invariant violation → panic
#
# Ownership visibility on client is coarse:
#   - Entities not owned by this client → reported as EntityOwner::Server
#   - Client-owned by this client → EntityOwner::Client
#   - Local-only → EntityOwner::Local
#
# ----------------------------------------------------------------------------
# MUTATE VS WRITE BEHAVIOR
# ----------------------------------------------------------------------------
#
# Non-owners may mutate locally but must never write:
#   - Local mutations persist until server overwrites
#   - No outbound replication for non-owned entities
#
# Local-only components persist until despawn or overwrite:
#   - Server replication overwrites with Insert event
#
# Removing server-replicated components from unowned:
#   - Client MAY remove local-only component
#   - Removing server-replicated component → Err
#
# ----------------------------------------------------------------------------
# OWNERSHIP TRANSITIONS
# ----------------------------------------------------------------------------
#
#   - Server-owned entities never migrate to client-owned
#   - Client-owned entities may migrate to server-owned delegated
#     (requires delegation enabled, transfers ownership to server)
#   - Cannot revert back to client ownership
#
# Owning client always in-scope for its entities
#
# ----------------------------------------------------------------------------
# DISCONNECT HANDLING
# ----------------------------------------------------------------------------
#
# Owner disconnect despawns all client-owned entities
#
# Out-of-scope write attempts:
#   - Internal attempt → panic (framework invariant)
#
# ============================================================================


Feature: Entity Ownership

  # All executable scenarios deferred until step bindings implemented.

