# ============================================================================
# Messaging Channel Semantics — Grouped Contract Suite
# ============================================================================
# Post-A.4 grouping of multiple source feature files. Each source's content
# is preserved verbatim from the @Rule line onward; per-source separators
# (`# === Source: ... ===`) keep the original boundaries greppable. Free-text
# feature-description blocks from sources are stripped (gherkin only allows
# them under the top-level Feature:). @Rule/@Scenario tag numbers are
# renumbered globally within this file (each source's local 01, 02, ...
# becomes a continuous sequence) so namako sees no duplicate-tag collisions.
# ============================================================================

@Feature(messaging)
Feature: Messaging Channel Semantics


  # Auto-applied prelude — every Scenario in this file gets this
  # Given run before its own Givens (idempotent).
  Background:
    Given a server is running

  # ==========================================================================
  # === Source: 03_messaging.feature ===
  # ==========================================================================

  @Rule(01)
  Rule: Channel direction enforcement

    @Scenario(01)
    Scenario: [messaging-04] Sending on wrong direction returns error
      Given a server is running
      And a client connects
      When the client sends on a server-to-client channel
      Then the send returns an error

    @Scenario(02)
    Scenario: [messaging-04] Channel direction violation does not cause panic
      Given a server is running
      And a client connects
      When the client sends on a server-to-client channel
      Then the send returns an error
      And no panic occurs

    @Scenario(03)
    Scenario: [messaging-04] Channel direction violation does not disrupt the connection
      Given a server is running
      And a client connects
      When the client sends on a server-to-client channel
      Then the send returns an error
      And no connection disruption occurs

  # --------------------------------------------------------------------------
  # Rule: OrderedReliable delivery
  # --------------------------------------------------------------------------
  # Reliable + strict send-order delivery.
  # --------------------------------------------------------------------------
  @Rule(02)
  Rule: OrderedReliable delivery

    @Scenario(01)
    Scenario: [messaging-05] OrderedReliable delivers messages in send order
      Given a server is running
      And a client connects
      When the server sends messages A B C on an ordered reliable channel
      Then the client receives messages A B C in order

    @Scenario(02)
    Scenario: [messaging-06] OrderedReliable deduplicates messages
      Given a server is running
      And a client connects
      When the server sends message A on an ordered reliable channel
      Then the client receives message A exactly once

  # --------------------------------------------------------------------------
  # Rule: Request/Response matching
  # --------------------------------------------------------------------------
  # Response is matched to Request by Request ID.
  # --------------------------------------------------------------------------
  @Rule(03)
  Rule: Request/Response matching

    @Scenario(01)
    Scenario: [messaging-21] Response is delivered to correct request handler
      Given a server is running
      And a client connects
      When the client sends a request
      And the server responds to the request
      Then the client receives the response for that request

    @Scenario(02)
    Scenario: [messaging-21] Request-response flow completes without panic
      Given a server is running
      And a client connects
      When the client sends a request
      And the server responds to the request
      Then the client receives the response for that request
      And no panic occurs

    @Scenario(03)
    Scenario: [messaging-22] Sequential requests receive matching responses
      Given a server is running
      And a client connects
      When the client sends a request
      And the server responds to the request
      And the client sends a request
      And the server responds to the request
      Then the client receives the response for that request

    @Scenario(04)
    Scenario: [messaging-21] Request-response matching is not disrupted by connection state
      Given a server is running
      And a client connects
      When the client sends a request
      And the server responds to the request
      Then the client receives the response for that request
      And no connection disruption occurs

  # ──────────────────────────────────────────────────────────────────────
  # Phase D.3 — coverage stubs (deferred)
  # ──────────────────────────────────────────────────────────────────────
  #
  # 03_messaging.rs has 24 unique messaging-NN contracts; the 6 above
  # cover the most-exercised channel-direction, ordered-reliable, and
  # RPC paths. The remaining 21 IDs (handshake errors, channel matrix,
  # RPC corner cases, EntityProperty buffering, tick-buffered
  # behavior) are tagged `@Deferred` (Category B) — testable behaviors
  # waiting for a real Scenario body. Convert in Q5.

  @Rule(04)
  Rule: Coverage stubs for legacy contracts not yet expressed as Scenarios

    @Deferred
    @Scenario(01)
    Scenario: [messaging-01] User errors return Result
      Then the system intentionally fails

    @Deferred
    @Scenario(02)
    Scenario: [messaging-02] Remote/untrusted input does not panic
      Then the system intentionally fails

    @Deferred
    @Scenario(03)
    Scenario: [messaging-03] Wire-format errors surface cleanly
      Then the system intentionally fails

    @Deferred
    @Scenario(04)
    Scenario: [messaging-07] OrderedReliable preserves order under reordering
      Then the system intentionally fails

    @Deferred
    @Scenario(05)
    Scenario: [messaging-08] OrderedReliable preserves order under jitter
      Then the system intentionally fails

    @Deferred
    @Scenario(06)
    Scenario: [messaging-09] UnorderedReliable delivers all but in any order
      Then the system intentionally fails

    @Deferred
    @Scenario(07)
    Scenario: [messaging-10] UnorderedUnreliable best-effort semantics
      Then the system intentionally fails

    @Deferred
    @Scenario(08)
    Scenario: [messaging-11] SequencedUnreliable discards late updates
      Then the system intentionally fails

    @Deferred
    @Scenario(09)
    Scenario: [messaging-12] SequencedReliable exposes only latest
      Then the system intentionally fails

    @Deferred
    @Scenario(10)
    Scenario: [messaging-13] TickBuffered groups messages by tick
      Then the system intentionally fails

    @Deferred
    @Scenario(11)
    Scenario: [messaging-14] TickBuffered discards too-old ticks
      Then the system intentionally fails

    @Deferred
    @Scenario(12)
    Scenario: [messaging-15] TickBuffered discards too-far-ahead ticks
      Then the system intentionally fails

    @Deferred
    @Scenario(13)
    Scenario: [messaging-16] Reliable channel allows fragmentation
      Then the system intentionally fails

    @Deferred
    @Scenario(14)
    Scenario: [messaging-17] Unreliable fragmentation drops oversize
      Then the system intentionally fails

    @Deferred
    @Scenario(15)
    Scenario: [messaging-18] EntityProperty message buffering
      Then the system intentionally fails

    @Deferred
    @Scenario(16)
    Scenario: [messaging-19] EntityProperty message TTL
      Then the system intentionally fails

    @Deferred
    @Scenario(17)
    Scenario: [messaging-20] EntityProperty buffer caps with FIFO eviction
      Then the system intentionally fails

    @Deferred
    @Scenario(18)
    Scenario: [messaging-23] Request-response timeout semantics
      Then the system intentionally fails

    @Deferred
    @Scenario(19)
    Scenario: [messaging-24] Request-response IDs are unique
      Then the system intentionally fails

    @Deferred
    @Scenario(20)
    Scenario: [messaging-25] Disconnect cancels pending requests
      Then the system intentionally fails

    @Deferred
    @Scenario(21)
    Scenario: [messaging-26] Concurrent requests stay isolated per client
      Then the system intentionally fails

    @Deferred
    @Scenario(22)
    Scenario: [messaging-27] Reliable point-to-point request-response
      Then the system intentionally fails

