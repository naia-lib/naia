# ============================================================================
# World Integration — Canonical Contract
# ============================================================================
# Source: contracts/14_world_integration.spec.md
# Last converted: 2026-01-17
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

Feature: World Integration

  Background:
    Given a Naia test environment is initialized

  # --------------------------------------------------------------------------
  # Rule: External World mirrors Naia view
  # --------------------------------------------------------------------------
  # NORMATIVE: External World MUST converge to Naia World View as mutations
  # are drained and applied.
  # --------------------------------------------------------------------------
  Rule: External World mirrors Naia view

    Scenario: Server External World matches Naia server view
      Given a server with integrated External World
      When entities are spawned, updated, and despawned
      Then External World matches Naia server view each tick

    Scenario: Client External World matches client Naia view with scope
      Given a client with integrated External World
      When entities enter and leave scope
      Then External World matches client Naia local view

  # --------------------------------------------------------------------------
  # Rule: Mutation ordering is deterministic per tick
  # --------------------------------------------------------------------------
  # NORMATIVE: Spawn → Inserts → Updates → Removes → Despawn per entity.
  # --------------------------------------------------------------------------
  Rule: Mutation ordering is deterministic per tick

    Scenario: Spawn precedes component operations
      Given an entity spawns and receives components in same tick
      When adapter applies mutations
      Then Spawn is applied before component inserts

    Scenario: Despawn follows all component operations
      Given an entity has components removed and despawns in same tick
      When adapter applies mutations
      Then removes are applied before despawn

    Scenario: Component operations require entity presence
      Given an entity is not yet spawned
      When a component insert is attempted
      Then the operation is rejected or deferred

  # --------------------------------------------------------------------------
  # Rule: Exactly-once delivery per drain
  # --------------------------------------------------------------------------
  # NORMATIVE: Each mutation consumable exactly once.
  # --------------------------------------------------------------------------
  Rule: Exactly-once delivery per drain

    Scenario: Second drain without tick advance is empty (server)
      Given mutations were drained on server
      When draining again without advancing tick
      Then the drain returns empty

    Scenario: Second drain without tick advance is empty (client)
      Given mutations were drained on client
      When draining again without advancing tick
      Then the drain returns empty

    Scenario: No duplicate deliveries
      Given a single mutation is produced
      When adapter consumes mutations
      Then exactly one delivery occurs

  # --------------------------------------------------------------------------
  # Rule: Scope changes map to spawn/despawn in External World
  # --------------------------------------------------------------------------
  # NORMATIVE: OutOfScope → InScope = Spawn, InScope → OutOfScope = Despawn.
  # --------------------------------------------------------------------------
  Rule: Scope changes map to spawn/despawn in External World

    Scenario: Scope enter creates entity with snapshot
      Given an entity is out of scope for client
      When the entity enters scope
      Then External World receives Spawn with initial components

    Scenario: Scope leave removes entity
      Given an entity is in scope for client
      When the entity leaves scope
      Then External World receives Despawn
      And no ghost entities remain

  # --------------------------------------------------------------------------
  # Rule: Join-in-progress and reconnect yield coherent External World
  # --------------------------------------------------------------------------
  # NORMATIVE: Reconstructed from current server state, not stale leftovers.
  # --------------------------------------------------------------------------
  Rule: Join-in-progress and reconnect yield coherent External World

    Scenario: Late join builds world from snapshot only
      Given a game is in progress with entities
      When a new client joins
      Then External World is built from current server snapshot
      And no stale data is used

    Scenario: Reconnect clears old world and rebuilds
      Given a client was connected with entities
      When the client disconnects and reconnects
      Then previous session entities are cleared
      And External World is rebuilt from current state

    Scenario: Reconnecting client does not retain prior authority
      Given a client held authority before disconnect
      When the client reconnects
      Then authority from previous session is not retained

  # --------------------------------------------------------------------------
  # Rule: Stable identity mapping at integration boundary
  # --------------------------------------------------------------------------
  # NORMATIVE: Same logical identity = same external handle.
  # --------------------------------------------------------------------------
  Rule: Stable identity mapping at integration boundary

    Scenario: Same entity keeps same external mapping across ticks
      Given an entity exists across multiple ticks
      When adapter maintains mapping
      Then the same external handle is used consistently

    Scenario: Different entities not aliased across lifetimes
      Given entity A despawns
      And entity B spawns later
      When adapter handles both
      Then A and B have distinct external handles

  # --------------------------------------------------------------------------
  # Rule: Component type correctness
  # --------------------------------------------------------------------------
  # NORMATIVE: Component type MUST match protocol/schema.
  # --------------------------------------------------------------------------
  Rule: Component type correctness

    Scenario: Component types are correct and never misrouted
      Given various component types are used
      When mutations are applied
      Then each component type matches its declaration

    Scenario: Decode failure does not panic
      Given a component fails to decode
      When the mutation is processed
      Then no panic occurs
      And the mutation is safely ignored or rejected

  # --------------------------------------------------------------------------
  # Rule: Misuse safety at integration boundary
  # --------------------------------------------------------------------------
  # NORMATIVE: No panics, defined failures for misuse cases.
  # --------------------------------------------------------------------------
  Rule: Misuse safety at integration boundary

    Scenario: Update for missing entity is safe
      Given an update targets a non-existent entity
      When the update is applied
      Then no panic occurs
      And the operation is a no-op or returns error

    Scenario: Update for missing component is safe
      Given an update targets a missing component
      When the update is applied
      Then no panic occurs
      And the operation is a no-op or returns error

    Scenario: Double-apply is safe and deterministic
      Given a mutation was already applied
      When the same mutation is applied again
      Then no corruption occurs
      And behavior is deterministic

  # --------------------------------------------------------------------------
  # Rule: Zero-leak lifecycle cleanup
  # --------------------------------------------------------------------------
  # NORMATIVE: Disconnect cleans External World fully, no leaks.
  # --------------------------------------------------------------------------
  Rule: Zero-leak lifecycle cleanup

    Scenario: Disconnect cleans world fully
      Given a client has entities in External World
      When the client disconnects
      Then External World contains no entities for that session

    Scenario: Server clearing world empties External World
      Given a server has entities in External World
      When all entities are cleared
      Then External World is empty

    Scenario: Long-running cycles do not leak entities
      Given many connect/disconnect cycles occur
      When External World is inspected
      Then no leaked entities exist

# ============================================================================
# DEFERRED TESTS
# ============================================================================
# Items that cannot be tested with current harness capabilities.
# ============================================================================
#
# Rule: External World adapter performance under load
#   Assertions:
#     - Mutation application rate scales with entity count
#   Harness needs: Performance benchmarking infrastructure
#
# Rule: External World consistency after crash recovery
#   Assertions:
#     - External World can be reconstructed from Naia state
#   Harness needs: Crash injection and state recovery testing
#
# ============================================================================

# ============================================================================
# AMBIGUITIES + PROPOSED CLARIFICATIONS
# ============================================================================
# None identified. The world integration spec clearly defines boundaries and
# explicitly notes what is out of scope.
# ============================================================================
