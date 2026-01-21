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


@Feature(world_integration)
Feature: World Integration

  @Rule(01)
  Rule: World Integration

    # All executable scenarios deferred until step bindings implemented.


