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
# ----------------------------------------------------------------------------
# CORE MODEL
# ----------------------------------------------------------------------------
#
# Delegation applies only to server-owned delegated entities
#
# Single-writer invariant:
#   - At most one client may hold authority
#   - Server MAY reset/revoke at any time
#   - While client holds authority, server MUST NOT originate writes
#
# Authority is scoped: only in-scope clients participate
#
# ----------------------------------------------------------------------------
# ENTERING DELEGATION (MIGRATION)
# ----------------------------------------------------------------------------
#
# Client-owned → delegated requires Published
#
# Migration grants authority to previous owner:
#   - Previous owner immediately becomes authority holder
#   - EntityAuthStatus == Granted
#
# ----------------------------------------------------------------------------
# AUTHORITY ARBITRATION
# ----------------------------------------------------------------------------
#
# First request wins:
#   - If Available, first in-scope request is granted
#   - Requests while held resolve as Denied (no queue)
#
# Meaning of Denied:
#   - Another holds authority; remains Denied until release/reset
#
# Requested means pending; no writes allowed:
#   - May mutate locally, MUST NOT write
#   - Write attempt → panic
#
# Granted means writes allowed; single writer enforced
#
# Releasing means writes may continue until finalized
#
# Release transitions authority back to Available
#
# ----------------------------------------------------------------------------
# CLIENT SAFETY
# ----------------------------------------------------------------------------
#
# Client must never write without permission:
#   - Write without Granted/Releasing → panic
#
# ----------------------------------------------------------------------------
# SCOPE/DISCONNECT INTERACTIONS
# ----------------------------------------------------------------------------
#
# Losing scope ends client authority
#
# Disconnect releases authority
#
# ----------------------------------------------------------------------------
# ILLEGAL CASES AND OBSERVABILITY
# ----------------------------------------------------------------------------
#
# Out-of-scope requests are ignored
#
# Conflicting reconfiguration collapses to final
#
# Delegation observable via replication_config + events
#
# ============================================================================


@Feature(entity_delegation)
Feature: Entity Delegation

  @Rule(01)
  Rule: Entity Delegation

    # [entity-delegation-06] — First request wins
    # The first in-scope client to request authority MUST be granted it.
    # A second client requesting while authority is held MUST observe Denied.
    @Scenario(01)
    Scenario: entity-delegation-06 — First request wins; other in-scope clients observe Denied
      Given a server is running
      And client A connects
      And client B connects
      And the server spawns a delegated entity in-scope for both clients
      When client A requests authority for the delegated entity
      Then client A is granted authority for the delegated entity
      And client B is denied authority for the delegated entity


