# ============================================================================
# Entity Replication — Canonical Contract
# ============================================================================
# Source: contracts/07_entity_replication.spec.md
# Last converted: 2026-01-17
#
# Summary:
#   This specification defines the client-observable behavior of Naia's
#   entity/component replication: spawn/despawn, component insert/update/remove
#   ordering, tolerance to packet reordering/duplication/late arrival, and
#   entity identity across lifetimes.
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
#   Define client-observable replication behavior and invariants.
#
# GLOSSARY:
#   - Replicated component: Component type registered for wire replication
#   - Local-only component: Component present only locally, not replicated
#   - Entity lifetime (client): scope enter → scope leave (≥1 tick rule)
#   - GlobalEntity: Global identity (monotonic u64, practical uniqueness)
#   - LocalEntity: Per-connection handle that may wrap/reuse
#
# ENTITY LIFETIME:
#   For a given client, an entity lifetime is:
#   scope enter → scope leave, with re-entry after ≥1 tick being fresh lifetime.
#
#   - Entity-specific writes MUST be ignored outside current lifetime
#   - Update before Insert MUST be buffered until Insert arrives
#
# ----------------------------------------------------------------------------
# IDENTITY AND LIFETIME
# ----------------------------------------------------------------------------
#
# Global identity stability:
#   - Entity MUST have stable GlobalEntity while it exists
#   - Server MUST NOT change GlobalEntity during entity's existence
#
# Client-visible lifetime boundaries:
#   - Lifetime begins on Spawn (scope enter)
#   - Lifetime ends on Despawn (scope leave)
#   - Re-entry after ≥1 tick is new lifetime with fresh spawn snapshot
#
# ----------------------------------------------------------------------------
# SPAWN AND INITIAL STATE
# ----------------------------------------------------------------------------
#
# Spawn snapshot semantics:
#   - Spawn MUST include set of replicated components at send time
#   - Client MUST materialize baseline solely from Spawn snapshot
#
# No observable replication before Spawn:
#   - Client MUST NOT observe Insert/Update/Remove before Spawn
#   - If delivery causes early receipt, actions are not observable early
#   - HARD INVARIANT: no update-before-spawn observability
#
# ----------------------------------------------------------------------------
# REPLICATION ORDERING AND BUFFERING
# ----------------------------------------------------------------------------
#
# Actions outside lifetime are ignored:
#   - Late packets from prior lifetime: ignored
#   - Packets for out-of-scope entities: ignored
#   - In Debug: MAY warn
#
# Update-before-Insert buffering:
#   - Within active lifetime, buffer Update until Insert arrives
#   - Drop buffered on Despawn if not applied
#
# Local-only component overwrite by replication:
#   - Server replication overwrites local-only component
#   - MUST emit Insert event (not Update)
#
# ----------------------------------------------------------------------------
# TICK COLLAPSE AND IDEMPOTENCY
# ----------------------------------------------------------------------------
#
# Collapse to final state per tick:
#   - No intermediate transitions within a tick
#   - Only final state is observable
#
# Duplicate delivery is idempotent:
#   - Same action twice MUST NOT create additional observable effects
#   - Naia MUST remain convergent
#
# ----------------------------------------------------------------------------
# IDENTITY SAFETY
# ----------------------------------------------------------------------------
#
# Identity reuse safety (LocalEntity wrap):
#   - Late/reordered actions from old lifetime MUST NOT corrupt new lifetime
#   - Lifetime-disambiguating info MUST gate applicability
#
# GlobalEntity rollover is terminal error:
#   - MUST NOT silently wrap/reuse GlobalEntity values
#   - MUST enter terminal error mode (panic/abort)
#
# ----------------------------------------------------------------------------
# CONFLICT RESOLUTION
# ----------------------------------------------------------------------------
#
# Conflict resolution: server wins:
#   - Server replicated state MUST overwrite client local state
#
# ============================================================================


@Feature(entity_replication)
Feature: Entity Replication

  # --------------------------------------------------------------------------
  # Rule: Entity Replication
  # --------------------------------------------------------------------------
  # Core replication semantics: spawn, component sync, identity stability
  # --------------------------------------------------------------------------
  @Rule(01)
  Rule: Entity Replication

    @Scenario(01)
    Scenario: Entity spawns on client when entering scope
      Given a server is running
      And a client connects
      And a server-owned entity exists with a replicated component
      And the client and entity share a room
      Then the entity spawns on the client with the replicated component

    @Scenario(02)
    Scenario: Component updates are replicated to client
      Given a server is running
      And a client connects
      And a server-owned entity exists with a replicated component
      And the client and entity share a room
      And the entity is in-scope for the client
      When the server updates the replicated component
      Then the client observes the component update

    @Scenario(03)
    Scenario: Entity despawns on client when leaving scope
      Given a server is running
      And a client connects
      And a server-owned entity exists with a replicated component
      And the client and entity share a room
      And the entity is in-scope for the client
      When the server excludes the entity for the client
      Then the entity despawns on the client

    @Deferred
    @Scenario(04)
    Scenario: GlobalEntity identity is stable during entity lifetime
      Given a server is running
      And a client connects
      And a server-owned entity exists with a replicated component
      And the client and entity share a room
      And the entity is in-scope for the client
      When the server updates the replicated component
      Then the entity GlobalEntity remains unchanged

    @Deferred
    @Scenario(05)
    Scenario: Server state overwrites client local state on conflict
      Given a server is running
      And a client connects
      And a server-owned entity exists with a replicated component
      And the client and entity share a room
      And the entity is in-scope for the client
      And the client modifies the component locally
      When the server updates the replicated component
      Then the client observes the server value


