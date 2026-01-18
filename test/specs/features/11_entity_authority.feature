# ============================================================================
# Entity Authority — Canonical Contract
# ============================================================================
# Source: contracts/11_entity_authority.spec.md
# Last converted: 2026-01-17
#
# Summary:
#   This specification defines how a client can acquire and release the right
#   to write replicated updates for a server-owned delegated entity, and what
#   each side can observe about that right. Defines the EntityAuthStatus state
#   machine and can_write/can_read derived capabilities.
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
#   Define the authority state machine (EntityAuthStatus), client request/
#   release semantics, server-controlled authority, and required behavior.
#
# GLOSSARY:
#   - EntityAuthStatus (client-visible):
#     * Available: No client holds authority; may request
#     * Requested: Client requested, awaiting server decision (optimistic)
#     * Granted: Client holds authority
#     * Releasing: Client initiated release, awaiting confirmation
#     * Denied: Authority held by another client or server
#   - can_write: True iff endpoint owns entity OR is active authority holder
#   - can_read: True iff endpoint is NOT the active authority holder
#
# CORE CONTRACTS:
#   [entity-authority-01] Authority defined only for delegated entities
#     - If replication_config(E) != Delegated, authority(E) MUST be None
#     - Request/release on non-delegated MUST return error
#   [entity-authority-02] Single-writer rule (client-side)
#     - Client MAY write only when Granted or Releasing
#     - Writing in other states MUST panic
#   [entity-authority-03] Meaning of Denied
#     - Authority held by another client OR server
#     - Transitions to Available when holder releases or server resets
#
# CLIENT API SEMANTICS:
#   [entity-authority-04] request_authority() is optimistic
#     - Available → Requested immediately (no round-trip wait)
#   [entity-authority-05] Request completion transitions
#     - Requested → Granted (server grants)
#     - Requested → Denied (another holds)
#     - Requested → Available (server resets)
#   [entity-authority-06] release_authority() transitions
#     - Granted → Releasing → Available
#     - Requested → Available (cancels request)
#   [entity-authority-07] Client-side error returns
#     - Not delegated → ErrNotDelegated
#     - Out-of-scope → ErrNotInScope
#     - Entity doesn't exist → ErrNoSuchEntity
#
# SERVER SEMANTICS:
#   [entity-authority-08] First-request wins arbitration
#   [entity-authority-09] Server may hold authority and block clients
#     - All clients in Denied while server holds
#   [entity-authority-10] Server override/reset
#     - Granted/Releasing/Denied → Available (revoked)
#     - Requested → Available (cleared)
#
# SCOPE/LIFETIME/DISCONNECT:
#   [entity-authority-11] Out-of-scope ends authority for that client
#     - Entity lifetime ends, status cleared, buffered actions discarded
#   [entity-authority-12] Authority holder losing scope forces global release
#     - Other in-scope clients: Denied → Available
#   [entity-authority-13] Delegation disable clears authority
#     - Authority becomes None, pending cleared, grants revoked
#
# ROBUSTNESS:
#   [entity-authority-14] Out-of-scope requests ignored server-side
#     - Silent in production, MAY warn in Debug mode
#   [entity-authority-15] Duplicate/late signals are idempotent
#     - No duplicate effects, converge to final state
#
# OBSERVABILITY:
#   [entity-authority-16] Authority observable via status and events
#
# STATE TRANSITION TABLE:
#   Available + request_authority() → Requested (can_write=false, can_read=true)
#   Requested + Server grants → Granted (can_write=true, can_read=false)
#   Requested + Server denies → Denied (can_write=false, can_read=true)
#   Requested + Server resets → Available (can_write=false, can_read=true)
#   Granted + release_authority() → Releasing (can_write=true, can_read=false)
#   Granted + Server resets → Available (can_write=false, can_read=true)
#   Granted + Lose scope → (cleared)
#   Releasing + Server confirms → Available (can_write=false, can_read=true)
#   Denied + Holder releases → Available (can_write=false, can_read=true)
#   Denied + Server resets → Available (can_write=false, can_read=true)
#
# ============================================================================

