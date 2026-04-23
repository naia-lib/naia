# ============================================================================
# Entity Authority — Canonical Contract
# ============================================================================
# Source: contracts/11_entity_authority.spec.md
# Last converted: 2026-04-23
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
# ----------------------------------------------------------------------------
# CORE AUTHORITY RULES
# ----------------------------------------------------------------------------
#
# Authority defined only for delegated entities:
#   - If replication_config(E) != Delegated, authority(E) MUST be None
#   - Request/release on non-delegated MUST return error
#
# Single-writer rule (client-side):
#   - Client MAY write only when Granted or Releasing
#   - Writing in other states MUST panic
#
# Meaning of Denied:
#   - Authority held by another client OR server
#   - Transitions to Available when holder releases or server resets
#
# ----------------------------------------------------------------------------
# CLIENT API SEMANTICS
# ----------------------------------------------------------------------------
#
# request_authority() is optimistic:
#   - Available → Requested immediately (no round-trip wait)
#
# Request completion transitions:
#   - Requested → Granted (server grants)
#   - Requested → Denied (another holds)
#   - Requested → Available (server resets)
#
# release_authority() transitions:
#   - Granted → Releasing → Available
#   - Requested → Available (cancels request)
#
# Client-side error returns:
#   - Not delegated → ErrNotDelegated
#   - Out-of-scope → ErrNotInScope
#   - Entity doesn't exist → ErrNoSuchEntity
#
# ----------------------------------------------------------------------------
# SERVER SEMANTICS
# ----------------------------------------------------------------------------
#
# First-request wins arbitration
#
# Server may hold authority and block clients:
#   - All clients in Denied while server holds
#
# Server override/reset:
#   - Granted/Releasing/Denied → Available (revoked)
#   - Requested → Available (cleared)
#
# ----------------------------------------------------------------------------
# SCOPE/LIFETIME/DISCONNECT
# ----------------------------------------------------------------------------
#
# Out-of-scope ends authority for that client:
#   - Entity lifetime ends, status cleared, buffered actions discarded
#
# Authority holder losing scope forces global release:
#   - Other in-scope clients: Denied → Available
#
# Delegation disable clears authority:
#   - Authority becomes None, pending cleared, grants revoked
#
# ----------------------------------------------------------------------------
# OBSERVABILITY
# ----------------------------------------------------------------------------
#
# Authority observable via status and events
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


@Feature(entity_authority)
Feature: Entity Authority

  @Rule(01)
  Rule: Entity Authority

    # [entity-authority-01] — Authority is None for non-delegated entities
    # If replication_config(E) != Delegated, authority(E) MUST be None on clients.
    @Scenario(01)
    Scenario: entity-authority-01 — Non-delegated entity has no authority status
      Given a server is running
      And client A connects
      And the server spawns a non-delegated entity in-scope for client A
      Then client A observes no authority status for the entity

    # [entity-authority-09] — Server may hold authority; all clients observe Denied
    # While the server holds authority, all in-scope clients MUST observe Denied.
    @Scenario(02)
    Scenario: entity-authority-09 — Server holding authority puts all clients in Denied
      Given a server is running
      And client A connects
      And client B connects
      And the server spawns a delegated entity in-scope for both clients
      When the server takes authority for the delegated entity
      Then client A is denied authority for the delegated entity
      And client B is denied authority for the delegated entity

    # [entity-authority-10] — Server override/reset transitions all clients to Available
    # When the server resets authority, all clients MUST transition to Available.
    @Scenario(03)
    Scenario: entity-authority-10 — Server reset transitions all clients to Available
      Given a server is running
      And client A connects
      And client B connects
      And the server spawns a delegated entity in-scope for both clients
      And the server takes authority for the delegated entity
      And client A is denied authority for the delegated entity
      When the server releases authority for the delegated entity
      Then client A is available for the delegated entity
      And client B is available for the delegated entity

    # [entity-authority-06] — release_authority() transitions Granted → Releasing → Available
    # A client that holds authority MUST eventually become Available after releasing.
    @Scenario(04)
    Scenario: entity-authority-06 — Client release transitions Granted to Available
      Given a server is running
      And client A connects
      And the server spawns a delegated entity in-scope for client A
      When client A requests authority for the delegated entity
      Then client A is granted authority for the delegated entity
      When client A releases authority for the delegated entity
      Then client A is available for the delegated entity

    # [entity-authority-16] — Authority grant is observable via event API
    # When the server grants authority, the client MUST receive an authority granted event.
    @Scenario(05)
    Scenario: entity-authority-16 — Client receives authority granted event
      Given a server is running
      And client A connects
      And the server spawns a delegated entity in-scope for client A
      When client A requests authority for the delegated entity
      Then client A receives an authority granted event for the entity

    # [entity-authority-16] — Authority reset is observable via event API
    # When the server releases authority, all in-scope clients MUST receive an
    # authority reset event, signaling the entity returned to Available.
    @Scenario(06)
    Scenario: entity-authority-16 — Client receives authority reset event when server releases
      Given a server is running
      And client A connects
      And the server spawns a delegated entity in-scope for client A
      And the server takes authority for the delegated entity
      And client A is denied authority for the delegated entity
      When the server releases authority for the delegated entity
      Then client A receives an authority reset event for the entity

    # [entity-authority-16] — Authority denied event observable when request is denied
    # When client B requests authority while client A's grant is in flight, client B MUST
    # receive a denied event (Requested → Denied transition emits EntityAuthDeniedEvent).
    # Both clients request back-to-back (no intermediate wait) so B is still in Requested
    # state when the server denies it, triggering the Requested→Denied event.
    @Scenario(07)
    Scenario: entity-authority-16 — Client receives authority denied event when request is denied
      Given a server is running
      And client A connects
      And client B connects
      And the server spawns a delegated entity in-scope for both clients
      When client A requests authority for the delegated entity
      And client B requests authority for the delegated entity
      Then client B receives an authority denied event for the entity

    # [entity-authority-07] — request_authority on non-delegated entity MUST return error
    # Calling request_authority() on a non-delegated entity MUST return an error,
    # not panic. No state mutation should occur.
    @Scenario(08)
    Scenario: entity-authority-07 — Request authority on non-delegated entity returns error
      Given a server is running
      And client A connects
      And the server spawns a non-delegated entity in-scope for client A
      When client A requests authority for the non-delegated entity
      Then the authority request fails with an error
