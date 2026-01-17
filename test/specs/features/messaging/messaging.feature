# ============================================================================
# Messaging — Canonical Contract
# ============================================================================
# Source: contracts/03_messaging.spec.md
# Last converted: 2026-01-17
#
# Summary:
#   This specification defines Naia's message channel contract including
#   channel registration, delivery/ordering/duplication guarantees per
#   ChannelMode, fragmentation rules, EntityProperty resolution, and
#   Request/Response (RPC) semantics.
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
#   Define the message channel semantics for Naia client/server communication.
#
# GLOSSARY:
#   - Channel: Configured lane for sending/receiving messages
#   - ChannelKind: Unique identifier for a channel type in Protocol
#   - ChannelDirection: Allowed send direction (Client→Server or Server→Client)
#   - ChannelMode: Delivery/ordering semantics of a channel
#   - Reliable: Eventual delivery while connected, at-most-once observation
#   - Ordered: Application observes messages in send order
#   - Sequenced: "Current state" semantics - no rollback after newer observed
#   - TickBuffered: Messages grouped by tick, exposed per tick in order
#   - Entity lifetime: scope enter → scope leave (≥1 tick out-of-scope rule)
#
# CHANNEL MODE GUARANTEE MATRIX:
#   | ChannelMode          | Delivery      | Dedup | Ordering    | Sequenced |
#   |----------------------|---------------|-------|-------------|-----------|
#   | UnorderedUnreliable  | best-effort   | no    | none        | no        |
#   | SequencedUnreliable  | best-effort   | no    | none        | YES       |
#   | UnorderedReliable    | eventual      | YES   | none        | no        |
#   | OrderedReliable      | eventual      | YES   | YES (send)  | no        |
#   | SequencedReliable    | eventual      | YES   | none        | YES       |
#   | TickBuffered         | per tick      | mode  | tick order  | n/a       |
#
# NORMATIVE CHANNEL RULES:
#   [messaging-01] User-initiated errors are Results
#     - Invalid channel config, oversize payload → Result::Err
#
#   [messaging-02] Remote/untrusted input MUST NOT panic
#     - Malformed payload, reorder, duplicates, stale ticks → drop, no panic
#
#   [messaging-03] Framework invariant violations MUST panic
#     - Older state after newer on sequenced channel → panic
#
#   [messaging-04] Channel compatibility is gated by protocol_id
#     - Mismatched protocol_id → reject before any message exchange
#     - Matched protocol_id → guaranteed channel compatibility
#
#   [messaging-05] ChannelDirection is enforced at send-time
#     - Wrong direction → Result::Err
#
# CHANNEL MODE SEMANTICS:
#   [messaging-06] UnorderedUnreliable: best-effort, no ordering, duplicates ok
#   [messaging-07] SequencedUnreliable: best-effort, no rollback after newer
#   [messaging-08] UnorderedReliable: eventual delivery, deduped, unordered
#   [messaging-09] OrderedReliable: eventual + strict send-order delivery
#   [messaging-10] SequencedReliable: eventual + latest wins + no rollback
#
# TICKBUFFERED RULES:
#   [messaging-11] TickBuffered is Client→Server only
#   [messaging-12] Groups messages by tick, exposes in tick order
#   [messaging-13] Capacity and eviction: oldest tick first (FIFO)
#   [messaging-14] Discards very-late ticks (behind retained window)
#   [messaging-15-a] Discards too-far-ahead ticks (> current + MAX_FUTURE_TICKS)
#
# FRAGMENTATION:
#   [messaging-15] Unreliable channels MUST NOT fragment
#   [messaging-16] Reliable channels MAY fragment up to 2^16 fragments
#
# WRAP-AROUND:
#   [messaging-17] Wrap-around MUST NOT break ordering or sequencing
#
# ENTITYPROPERTY RESOLUTION:
#   [messaging-18] Buffer until mapped, drop on despawn
#   [messaging-19] TTL: 60 seconds (configurable), drop if unresolved
#   [messaging-20] Hard cap: 4096 per connection, 128 per entity, oldest-first
#
# REQUEST/RESPONSE (RPC):
#   [messaging-21] Request ID uniqueness per connection
#   [messaging-22] Response matching by Request ID
#   [messaging-23] Per-type timeout semantics (default 30 seconds)
#   [messaging-24] Disconnect cancels pending requests
#   [messaging-25] Transport over reliable ordered channel, deduped
#   [messaging-26] RPC ordering follows channel semantics
#   [messaging-27] Fire-and-forget is valid (no handler registered)
#
# ============================================================================

