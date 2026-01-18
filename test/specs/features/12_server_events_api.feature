# ============================================================================
# Server Events API — Canonical Contract
# ============================================================================
# Source: contracts/12_server_events_api.spec.md
# Last converted: 2026-01-17
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

Feature: Server Events API

  Background:
    Given a Naia server test environment is initialized

  # --------------------------------------------------------------------------
  # Rule: Receive step is ingestion only
  # --------------------------------------------------------------------------
  # NORMATIVE: Receive step MUST NOT advance tick, mutate world, or produce
  # observable events directly.
  # --------------------------------------------------------------------------
  Rule: Receive step is ingestion only

    Scenario: Receive step buffers packets without side effects
      Given pending packets in the transport buffer
      When receive_all_packets is called
      Then packets are buffered internally
      And no events are produced
      And tick does not advance

  # --------------------------------------------------------------------------
  # Rule: Process step is the only event-production boundary
  # --------------------------------------------------------------------------
  # NORMATIVE: New events MUST become pending only as a result of Process.
  # --------------------------------------------------------------------------
  Rule: Process step is the only event-production boundary

    Scenario: Events appear only after Process step
      Given packets have been received
      When draining events without calling process
      Then no events are returned

    Scenario: Process step produces pending events
      Given packets have been received
      When process_all_packets is called
      Then events become pending

  # --------------------------------------------------------------------------
  # Rule: Drains are pure read+remove
  # --------------------------------------------------------------------------
  # NORMATIVE: Drain operations MUST have no side effects other than
  # removing drained events.
  # --------------------------------------------------------------------------
  Rule: Drains are pure read+remove

    Scenario: Draining does not receive or process packets
      Given pending events exist
      When take_world_events is called
      Then no packets are received or processed

  # --------------------------------------------------------------------------
  # Rule: Drains are destructive and idempotent
  # --------------------------------------------------------------------------
  # NORMATIVE: Subsequent drains without Process step MUST return empty.
  # --------------------------------------------------------------------------
  Rule: Drains are destructive and idempotent

    Scenario: Second drain without Process returns empty
      Given pending events exist
      When take_world_events is called
      And take_world_events is called again
      Then the second drain returns empty

    Scenario: Drain after new Process returns new events
      Given events were drained
      When new packets are processed
      And take_world_events is called
      Then the new events are returned

  # --------------------------------------------------------------------------
  # Rule: Event types are partitioned
  # --------------------------------------------------------------------------
  # NORMATIVE: No cross-contamination between event categories.
  # --------------------------------------------------------------------------
  Rule: Event types are partitioned

    Scenario: World mutations do not appear in message streams
      Given a spawn event and a message event are pending
      When draining message events
      Then only messages are returned

    Scenario: Messages do not appear in world mutation streams
      Given a spawn event and a message event are pending
      When draining world events
      Then only world mutations are returned

    Scenario: Tick events do not appear in world streams
      Given a connect event and a spawn event are pending
      When draining world events
      Then only world mutations are returned

  # --------------------------------------------------------------------------
  # Rule: Auth/connect/disconnect ordering
  # --------------------------------------------------------------------------
  # NORMATIVE: Exactly-once per session transition, stable ordering.
  # --------------------------------------------------------------------------
  Rule: Auth/connect/disconnect ordering

    Scenario: Valid auth produces auth then connect
      Given auth is required
      When a client authenticates successfully
      Then exactly one auth event occurs
      And exactly one connect event occurs after auth

    Scenario: Invalid auth produces only auth event
      Given auth is required
      When a client fails authentication
      Then exactly one auth event occurs
      And no connect event occurs

    Scenario: Duplicate disconnect signals produce one event
      Given a connected client
      When duplicate disconnect signals are received
      Then exactly one disconnect event occurs

  # --------------------------------------------------------------------------
  # Rule: Disconnect cleanup is consistent with contracts
  # --------------------------------------------------------------------------
  # NORMATIVE: Per-connection scoped state cleaned up on disconnect.
  # --------------------------------------------------------------------------
  Rule: Disconnect cleanup is consistent with contracts

    Scenario: Disconnect cleans up scope membership
      Given a connected client with scope membership
      When the client disconnects
      Then scope membership is removed

    Scenario: Disconnect despawns owned entities
      Given a connected client owning entities
      When the client disconnects
      Then owned entities are despawned

  # --------------------------------------------------------------------------
  # Rule: Spawn/enter events per user, in-scope only, exactly-once
  # --------------------------------------------------------------------------
  # NORMATIVE: Exactly one spawn/enter for (U, E) when InScope becomes true.
  # --------------------------------------------------------------------------
  Rule: Spawn/enter events per user, in-scope only, exactly-once

    Scenario: Entity enters scope for one user only
      Given entity E is in scope for user A but not user B
      When draining events for entity E
      Then user A has a spawn event for E
      And user B has no spawn event for E

    Scenario: Late join snapshot produces spawn events
      Given entities exist in scope
      When a new user joins
      Then spawn events are produced for all in-scope entities exactly once

  # --------------------------------------------------------------------------
  # Rule: Component insert/update/remove per user, no duplicates
  # --------------------------------------------------------------------------
  # NORMATIVE: Exactly one event per applied transition per user.
  # --------------------------------------------------------------------------
  Rule: Component insert/update/remove per user, no duplicates

    Scenario: Component update produces events for in-scope users
      Given entity E is in scope for users A and B
      When component C on E is updated
      Then user A has an update event for (E, C)
      And user B has an update event for (E, C)

    Scenario: Duplicate packets do not create duplicate events
      Given a component update packet is received
      When a duplicate packet is received
      Then only one update event is produced

  # --------------------------------------------------------------------------
  # Rule: Despawn/leave-scope events exactly-once
  # --------------------------------------------------------------------------
  # NORMATIVE: No further events for (U, E, *) after exit unless re-enters.
  # --------------------------------------------------------------------------
  Rule: Despawn/leave-scope events exactly-once

    Scenario: Despawn produces exit event once
      Given entity E is in scope for user A
      When entity E is despawned
      Then exactly one exit event is produced for (A, E)

    Scenario: No component events after exit
      Given entity E has exited scope for user A
      When component events would occur for (A, E)
      Then no events are produced

    Scenario: Re-entering scope produces fresh spawn event
      Given entity E exited scope for user A
      When entity E re-enters scope for user A
      Then a fresh spawn event is produced

  # --------------------------------------------------------------------------
  # Rule: No component events before spawn/enter
  # --------------------------------------------------------------------------
  # NORMATIVE: API-visible ordering MUST respect spawn-before-components.
  # --------------------------------------------------------------------------
  Rule: No component events before spawn/enter

    Scenario: Component events follow spawn event
      Given entity E enters scope with components
      When draining events
      Then spawn event for E precedes component events for E

  # --------------------------------------------------------------------------
  # Rule: Message events grouped by channel and type
  # --------------------------------------------------------------------------
  # NORMATIVE: Each inbound message appears exactly once, with sender.
  # --------------------------------------------------------------------------
  Rule: Message events grouped by channel and type

    Scenario: Messages are grouped by channel and type
      Given messages on different channels are received
      When draining message events
      Then messages are grouped by channel and type

    Scenario: Message events include sender
      Given a message from user A
      When draining message events
      Then the event includes user A as sender

    Scenario: Each message appears exactly once
      Given a message is received
      When draining message events twice
      Then the message appears only in the first drain

  # --------------------------------------------------------------------------
  # Rule: Request/response events exactly-once
  # --------------------------------------------------------------------------
  # NORMATIVE: No duplicates under retransmit/duplication.
  # --------------------------------------------------------------------------
  Rule: Request/response events exactly-once

    Scenario: Request produces exactly one event
      Given a request is received
      When draining request events
      Then exactly one request event is produced

    Scenario: Duplicate request packets produce one event
      Given a request packet is received
      When a duplicate request packet is received
      Then only one request event is produced

  # --------------------------------------------------------------------------
  # Rule: Drains MUST NOT panic
  # --------------------------------------------------------------------------
  # NORMATIVE: Empty drains return empty, no panic.
  # --------------------------------------------------------------------------
  Rule: Drains MUST NOT panic

    Scenario: Empty drain returns empty
      Given no pending events
      When draining events
      Then empty result is returned

    Scenario: Repeated drains do not panic
      Given no pending events
      When draining events multiple times
      Then no panic occurs

# ============================================================================
# DEFERRED TESTS
# ============================================================================
# Items that cannot be tested with current harness capabilities.
# ============================================================================
#
# Rule: Event ordering under high-throughput conditions
#   Assertions:
#     - Event ordering preserved under sustained high message rates
#   Harness needs: High-throughput test framework with ordering verification
#
# Rule: Memory behavior during event accumulation
#   Assertions:
#     - Memory bounded when events accumulate between drains
#   Harness needs: Memory profiling instrumentation
#
# ============================================================================

# ============================================================================
# AMBIGUITIES + PROPOSED CLARIFICATIONS
# ============================================================================
# None identified. The server events API spec is comprehensive.
# ============================================================================
