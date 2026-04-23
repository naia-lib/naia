# ============================================================================
# Client Events API — Canonical Contract
# ============================================================================
# Source: contracts/13_client_events_api.spec.md
# Last converted: 2026-04-23
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

  @Rule(01)
  Rule: Client Events API

    # [client-events-04] — Spawn is the first event for an entity lifetime
    # The client MUST receive a SpawnEntityEvent when an entity enters scope.
    @Scenario(01)
    Scenario: client-events-04 — Client receives spawn event when entity enters scope
      Given a server is running
      And a client connects
      And a server-owned entity exists
      And the client and entity share a room
      Then the client receives a spawn event for the entity

    # [client-events-09] — Scope transitions are reflected as spawn/despawn events
    # Leaving scope MUST emit Despawn; re-entering scope MUST emit a new Spawn.
    @Scenario(02)
    Scenario: client-events-09 — Scope leave emits Despawn; re-enter emits Spawn
      Given a server is running
      And a client connects
      And a server-owned entity exists
      And the client and entity share a room
      And the entity is in-scope for the client
      When the server excludes the entity for the client
      Then the client receives a despawn event for the entity
      When the server includes the entity for the client
      Then the client receives a spawn event for the entity

    # [client-events-07] — Component update events are one-shot per applied change
    # When the server updates a replicated component, the client Events API MUST
    # surface exactly one component update event for that applied change.
    @Scenario(03)
    Scenario: client-events-07 — Client receives component update event via Events API
      Given a server is running
      And a client connects
      And a server-owned entity exists with a replicated component
      And the client and entity share a room
      And the entity is in-scope for the client
      When the server updates the replicated component
      Then the client receives a component update event for the entity

    # [client-events-08] — Component remove events are one-shot per applied removal
    # When the server removes a replicated component from an in-scope entity, the
    # client Events API MUST surface exactly one component remove event for that change.
    @Scenario(04)
    Scenario: client-events-08 — Client receives component remove event via Events API
      Given a server is running
      And a client connects
      And a server-owned entity exists with a replicated component
      And the client and entity share a room
      And the entity is in-scope for the client
      When the server removes the replicated component
      Then the client receives a component remove event for the entity

    # [client-events-06] — Component insert events are one-shot per applied insertion
    # When the server inserts a replicated component into an already-in-scope entity,
    # the client Events API MUST surface exactly one component insert event.
    @Scenario(05)
    Scenario: client-events-06 — Client receives component insert event via Events API
      Given a server is running
      And a client connects
      And a server-owned entity exists without a replicated component
      And the client and entity share a room
      And the entity is in-scope for the client
      When the server inserts the replicated component
      Then the client receives a component insert event for the entity
