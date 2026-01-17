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
# CORE CONTRACTS:
#   [client-events-00] Receive step is ingestion only
#     - MUST NOT mutate client world or produce events
#   [client-events-01] Process step is the only event-production boundary
#     - Replicated state application and pending events via Process only
#   [client-events-02] Drains are pure read+remove
#     - MUST NOT receive/process packets, advance tick
#   [client-events-03] Drain is destructive and idempotent
#     - Subsequent drains without Process step MUST return empty
#   [client-events-04] Spawn is first event for entity lifetime
#     - No Update/Remove before Spawn for that lifetime
#   [client-events-05] No events for entities never in scope
#     - No spawn/insert/update/remove/despawn if never InScope
#   [client-events-06] Despawn ends entity lifetime
#     - No further events for that lifetime after Despawn
#     - Re-enter scope = new lifetime, new Spawn
#   [client-events-07] Component insert/update/remove are one-shot
#     - Exactly one event per applied change
#     - Duplicate packets do not cause duplicate events
#   [client-events-08] Per-entity ordering: spawn→(components)*→despawn
#     - API-visible ordering MUST respect this
#   [client-events-09] Scope transitions reflected as spawn/despawn
#     - Leave scope = Despawn, re-enter = Spawn with snapshot
#   [client-events-10] Message events typed, correctly routed, drain once
#     - Grouped by channel/type, includes sender, no duplicates
#   [client-events-11] Request/response events matched, one-shot, cleaned up
#     - Each delivered exactly once, responses matchable to request
#     - Disconnect cleans up in-flight requests
#   [client-events-12] Authority events out of scope (see 11_entity_authority)
#     - If surfaced via drain, MUST obey drain semantics
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

