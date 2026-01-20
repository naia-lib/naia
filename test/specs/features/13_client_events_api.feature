# ============================================================================
# Client Events API — Canonical Contract
# ============================================================================
# Source: contracts/13_client_events_api.spec.md
# Last converted: 2026-01-17
#
# Summary:
#   This specification defines the only valid semantics for the client-side
#   Events API: what events exist, when they become observable, how they are
#   drained, ordering guarantees, and behavior under reordering/duplication/
#   scope changes/disconnects.
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
#   Define the client-side Events API surface including receive/process/drain
#   boundaries, event ordering, and scope-related guarantees.
#
# GLOSSARY:
#   - Client Events API: Interface to drain replicated-world events
#   - World events: Client's replicated world changes + inbound messages
#   - Tick events: Connection/tick/session-level happenings
#   - Receive step: Ingesting packets into internal buffer
#   - Process step: Processing buffered packets, producing pending events
#   - Drain: Reading events from API such that they are removed
#   - InScope(C,E): Entity E exists in client C's local world
#   - Entity lifetime: scope enter → scope leave (≥1 tick out-of-scope rule)
#
# API BOUNDARY MODEL:
#   1) receive_all_packets() — Receive step
#   2) process_all_packets() — Process step
#   3) take_tick_events() and/or take_world_events() — Drain steps
#
# ----------------------------------------------------------------------------
# CORE EVENT PIPELINE RULES
# ----------------------------------------------------------------------------
#
# Receive step is ingestion only:
#   - MUST NOT mutate client world or produce events
#
# Process step is the only event-production boundary:
#   - Replicated state application and pending events via Process only
#
# Drains are pure read+remove:
#   - MUST NOT receive/process packets, advance tick
#
# Drain is destructive and idempotent:
#   - Subsequent drains without Process step MUST return empty
#
# ----------------------------------------------------------------------------
# ENTITY EVENT ORDERING
# ----------------------------------------------------------------------------
#
# Spawn is first event for entity lifetime:
#   - No Update/Remove before Spawn for that lifetime
#
# No events for entities never in scope:
#   - No spawn/insert/update/remove/despawn if never InScope
#
# Despawn ends entity lifetime:
#   - No further events for that lifetime after Despawn
#   - Re-enter scope = new lifetime, new Spawn
#
# Component insert/update/remove are one-shot:
#   - Exactly one event per applied change
#   - Duplicate packets do not cause duplicate events
#
# Per-entity ordering: spawn→(components)*→despawn:
#   - API-visible ordering MUST respect this
#
# Scope transitions reflected as spawn/despawn:
#   - Leave scope = Despawn, re-enter = Spawn with snapshot
#
# ----------------------------------------------------------------------------
# MESSAGE/RPC EVENTS
# ----------------------------------------------------------------------------
#
# Message events typed, correctly routed, drain once:
#   - Grouped by channel/type, includes sender, no duplicates
#
# Request/response events matched, one-shot, cleaned up:
#   - Each delivered exactly once, responses matchable to request
#   - Disconnect cleans up in-flight requests
#
# ----------------------------------------------------------------------------
# AUTHORITY EVENTS
# ----------------------------------------------------------------------------
#
# Authority events out of scope (see 11_entity_authority):
#   - If surfaced via drain, MUST obey drain semantics
#
# ----------------------------------------------------------------------------
# FORBIDDEN BEHAVIORS
# ----------------------------------------------------------------------------
#
# FORBIDDEN BEHAVIORS:
#   - Producing events during drains
#   - Replaying drained events without new Process step
#   - Update/Remove before Spawn for entity lifetime
#   - Entity events for never-in-scope entities
#   - Entity events after Despawn for that lifetime
#   - Misrouting message events to wrong channel/type
#   - Panicking on empty or repeated drains
#
# ============================================================================


@Feature(client_events_api)
Feature: Client Events API

  # All executable scenarios deferred until step bindings implemented.