Feature: Messaging Channel Semantics

  Background:
    Given a Naia test environment is initialized

  # --------------------------------------------------------------------------
  # Rule: Channel direction is enforced at send-time
  # --------------------------------------------------------------------------
  # NORMATIVE: Sending on a channel not configured for that direction MUST
  # return Result::Err.
  # --------------------------------------------------------------------------
  Rule: Channel direction is enforced at send-time

    Scenario: Sending on wrong direction channel returns Err
      Given a connected client and server
      And a channel configured for Server to Client only
      When the client attempts to send on that channel
      Then the send operation returns an Err result

  # --------------------------------------------------------------------------
  # Rule: OrderedReliable delivers in send order
  # --------------------------------------------------------------------------
  # NORMATIVE: OrderedReliable MUST deliver messages in the same order they
  # were sent, despite network reordering.
  # --------------------------------------------------------------------------
  Rule: OrderedReliable delivers in send order

    Scenario: Ordered messages arrive in send order despite reordering
      Given a connected client and server
      And an OrderedReliable channel
      And a transport conditioner that reorders packets
      When the server sends messages A then B then C
      Then the client receives messages in order A then B then C

  # --------------------------------------------------------------------------
  # Rule: SequencedUnreliable prevents rollback
  # --------------------------------------------------------------------------
  # NORMATIVE: Once application observes message with sequence S_new, it
  # MUST NOT later observe any message with older sequence.
  # --------------------------------------------------------------------------
  Rule: SequencedUnreliable prevents rollback

    Scenario: Sequenced channel never rolls back after newer state
      Given a connected client and server
      And a SequencedUnreliable channel
      When the server sends state with sequence 5
      And the client observes state with sequence 5
      And delayed state with sequence 3 arrives
      Then the client does not observe the older state

  # --------------------------------------------------------------------------
  # Rule: UnorderedReliable deduplicates
  # --------------------------------------------------------------------------
  # NORMATIVE: UnorderedReliable MUST dedupe so each message is observed
  # at most once.
  # --------------------------------------------------------------------------
  Rule: UnorderedReliable deduplicates

    Scenario: Reliable messages are deduplicated
      Given a connected client and server
      And an UnorderedReliable channel
      And a transport conditioner that duplicates packets
      When the server sends a message
      Then the client receives the message exactly once

  # --------------------------------------------------------------------------
  # Rule: TickBuffered is Client to Server only
  # --------------------------------------------------------------------------
  # NORMATIVE: TickBuffered channels MUST be configurable only for
  # Client→Server direction.
  # --------------------------------------------------------------------------
  Rule: TickBuffered is Client to Server only

    Scenario: Configuring TickBuffered for wrong direction returns Err
      Given a protocol configuration attempt
      When TickBuffered channel is configured for Server to Client
      Then the configuration returns an Err result

  # --------------------------------------------------------------------------
  # Rule: TickBuffered groups and orders by tick
  # --------------------------------------------------------------------------
  # NORMATIVE: TickBuffered groups messages by tick and exposes ticks in
  # increasing tick order (wrap-safe).
  # --------------------------------------------------------------------------
  Rule: TickBuffered groups and orders by tick

    Scenario: TickBuffered messages are grouped by tick
      Given a connected client and server
      And a TickBuffered channel
      When the client sends messages for tick 5 and tick 7
      Then the server receives tick 5 messages before tick 7 messages

  # --------------------------------------------------------------------------
  # Rule: Very-late tick messages are dropped
  # --------------------------------------------------------------------------
  # NORMATIVE: Messages for ticks older than the oldest retained tick
  # MUST be discarded.
  # --------------------------------------------------------------------------
  Rule: Very-late tick messages are dropped

    Scenario: Very-late tick message is not delivered
      Given a connected client and server
      And a TickBuffered channel with capacity 10
      When the server has processed up to tick 100
      And a message arrives for tick 50
      Then the message is not delivered

  # --------------------------------------------------------------------------
  # Rule: Too-far-ahead tick messages are dropped
  # --------------------------------------------------------------------------
  # NORMATIVE: Messages with tick > current_server_tick + MAX_FUTURE_TICKS
  # MUST be dropped.
  # --------------------------------------------------------------------------
  Rule: Too-far-ahead tick messages are dropped

    Scenario: Too-far-ahead tick message is dropped
      Given a connected client and server
      And a TickBuffered channel with capacity 10
      When a message arrives for current tick plus MAX_FUTURE_TICKS plus 1
      Then the message is dropped

    Scenario: Message at boundary is accepted
      Given a connected client and server
      And a TickBuffered channel with capacity 10
      When a message arrives for current tick plus MAX_FUTURE_TICKS
      Then the message is accepted

  # --------------------------------------------------------------------------
  # Rule: Unreliable channels must not fragment
  # --------------------------------------------------------------------------
  # NORMATIVE: UnorderedUnreliable and SequencedUnreliable MUST NOT fragment.
  # Oversize payload returns Err.
  # --------------------------------------------------------------------------
  Rule: Unreliable channels must not fragment

    Scenario: Oversize unreliable message returns Err
      Given a connected client and server
      And an UnorderedUnreliable channel
      When the client attempts to send an oversize message
      Then the send operation returns an Err result

  # --------------------------------------------------------------------------
  # Rule: EntityProperty buffers until entity is mapped
  # --------------------------------------------------------------------------
  # NORMATIVE: EntityProperty messages buffer until entity spawn, drop on
  # despawn or TTL expiry.
  # --------------------------------------------------------------------------
  Rule: EntityProperty buffers until entity is mapped

    Scenario: EntityProperty received before spawn is applied after spawn
      Given a connected client and server
      When an EntityProperty message references an entity not yet spawned
      And the entity is spawned within TTL
      Then the EntityProperty message is applied

    Scenario: EntityProperty for despawned entity is never applied
      Given a connected client and server
      When an EntityProperty message is buffered for an entity
      And the entity despawns before the message is applied
      Then the EntityProperty message is dropped

  # --------------------------------------------------------------------------
  # Rule: Request ID uniqueness
  # --------------------------------------------------------------------------
  # NORMATIVE: Each Request MUST have a unique Request ID within connection
  # scope and lifetime.
  # --------------------------------------------------------------------------
  Rule: Request ID uniqueness

    Scenario: Multiple requests have distinct IDs
      Given a connected client and server
      When the client sends multiple requests
      Then each request has a distinct Request ID

  # --------------------------------------------------------------------------
  # Rule: Response matching
  # --------------------------------------------------------------------------
  # NORMATIVE: Response MUST be matched to Request by Request ID.
  # --------------------------------------------------------------------------
  Rule: Response matching

    Scenario: Response is delivered to correct request handler
      Given a connected client and server
      When the client sends a request
      And the server sends a response
      Then the response is matched to the original request

    Scenario: Orphan response is dropped silently
      Given a connected client and server
      When a response arrives with no matching pending request
      Then the response is dropped

  # --------------------------------------------------------------------------
  # Rule: Request timeout
  # --------------------------------------------------------------------------
  # NORMATIVE: If response not received within timeout, request is canceled.
  # --------------------------------------------------------------------------
  Rule: Request timeout

    Scenario: Request times out if no response
      Given a connected client and server
      When the client sends a request
      And no response arrives within timeout
      Then the request is canceled with timeout error

  # --------------------------------------------------------------------------
  # Rule: Disconnect cancels pending requests
  # --------------------------------------------------------------------------
  # NORMATIVE: On disconnect, all pending requests MUST be canceled.
  # --------------------------------------------------------------------------
  Rule: Disconnect cancels pending requests

    Scenario: Pending requests canceled on disconnect
      Given a connected client and server
      And the client has pending requests
      When the connection is disconnected
      Then all pending request handlers receive error indication

# ============================================================================
# AMBIGUITIES + PROPOSED CLARIFICATIONS
# ============================================================================
# None identified. The messaging spec is comprehensive and well-defined.
# ============================================================================
