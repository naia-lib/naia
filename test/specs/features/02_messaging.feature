# ============================================================================
# Messaging Channel Semantics — Grouped Contract Suite
# ============================================================================
# This file is the post-A.4 grouping of multiple source feature files into
# a single grouped suite per the SDD migration plan. Each `# === Source: ... ===`
# block below corresponds to one of the original 24 .feature files.
# ============================================================================

@Feature(02_messaging)
Feature: Messaging Channel Semantics

  # ==========================================================================
  # === Source: 03_messaging.feature ===
  # ==========================================================================


  # --------------------------------------------------------------------------
  # Rule: Channel direction enforcement
  # --------------------------------------------------------------------------
  # ChannelDirection is enforced at send-time.
  # --------------------------------------------------------------------------
  @Rule(01)
  Rule: Channel direction enforcement

    @Scenario(01)
    Scenario: Sending on wrong direction returns error
      Given a server is running
      And a client connects
      When the client sends on a server-to-client channel
      Then the send returns an error

    @Scenario(02)
    Scenario: Channel direction violation does not cause panic
      Given a server is running
      And a client connects
      When the client sends on a server-to-client channel
      Then the send returns an error
      And no panic occurs

    @Scenario(03)
    Scenario: Channel direction violation does not disrupt the connection
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
    Scenario: OrderedReliable delivers messages in send order
      Given a server is running
      And a client connects
      When the server sends messages A B C on an ordered reliable channel
      Then the client receives messages A B C in order

    @Scenario(02)
    Scenario: OrderedReliable deduplicates messages
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
    Scenario: Response is delivered to correct request handler
      Given a server is running
      And a client connects
      When the client sends a request
      And the server responds to the request
      Then the client receives the response for that request

    @Scenario(02)
    Scenario: Request-response flow completes without panic
      Given a server is running
      And a client connects
      When the client sends a request
      And the server responds to the request
      Then the client receives the response for that request
      And no panic occurs

    @Scenario(03)
    Scenario: Sequential requests receive matching responses
      Given a server is running
      And a client connects
      When the client sends a request
      And the server responds to the request
      And the client sends a request
      And the server responds to the request
      Then the client receives the response for that request

    @Scenario(04)
    Scenario: Request-response matching is not disrupted by connection state
      Given a server is running
      And a client connects
      When the client sends a request
      And the server responds to the request
      Then the client receives the response for that request
      And no connection disruption occurs


