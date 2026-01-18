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

  Background:
    Given a Naia test environment is initialized

  # --------------------------------------------------------------------------
  # Rule: Ownership is per-entity and exclusive
  # --------------------------------------------------------------------------
  # NORMATIVE: Entity MUST have exactly one owner at any moment.
  # --------------------------------------------------------------------------
  Rule: Ownership is per-entity and exclusive

    Scenario: Entity has exactly one owner
      Given a server with an entity
      When querying the entity owner
      Then exactly one owner is returned

  # --------------------------------------------------------------------------
  # Rule: Server accepts writes only from owning client
  # --------------------------------------------------------------------------
  # NORMATIVE: For client-owned entity, server accepts writes only from owner.
  # --------------------------------------------------------------------------
  Rule: Server accepts writes only from owning client

    Scenario: Unauthorized client write attempts are rejected
      Given a client-owned entity with owner A
      And another client B
      When client B attempts to write to the entity
      Then the server does not apply the write

  # --------------------------------------------------------------------------
  # Rule: Non-delegated server-owned entities reject client writes
  # --------------------------------------------------------------------------
  # NORMATIVE: For non-delegated server-owned entity, no client writes.
  # --------------------------------------------------------------------------
  Rule: Non-delegated server-owned entities reject client writes

    Scenario: Client writes to non-delegated server-owned entity are ignored
      Given a server-owned entity that is not delegated
      When a client attempts to write to the entity
      Then the server does not apply the write

  # --------------------------------------------------------------------------
  # Rule: Client write permission is enforced
  # --------------------------------------------------------------------------
  # NORMATIVE: Client MUST NOT write unless owner or authority holder.
  # API returns Err for unauthorized attempts.
  # --------------------------------------------------------------------------
  Rule: Client write permission is enforced

    Scenario: User API call to write unowned entity returns Err
      Given a client with an entity it does not own
      When the client attempts to write to that entity via API
      Then the API returns an Err result

  # --------------------------------------------------------------------------
  # Rule: Non-owners may mutate locally but not write
  # --------------------------------------------------------------------------
  # NORMATIVE: Local mutations persist until server overwrites.
  # No outbound replication for non-owned.
  # --------------------------------------------------------------------------
  Rule: Non-owners may mutate locally but not write

    Scenario: Local mutation on non-owned entity persists until server update
      Given a client with a non-owned entity
      When the client mutates the entity locally
      Then the mutation persists locally
      And no outbound replication occurs

    Scenario: Server update overwrites local mutation
      Given a client with a local mutation on a non-owned entity
      When the server sends an update
      Then the server state overwrites the local mutation

  # --------------------------------------------------------------------------
  # Rule: Removing server-replicated component from unowned returns Err
  # --------------------------------------------------------------------------
  # NORMATIVE: Client MAY remove local-only. Removing server-replicated → Err.
  # --------------------------------------------------------------------------
  Rule: Removing server-replicated component from unowned returns Err

    Scenario: Removing server-replicated component returns Err
      Given a client with a non-owned entity having a server-replicated component
      When the client attempts to remove the replicated component
      Then the API returns an Err result

  # --------------------------------------------------------------------------
  # Rule: Server-owned entities never migrate to client-owned
  # --------------------------------------------------------------------------
  # NORMATIVE: Server-owned MUST NOT transition to client-owned.
  # --------------------------------------------------------------------------
  Rule: Server-owned entities never migrate to client-owned

    Scenario: Server-owned entity cannot become client-owned
      Given a server-owned entity
      Then there is no operation to make it client-owned

  # --------------------------------------------------------------------------
  # Rule: Client-owned may migrate to server-owned delegated
  # --------------------------------------------------------------------------
  # NORMATIVE: Delegation enabled transfers ownership to server.
  # Cannot revert.
  # --------------------------------------------------------------------------
  Rule: Client-owned may migrate to server-owned delegated

    Scenario: Enabling delegation transfers ownership to server
      Given a published client-owned entity
      When delegation is enabled on the entity
      Then ownership transfers to the server

    Scenario: Delegated entity cannot revert to client ownership
      Given a client-owned entity that became delegated
      Then it cannot revert to client ownership

  # --------------------------------------------------------------------------
  # Rule: Owning client always in-scope for its entities
  # --------------------------------------------------------------------------
  # NORMATIVE: Owning client never loses scope of owned entities.
  # --------------------------------------------------------------------------
  Rule: Owning client always in-scope for its entities

    Scenario: Owning client retains owned entities across scope changes
      Given a client-owned entity
      When scope operations occur
      Then the owning client never receives despawn for that entity

  # --------------------------------------------------------------------------
  # Rule: Owner disconnect despawns all client-owned entities
  # --------------------------------------------------------------------------
  # NORMATIVE: When owner disconnects, all their entities are despawned.
  # --------------------------------------------------------------------------
  Rule: Owner disconnect despawns all client-owned entities

    Scenario: Client disconnect despawns all client-owned entities
      Given a client with owned entities
      When the client disconnects
      Then all client-owned entities are despawned on the server

    Scenario: Other clients observe despawn for disconnected clients entities
      Given client A with owned entities visible to client B
      When client A disconnects
      Then client B observes despawn for those entities

# ============================================================================
# DEFERRED TESTS
# ============================================================================
# Items that cannot be tested with current harness capabilities.
# ============================================================================
#
# Rule: Rollback of client writes on server rejection
#   Assertions:
#     - Client writes rolled back when server rejects authority
#     - Rollback is atomic and consistent
#   Harness needs: Server-side authority rejection injection + client state inspection
#
# Rule: Race condition resolution between ownership and scope changes
#   Assertions:
#     - Ownership cleanup correct when scope changes mid-transfer
#   Harness needs: Precise timing control of concurrent operations
#
# ============================================================================

# ============================================================================
# AMBIGUITIES + PROPOSED CLARIFICATIONS
# ============================================================================
# None identified. The entity ownership spec is comprehensive.
# ============================================================================
