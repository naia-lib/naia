# ============================================================================
# Entity Delegation — Canonical Contract
# ============================================================================
# Source: contracts/10_entity_delegation.spec.md
# Last converted: 2026-01-17
#
# Summary:
#   This specification defines how a server-owned delegated entity grants
#   temporary Authority to clients so exactly one client at a time may write
#   replicated updates for that entity.
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
#   Define the meaning of Delegated replication configuration, authority
#   arbitration (request/grant/deny/release), and required behavior.
#
# GLOSSARY:
#   - Delegated entity: Server-owned with ReplicationConfig::Delegated
#   - Authority holder: Single actor allowed to write (server or one client)
#   - EntityAuthStatus (client view):
#     * Available: No one holding, client may request
#     * Requested: Client requested, awaiting server decision
#     * Granted: Client holds authority and may write
#     * Releasing: Authority release in progress, may still write
#     * Denied: Another holds authority
#
# CORE MODEL:
#   [entity-delegation-01] Delegation applies only to server-owned delegated
#   [entity-delegation-02] Single-writer invariant
#     - At most one client may hold authority
#     - Server MAY reset/revoke at any time
#     - While client holds authority, server MUST NOT originate writes
#   [entity-delegation-03] Authority is scoped: only in-scope clients participate
#
# ENTERING DELEGATION (MIGRATION):
#   [entity-delegation-04] Client-owned → delegated requires Published
#   [entity-delegation-05] Migration grants authority to previous owner
#     - Previous owner immediately becomes authority holder
#     - EntityAuthStatus == Granted
#
# AUTHORITY ARBITRATION:
#   [entity-delegation-06] First request wins
#     - If Available, first in-scope request is granted
#     - Requests while held resolve as Denied (no queue)
#   [entity-delegation-07] Meaning of Denied
#     - Another holds authority; remains Denied until release/reset
#   [entity-delegation-08] Requested means pending; no writes allowed
#     - May mutate locally, MUST NOT write
#     - Write attempt → panic
#   [entity-delegation-09] Granted means writes allowed; single writer enforced
#   [entity-delegation-10] Releasing means writes may continue until finalized
#   [entity-delegation-11] Release transitions authority back to Available
#
# CLIENT SAFETY:
#   [entity-delegation-12] Client must never write without permission
#     - Write without Granted/Releasing → panic
#
# SCOPE/DISCONNECT INTERACTIONS:
#   [entity-delegation-13] Losing scope ends client authority
#   [entity-delegation-14] Disconnect releases authority
#
# ILLEGAL CASES:
#   [entity-delegation-15] Out-of-scope requests are ignored
#   [entity-delegation-16] Conflicting reconfiguration collapses to final
#
# OBSERVABILITY:
#   [entity-delegation-17] Delegation observable via replication_config + events
#
# ============================================================================

Feature: Entity Delegation

  Background:
    Given a Naia test environment is initialized

  # --------------------------------------------------------------------------
  # Rule: Delegation applies only to server-owned delegated entities
  # --------------------------------------------------------------------------
  # NORMATIVE: Authority delegation semantics apply only when entity is
  # server-owned and replication_config == Delegated.
  # --------------------------------------------------------------------------
  Rule: Delegation applies only to server-owned delegated entities

    Scenario: Non-delegated entity has no authority semantics
      Given a server-owned entity that is not delegated
      Then authority status is not applicable

  # --------------------------------------------------------------------------
  # Rule: Single-writer invariant
  # --------------------------------------------------------------------------
  # NORMATIVE: At most one client may hold authority at a time.
  # Server MUST NOT originate writes while client holds authority.
  # --------------------------------------------------------------------------
  Rule: Single-writer invariant

    Scenario: Only one client can hold authority
      Given a delegated entity
      When client A holds authority
      Then client B is Denied

  # --------------------------------------------------------------------------
  # Rule: Migration grants authority to previous owner
  # --------------------------------------------------------------------------
  # NORMATIVE: When client-owned entity migrates to delegated, previous
  # owner immediately becomes authority holder with Granted status.
  # --------------------------------------------------------------------------
  Rule: Migration grants authority to previous owner

    Scenario: Previous owner gets Granted on migration
      Given a published client-owned entity
      When delegation is enabled
      Then the previous owner has EntityAuthStatus Granted

  # --------------------------------------------------------------------------
  # Rule: First request wins
  # --------------------------------------------------------------------------
  # NORMATIVE: If authority is Available, first in-scope request wins.
  # Requests while held resolve as Denied.
  # --------------------------------------------------------------------------
  Rule: First request wins

    Scenario: First requester gets authority
      Given a delegated entity with Available authority
      When client A and client B both request authority
      Then the first request wins
      And the other is Denied

    Scenario: Request while held is Denied
      Given a delegated entity with authority held by client A
      When client B requests authority
      Then client B is Denied

  # --------------------------------------------------------------------------
  # Rule: Requested status means no writes allowed
  # --------------------------------------------------------------------------
  # NORMATIVE: While Requested, client may mutate locally but MUST NOT write.
  # --------------------------------------------------------------------------
  Rule: Requested status means no writes allowed

    Scenario: Writing while Requested would panic
      Given a client with Requested status for an entity
      Then attempting to write would trigger a panic

  # --------------------------------------------------------------------------
  # Rule: Granted status allows writes
  # --------------------------------------------------------------------------
  # NORMATIVE: When Granted, client MAY write. All others are Denied.
  # --------------------------------------------------------------------------
  Rule: Granted status allows writes

    Scenario: Granted client can write
      Given a client with Granted status for an entity
      When the client writes to the entity
      Then the write is accepted

  # --------------------------------------------------------------------------
  # Rule: Releasing allows writes until finalized
  # --------------------------------------------------------------------------
  # NORMATIVE: While Releasing, writes may continue. Others remain Denied.
  # --------------------------------------------------------------------------
  Rule: Releasing allows writes until finalized

    Scenario: Client can write while Releasing
      Given a client in Releasing status
      When the client writes
      Then the write is accepted

    Scenario: Release finalizes to Available
      Given a client in Releasing status
      When release finalizes
      Then the client becomes Available
      And other clients become Available

  # --------------------------------------------------------------------------
  # Rule: Losing scope ends authority
  # --------------------------------------------------------------------------
  # NORMATIVE: If authority holder loses scope, authority is released/reset.
  # --------------------------------------------------------------------------
  Rule: Losing scope ends authority

    Scenario: Out-of-scope authority holder loses authority
      Given a client holding authority
      When the client loses scope for the entity
      Then authority is released
      And other in-scope clients become Available

  # --------------------------------------------------------------------------
  # Rule: Disconnect releases authority
  # --------------------------------------------------------------------------
  # NORMATIVE: If authority-holding client disconnects, authority is released.
  # --------------------------------------------------------------------------
  Rule: Disconnect releases authority

    Scenario: Disconnecting authority holder releases authority
      Given a client holding authority
      When the client disconnects
      Then authority is released
      And other in-scope clients become Available

  # --------------------------------------------------------------------------
  # Rule: Out-of-scope requests are ignored
  # --------------------------------------------------------------------------
  # NORMATIVE: Requests from out-of-scope clients are ignored.
  # --------------------------------------------------------------------------
  Rule: Out-of-scope requests are ignored

    Scenario: Out-of-scope request is ignored
      Given a client out-of-scope for a delegated entity
      When the client attempts to request authority
      Then the request is ignored

# ============================================================================
# AMBIGUITIES + PROPOSED CLARIFICATIONS
# ============================================================================
# None identified. The entity delegation spec is comprehensive.
# ============================================================================