Feature: Entity Authority

  Background:
    Given a Naia test environment is initialized

  # --------------------------------------------------------------------------
  # Rule: Authority is defined only for delegated entities
  # --------------------------------------------------------------------------
  # NORMATIVE: If replication_config(E) != Delegated, authority(E) MUST be
  # None. Request/release on non-delegated MUST return error.
  # --------------------------------------------------------------------------
  Rule: Authority is defined only for delegated entities

    Scenario: Non-delegated entity has no authority
      Given a server-owned entity that is not delegated
      Then authority status is None

    Scenario: Request on non-delegated returns error
      Given a server-owned entity that is not delegated
      When the client calls request_authority
      Then an error is returned

  # --------------------------------------------------------------------------
  # Rule: Single-writer rule enforced via panic
  # --------------------------------------------------------------------------
  # NORMATIVE: Client MAY write only when Granted or Releasing. Writing in
  # Available, Requested, or Denied MUST panic.
  # --------------------------------------------------------------------------
  Rule: Single-writer rule enforced via panic

    Scenario: Writing while Available would panic
      Given a client with Available status for a delegated entity
      Then attempting to write would trigger a panic

    Scenario: Writing while Requested would panic
      Given a client with Requested status for a delegated entity
      Then attempting to write would trigger a panic

    Scenario: Writing while Denied would panic
      Given a client with Denied status for a delegated entity
      Then attempting to write would trigger a panic

    Scenario: Writing while Granted succeeds
      Given a client with Granted status for a delegated entity
      When the client writes to the entity
      Then the write is accepted

    Scenario: Writing while Releasing succeeds
      Given a client with Releasing status for a delegated entity
      When the client writes to the entity
      Then the write is accepted

  # --------------------------------------------------------------------------
  # Rule: can_read is false for authority holder
  # --------------------------------------------------------------------------
  # NORMATIVE: can_read is true iff endpoint is NOT the active authority
  # holder. Authority holder MUST NOT apply incoming replicated updates.
  # --------------------------------------------------------------------------
  Rule: can_read is false for authority holder

    Scenario: Granted client has can_read false
      Given a client with Granted status
      Then can_read is false for that entity

    Scenario: Releasing client has can_read false
      Given a client with Releasing status
      Then can_read is false for that entity

    Scenario: Available client has can_read true
      Given a client with Available status
      Then can_read is true for that entity

  # --------------------------------------------------------------------------
  # Rule: request_authority is optimistic
  # --------------------------------------------------------------------------
  # NORMATIVE: Available → Requested immediately without round-trip.
  # --------------------------------------------------------------------------
  Rule: request_authority is optimistic

    Scenario: Request transitions immediately to Requested
      Given a client with Available status for a delegated entity
      When the client calls request_authority
      Then the client immediately has Requested status

  # --------------------------------------------------------------------------
  # Rule: Request completion transitions
  # --------------------------------------------------------------------------
  # NORMATIVE: Requested → Granted/Denied/Available based on server decision.
  # --------------------------------------------------------------------------
  Rule: Request completion transitions

    Scenario: Server grants authority
      Given a client with Requested status
      When the server grants authority
      Then the client has Granted status

    Scenario: Server denies because another holds
      Given a client with Requested status
      And another client holds authority
      When the server processes the request
      Then the client has Denied status

    Scenario: Server resets clears request
      Given a client with Requested status
      When the server resets authority
      Then the client has Available status

  # --------------------------------------------------------------------------
  # Rule: release_authority transitions
  # --------------------------------------------------------------------------
  # NORMATIVE: Granted → Releasing → Available. Requested → Available.
  # --------------------------------------------------------------------------
  Rule: release_authority transitions

    Scenario: Release from Granted goes to Releasing
      Given a client with Granted status
      When the client calls release_authority
      Then the client has Releasing status

    Scenario: Releasing finalizes to Available
      Given a client with Releasing status
      When the server confirms release
      Then the client has Available status

    Scenario: Release from Requested cancels request
      Given a client with Requested status
      When the client calls release_authority
      Then the client has Available status

  # --------------------------------------------------------------------------
  # Rule: Client-side error returns
  # --------------------------------------------------------------------------
  # NORMATIVE: Errors for not-delegated, out-of-scope, no-such-entity.
  # --------------------------------------------------------------------------
  Rule: Client-side error returns

    Scenario: Request on out-of-scope entity returns error
      Given a delegated entity out of scope for the client
      When the client calls request_authority
      Then an ErrNotInScope error is returned

    Scenario: Request on non-existent entity returns error
      Given no entity exists with that ID
      When the client calls request_authority
      Then an ErrNoSuchEntity error is returned

  # --------------------------------------------------------------------------
  # Rule: Server may hold authority and block clients
  # --------------------------------------------------------------------------
  # NORMATIVE: When server holds authority, all clients are Denied.
  # --------------------------------------------------------------------------
  Rule: Server may hold authority and block clients

    Scenario: Server holding authority blocks all clients
      Given the server holds authority for a delegated entity
      Then all clients have Denied status for that entity

  # --------------------------------------------------------------------------
  # Rule: Server override/reset clears all states
  # --------------------------------------------------------------------------
  # NORMATIVE: Server reset transitions all clients to Available.
  # --------------------------------------------------------------------------
  Rule: Server override/reset clears all states

    Scenario: Server reset revokes Granted
      Given a client with Granted status
      When the server resets authority
      Then the client has Available status

    Scenario: Server reset clears Denied
      Given a client with Denied status
      When the server resets authority
      Then the client has Available status

  # --------------------------------------------------------------------------
  # Rule: Scope loss ends authority
  # --------------------------------------------------------------------------
  # NORMATIVE: Out-of-scope clears authority status; holder loss triggers
  # global release.
  # --------------------------------------------------------------------------
  Rule: Scope loss ends authority

    Scenario: Losing scope clears authority status
      Given a client with authority status for an entity
      When the client loses scope for the entity
      Then authority status is cleared

    Scenario: Authority holder losing scope releases for all
      Given a client with Granted status
      When that client loses scope
      Then other clients transition from Denied to Available

  # --------------------------------------------------------------------------
  # Rule: Delegation disable clears authority
  # --------------------------------------------------------------------------
  # NORMATIVE: Changing away from Delegated clears all authority.
  # --------------------------------------------------------------------------
  Rule: Delegation disable clears authority

    Scenario: Disabling delegation clears authority
      Given a delegated entity with authority state
      When replication_config changes away from Delegated
      Then authority becomes None for all clients

  # --------------------------------------------------------------------------
  # Rule: Out-of-scope requests ignored server-side
  # --------------------------------------------------------------------------
  # NORMATIVE: Server ignores requests from out-of-scope clients.
  # --------------------------------------------------------------------------
  Rule: Out-of-scope requests ignored server-side

    Scenario: Out-of-scope request is ignored
      Given a client out of scope for a delegated entity
      When the server receives an authority request from that client
      Then the request is ignored

  # --------------------------------------------------------------------------
  # Rule: Duplicate and late signals are idempotent
  # --------------------------------------------------------------------------
  # NORMATIVE: No duplicate effects; converge to final state.
  # --------------------------------------------------------------------------
  Rule: Duplicate and late signals are idempotent

    Scenario: Duplicate grant signals produce single effect
      Given a client with Granted status
      When a duplicate grant signal arrives
      Then no additional observable effect occurs

    Scenario: Late signal for ended lifetime is ignored
      Given an entity that has been despawned
      When a late authority signal arrives
      Then the signal is ignored

# ============================================================================
# AMBIGUITIES + PROPOSED CLARIFICATIONS
# ============================================================================
# None identified. The entity authority spec is comprehensive with clear
# state transition table.
# ============================================================================
