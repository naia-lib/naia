# ============================================================================
# Server Events API — Canonical Contract
# ============================================================================
# Source: contracts/12_server_events_api.spec.md
# Last converted: 2026-04-23
#
# Summary:
#   This specification defines the only valid semantics for the server-side
#   Events API surface: what is collected, when it becomes observable, how
#   it is drained, and what ordering/duplication guarantees exist.
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
#   Define the server-side Events API surface including receive/process/drain
#   boundaries, event partitioning, and ordering guarantees.
#
# GLOSSARY:
#   - Events API: Server-facing interface that buffers and exposes events
#   - World events: Replicated-world changes + inbound app-level messages
#   - Tick events: Connection/tick/session-level happenings
#   - Receive step: Ingesting packets from transport into internal buffer
#   - Process step: Processing buffered packets, producing pending events
#   - Drain: Reading events from API such that they are removed (read+remove)
#   - In scope: User is recipient for entity per InScope(user, entity)
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
#   - MUST NOT advance tick, mutate world, or produce events
#
# Process step is the only event-production boundary:
#   - New events MUST become pending only via Process step
#
# Drains are pure read+remove:
#   - MUST NOT receive/process packets, advance tick, or have side effects
#
# Drains are destructive and idempotent:
#   - Subsequent drains without Process step MUST return empty
#
# Event types are partitioned:
#   - World mutations NOT in message/request streams
#   - Messages NOT in world mutation streams
#   - Tick/session events NOT in world mutation streams
#
# ----------------------------------------------------------------------------
# CONNECTION EVENTS
# ----------------------------------------------------------------------------
#
# Auth/connect/disconnect ordering:
#   - Exactly one auth decision per attempt
#   - Exactly one connect after accepted auth
#   - Exactly one disconnect per session termination
#
# Disconnect cleanup consistent with contracts:
#   - Per-connection scoped state cleaned up
#   - Ownership cleanup per 08_entity_ownership.spec.md
#
# ----------------------------------------------------------------------------
# ENTITY EVENTS
# ----------------------------------------------------------------------------
#
# Spawn/enter events: per user, in-scope only, once:
#   - Exactly one spawn/enter for (U, E) when InScope becomes true
#
# Component insert/update/remove: per user, no dupes:
#   - Exactly one event per applied transition
#
# Despawn/leave-scope events: exactly-once:
#   - No further events for (U, E, *) after exit unless re-enters
#
# No component events before spawn/enter:
#   - API-visible ordering MUST respect this invariant
#
# ----------------------------------------------------------------------------
# MESSAGE/RPC EVENTS
# ----------------------------------------------------------------------------
#
# Message events: grouped by channel/type, drain once:
#   - Each inbound message appears exactly once
#
# Request/response events: exactly-once, correct match:
#   - No duplicates under retransmit
#
# ----------------------------------------------------------------------------
# SAFETY
# ----------------------------------------------------------------------------
#
# Drains MUST NOT panic:
#   - Empty drains return empty, no panic
#
# FORBIDDEN BEHAVIORS:
#   - Producing events during drains
#   - Replaying drained events without new Process step
#   - Component events before spawn/enter
#   - Entity/component events for out-of-scope users
#   - Duplicating auth/connect/disconnect events
#   - Misrouting messages to wrong channel/type
#   - Panicking on empty drains
#
# ============================================================================


@Feature(server_events_api)
Feature: Server Events API

  @Rule(01)
  Rule: Server Events API

    # [server-events-07] — Entity spawn events are per-user and in-scope only
    # When E enters scope for user U, the server MUST fire exactly one SpawnEntityEvent for (U, E).
    # Out-of-scope users MUST NOT receive spawn events.
    @Scenario(01)
    Scenario: server-events-07 — Server spawn event fires for in-scope user only
      Given a server is running
      And client A connects
      And a server-owned entity enters scope for client A
      Then the server observes a spawn event for client A

    # [server-events-09] — Despawn events are exactly-once per user lifecycle
    # When E leaves scope for U, exactly one despawn/exit event MUST fire for (U, E).
    @Scenario(02)
    Scenario: server-events-09 — Server despawn event fires when entity leaves scope
      Given a server is running
      And client A connects
      And a server-owned entity enters scope for client A
      And the server has observed a spawn event for client A
      When the server removes the entity from client A's scope
      Then the server observes a despawn event for client A

    # [server-events-XX] — Authority grant events are observable server-side
    # When the server grants authority to a client, an EntityAuthGrantEvent MUST fire
    # on the server for the granting user and entity.
    @Scenario(03)
    Scenario: server-events-XX — Server observes authority grant event when client is granted
      Given a server is running
      And client A connects
      And the server spawns a delegated entity in-scope for client A
      When client A requests authority for the delegated entity
      Then the server observes an authority grant event for client A

    # [server-events-XX] — Authority reset events are observable server-side
    # When a client releases authority, a ServerEntityAuthResetEvent MUST fire on the
    # server for the entity, signaling the authority returned to Available.
    @Scenario(04)
    Scenario: server-events-XX — Server observes authority reset event when client releases
      Given a server is running
      And client A connects
      And the server spawns a delegated entity in-scope for client A
      When client A requests authority for the delegated entity
      Then client A is granted authority for the delegated entity
      When client A releases authority for the delegated entity
      Then the server observes an authority reset event

    # [server-events-XX] — Publish events are observable server-side
    # When a client makes its entity Public, a ServerPublishEntityEvent MUST fire
    # on the server for the publishing client and entity.
    @Scenario(05)
    Scenario: server-events-XX — Server observes publish event when client publishes entity
      Given a server is running
      And client A connects
      And client A spawns a client-owned entity with Private replication config
      When client A publishes the entity
      Then the server observes a publish event for client A
