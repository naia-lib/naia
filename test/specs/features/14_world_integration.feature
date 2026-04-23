# ============================================================================
# World Integration — Canonical Contract
# ============================================================================
# Source: contracts/14_world_integration.spec.md
# Last converted: 2026-04-23
#
# Summary:
#   This specification defines the only valid semantics for integrating Naia's
#   replicated state into an external "game world" (engine ECS, custom world,
#   adapter layer), on both server and client.
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
#   Define how Naia delivers world mutations to external world implementations,
#   ordering expectations, integration lifecycle, and misuse safety requirements.
#
# GLOSSARY:
#   - External World: User/engine-owned state container mirroring Naia's view
#   - Integration Adapter: Code that takes Naia events and applies to External World
#   - Naia World View: Authoritative state Naia believes exists
#   - World Mutation: Spawn, Despawn, ComponentInsert, ComponentUpdate, ComponentRemove
#   - Tick: Discrete step at which Naia advances and produces mutations
#   - Drain: Single pass consuming available Naia events/mutations
#   - In Scope: Entity present in client's Naia World View
#
# ----------------------------------------------------------------------------
# CORE INTEGRATION RULES
# ----------------------------------------------------------------------------
#
# World mirrors Naia view:
#   - External World MUST converge to Naia World View
#   - Entities present/absent MUST match after mutations applied
#   - Component sets and values MUST match
#
# Mutation ordering is deterministic per tick:
#   - Order: Spawn → Inserts → Updates → Removes → Despawn
#   - Insert/Update/Remove MUST NOT apply to absent entity
#   - Despawn MUST occur after all other mutations for that entity
#
# Exactly-once delivery per drain:
#   - Each mutation consumable exactly once
#   - Second drain without tick advance MUST be empty
#
# ----------------------------------------------------------------------------
# SCOPE SEMANTICS
# ----------------------------------------------------------------------------
#
# Scope changes map to spawn/despawn:
#   - OutOfScope → InScope = Spawn + initial components
#   - InScope → OutOfScope = Despawn
#
# Join-in-progress and reconnect yield coherent world:
#   - Reconstructed from current server state, not stale client leftovers
#   - Reconnect is always fresh session (no resumption)
#   - MUST NOT retain entities from prior disconnected session
#
# ----------------------------------------------------------------------------
# IDENTITY AND TYPE CORRECTNESS
# ----------------------------------------------------------------------------
#
# Stable identity mapping:
#   - Same logical identity = same external handle
#   - MUST NOT alias different entities as same external entity
#
# Component type correctness:
#   - Component type MUST match protocol/schema
#   - Decode failure MUST NOT panic
#
# ----------------------------------------------------------------------------
# ROBUSTNESS AND SAFETY
# ----------------------------------------------------------------------------
#
# Misuse safety: no panics, defined failures:
#   - Mutation for absent entity = no-op or error, not panic
#   - Update for missing component = no-op or error, not panic
#   - Re-apply same mutation = deterministic rejection/no-op
#
# Zero-leak lifecycle cleanup:
#   - Disconnect cleans External World fully
#   - Long-running cycles do not leak external entities
#
# ============================================================================


@Feature(world_integration)
Feature: World Integration

  @Rule(01)
  Rule: World Integration

    # [world-integration-01/04] — Client world mirrors Naia view; scope drives presence
    # After scope changes, entity presence in the client world MUST match Naia's view.
    @Scenario(01)
    Scenario: world-integration-04 — Entity presence in client world mirrors scope state
      Given a server is running
      And a client connects
      And a server-owned entity exists
      And the client and entity share a room
      Then the entity is in-scope for the client
      When the server excludes the entity for the client
      Then the entity is out-of-scope for the client
      When the server includes the entity for the client
      Then the entity is in-scope for the client

    # [world-integration-05] — Late-joining client world is built from current server state
    # A second client joining a running game MUST see current entities, not stale state.
    @Scenario(02)
    Scenario: world-integration-05 — Late-joining client receives current server snapshot
      Given a server is running
      And a client connects
      And a server-owned entity exists with a replicated component
      And the client and entity share a room
      And the entity is in-scope for the client
      When a second client connects and the entity enters scope for it
      Then the second client has the entity in its world

    # [world-integration-07] — Component type correctness: values match the server's authoritative state
    # The client's replicated component values MUST match what the server wrote.
    @Scenario(03)
    Scenario: world-integration-07 — Component values in client world match server state
      Given a server is running
      And a client connects
      And a server-owned entity exists with a replicated component
      And the client and entity share a room
      And the entity is in-scope for the client
      When the server updates the replicated component
      Then the client observes the component update

    # [world-integration-09] — Component removal is reflected in client world
    # When the server removes a replicated component from an in-scope entity,
    # the client's world MUST no longer contain that component value.
    @Scenario(04)
    Scenario: world-integration-09 — Component removal propagates to client world
      Given a server is running
      And a client connects
      And a server-owned entity exists with a replicated component
      And the client and entity share a room
      And the entity is in-scope for the client
      When the server removes the replicated component
      Then the client world no longer has the component on the entity

    # [world-integration-08] — Component insert is reflected in client world
    # When the server inserts a replicated component into an already-in-scope entity,
    # the client's world MUST converge to include that component.
    @Scenario(05)
    Scenario: world-integration-08 — Component insert propagates to client world
      Given a server is running
      And a client connects
      And a server-owned entity exists without a replicated component
      And the client and entity share a room
      And the entity is in-scope for the client
      When the server inserts the replicated component
      Then the client world has the component on the entity

    # [world-integration-06] — Disconnect cleans External World fully
    # Zero-leak lifecycle cleanup: after client disconnect, the client world
    # MUST NOT retain entities from the session.
    @Scenario(06)
    Scenario: world-integration-06 — Disconnect cleans client world of all server entities
      Given a server is running
      And a client connects
      And a server-owned entity exists
      And the client and entity share a room
      And the entity is in-scope for the client
      When the client disconnects
      Then the entity despawns on the client