Feature: Client Events API

  Background:
    Given a Naia client test environment is initialized

  # --------------------------------------------------------------------------
  # Rule: Receive step is ingestion only
  # --------------------------------------------------------------------------
  # NORMATIVE: Receive step MUST NOT mutate client world or produce events.
  # --------------------------------------------------------------------------
  Rule: Receive step is ingestion only

    Scenario: Receive step buffers packets without side effects
      Given pending packets in the transport buffer
      When receive_all_packets is called
      Then packets are buffered internally
      And no events are produced
      And client world is not mutated

  # --------------------------------------------------------------------------
  # Rule: Process step is the only event-production boundary
  # --------------------------------------------------------------------------
  # NORMATIVE: Replicated state application and pending events via Process.
  # --------------------------------------------------------------------------
  Rule: Process step is the only event-production boundary

    Scenario: Events appear only after Process step
      Given packets have been received
      When draining events without calling process
      Then no events are returned

    Scenario: Process step applies replicated state and produces events
      Given packets have been received
      When process_all_packets is called
      Then events become pending
      And replicated state is applied

  # --------------------------------------------------------------------------
  # Rule: Drains are pure read+remove
  # --------------------------------------------------------------------------
  # NORMATIVE: Drains MUST NOT receive/process packets or advance tick.
  # --------------------------------------------------------------------------
  Rule: Drains are pure read+remove

    Scenario: Draining does not receive or process packets
      Given pending events exist
      When take_world_events is called
      Then no packets are received or processed

  # --------------------------------------------------------------------------
  # Rule: Drain is destructive and idempotent
  # --------------------------------------------------------------------------
  # NORMATIVE: Subsequent drains without Process step MUST return empty.
  # --------------------------------------------------------------------------
  Rule: Drain is destructive and idempotent

    Scenario: Second drain without Process returns empty
      Given pending events exist
      When take_world_events is called
      And take_world_events is called again
      Then the second drain returns empty

    Scenario: Draining twice does not return same event twice
      Given an event is pending
      When draining twice back-to-back
      Then the event appears only once

  # --------------------------------------------------------------------------
  # Rule: Spawn is first event for entity lifetime
  # --------------------------------------------------------------------------
  # NORMATIVE: No Update/Remove before Spawn for that entity lifetime.
  # --------------------------------------------------------------------------
  Rule: Spawn is first event for entity lifetime

    Scenario: Spawn precedes all component events
      Given an entity enters scope with components
      When draining events
      Then Spawn is the first event for that entity

    Scenario: No update before spawn under reordering
      Given packets arrive out of order
      When process applies them
      Then no Update event precedes Spawn for any entity

  # --------------------------------------------------------------------------
  # Rule: No events for entities never in scope
  # --------------------------------------------------------------------------
  # NORMATIVE: No entity events if never InScope(C,E).
  # --------------------------------------------------------------------------
  Rule: No events for entities never in scope

    Scenario: Entity created and destroyed out of scope produces no events
      Given an entity is created and destroyed while client is out of scope
      When draining events
      Then no events are produced for that entity

  # --------------------------------------------------------------------------
  # Rule: Despawn ends entity lifetime
  # --------------------------------------------------------------------------
  # NORMATIVE: No further events after Despawn for that lifetime.
  # --------------------------------------------------------------------------
  Rule: Despawn ends entity lifetime

    Scenario: No events after Despawn
      Given Despawn has been emitted for an entity
      When late packets reference that entity
      Then no further events are emitted for that lifetime

    Scenario: Re-entering scope creates new lifetime
      Given an entity was despawned
      When the entity re-enters scope
      Then a new Spawn event is emitted

  # --------------------------------------------------------------------------
  # Rule: Component insert/update/remove are one-shot
  # --------------------------------------------------------------------------
  # NORMATIVE: Exactly one event per applied change, no duplicates.
  # --------------------------------------------------------------------------
  Rule: Component insert/update/remove are one-shot

    Scenario: Insert event when component becomes present
      Given a component is added to an entity
      When draining events
      Then exactly one Insert event is produced

    Scenario: Update event per distinct applied update
      Given a component is updated
      When draining events
      Then exactly one Update event is produced

    Scenario: Remove event when component is removed
      Given a component is removed from an entity
      When draining events
      Then exactly one Remove event is produced

    Scenario: Duplicate packets do not cause duplicate events
      Given a component update packet
      When a duplicate packet is received
      Then only one Update event is produced

  # --------------------------------------------------------------------------
  # Rule: Per-entity ordering: spawn→(components)*→despawn
  # --------------------------------------------------------------------------
  # NORMATIVE: API-visible ordering MUST respect this pattern.
  # --------------------------------------------------------------------------
  Rule: Per-entity ordering: spawn→(components)*→despawn

    Scenario: Event ordering respects spawn before components before despawn
      Given an entity with full lifecycle
      When draining all events
      Then Spawn precedes component events
      And component events precede Despawn

  # --------------------------------------------------------------------------
  # Rule: Scope transitions reflected as spawn/despawn
  # --------------------------------------------------------------------------
  # NORMATIVE: Leave scope = Despawn, re-enter = Spawn with snapshot.
  # --------------------------------------------------------------------------
  Rule: Scope transitions reflected as spawn/despawn

    Scenario: Leaving scope produces Despawn
      Given an entity is in scope
      When the entity leaves scope
      Then a Despawn event is produced

    Scenario: Re-entering scope produces Spawn with snapshot
      Given an entity left scope
      When the entity re-enters scope
      Then a Spawn event is produced with coherent snapshot

  # --------------------------------------------------------------------------
  # Rule: Message events typed, correctly routed, drain once
  # --------------------------------------------------------------------------
  # NORMATIVE: Grouped by channel/type, includes sender, no duplicates.
  # --------------------------------------------------------------------------
  Rule: Message events typed, correctly routed, drain once

    Scenario: Messages grouped by channel and type
      Given messages on different channels are received
      When draining message events
      Then messages are grouped by channel and type

    Scenario: Message events include sender
      Given a message from the server
      When draining message events
      Then the event includes sender identity

    Scenario: Each message drains exactly once
      Given a message is received
      When draining message events twice
      Then the message appears only in the first drain

    Scenario: Dropped unreliable messages produce no events
      Given an unreliable message is dropped
      When draining message events
      Then no event is produced for that message

  # --------------------------------------------------------------------------
  # Rule: Request/response events matched, one-shot, cleaned up
  # --------------------------------------------------------------------------
  # NORMATIVE: Each delivered exactly once, responses matchable to request.
  # --------------------------------------------------------------------------
  Rule: Request/response events matched, one-shot, cleaned up

    Scenario: Response is matchable to request
      Given a request was sent
      When a response is received
      Then the response is matchable to the originating request

    Scenario: Request/response drains exactly once
      Given a response is received
      When draining twice
      Then the response appears only once

    Scenario: Disconnect cleans up in-flight requests
      Given in-flight requests exist
      When client disconnects
      Then in-flight requests are cleaned up
      And no request tracking state is leaked

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
# AMBIGUITIES + PROPOSED CLARIFICATIONS
# ============================================================================
# None identified. The client events API spec is comprehensive and mirrors
# the server events API structure with client-specific considerations.
# ============================================================================
