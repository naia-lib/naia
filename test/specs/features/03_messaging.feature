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
# ----------------------------------------------------------------------------
# ERROR HANDLING
# ----------------------------------------------------------------------------
#
# User-initiated errors return Result::Err:
#   - Invalid channel config, oversize payload → Result::Err
#
# Remote/untrusted input MUST NOT panic:
#   - Malformed payload, reorder, duplicates, stale ticks → drop, no panic
#
# Framework invariant violations MUST panic:
#   - Older state after newer on sequenced channel → panic
#
# ----------------------------------------------------------------------------
# CHANNEL COMPATIBILITY
# ----------------------------------------------------------------------------
#
# Channel compatibility is gated by protocol_id:
#   - Mismatched protocol_id → reject before any message exchange
#   - Matched protocol_id → guaranteed channel compatibility
#
# ChannelDirection is enforced at send-time:
#   - Wrong direction → Result::Err
#
# ----------------------------------------------------------------------------
# CHANNEL MODE SEMANTICS
# ----------------------------------------------------------------------------
#
#   - UnorderedUnreliable: best-effort, no ordering, duplicates ok
#   - SequencedUnreliable: best-effort, no rollback after newer
#   - UnorderedReliable: eventual delivery, deduped, unordered
#   - OrderedReliable: eventual + strict send-order delivery
#   - SequencedReliable: eventual + latest wins + no rollback
#
# ----------------------------------------------------------------------------
# TICKBUFFERED RULES
# ----------------------------------------------------------------------------
#
#   - TickBuffered is Client→Server only
#   - Groups messages by tick, exposes in tick order
#   - Capacity and eviction: oldest tick first (FIFO)
#   - Discards very-late ticks (behind retained window)
#   - Discards too-far-ahead ticks (> current + MAX_FUTURE_TICKS)
#
# ----------------------------------------------------------------------------
# FRAGMENTATION
# ----------------------------------------------------------------------------
#
#   - Unreliable channels MUST NOT fragment
#   - Reliable channels MAY fragment up to 2^16 fragments
#
# ----------------------------------------------------------------------------
# WRAP-AROUND
# ----------------------------------------------------------------------------
#
#   - Wrap-around MUST NOT break ordering or sequencing
#
# ----------------------------------------------------------------------------
# ENTITYPROPERTY RESOLUTION
# ----------------------------------------------------------------------------
#
#   - Buffer until entity is mapped, drop on despawn
#   - TTL: 60 seconds (configurable), drop if unresolved
#   - Hard cap: 4096 per connection, 128 per entity, oldest-first eviction
#
# ----------------------------------------------------------------------------
# REQUEST/RESPONSE (RPC)
# ----------------------------------------------------------------------------
#
#   - Request ID uniqueness per connection
#   - Response matching by Request ID
#   - Per-type timeout semantics (default 30 seconds)
#   - Disconnect cancels pending requests
#   - Transport over reliable ordered channel, deduped
#   - RPC ordering follows channel semantics
#   - Fire-and-forget is valid (no handler registered)
#
# ============================================================================


@Feature(messaging_channel_semantics)
Feature: Messaging Channel Semantics

  @Rule(01)
  Rule: Messaging Channel Semantics

    # All executable scenarios deferred until step bindings implemented.


