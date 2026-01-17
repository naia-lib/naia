# ============================================================================
# Transport — Canonical Contract
# ============================================================================
# Source: contracts/02_transport.spec.md
# Last converted: 2026-01-17
#
# Summary:
#   This specification defines the transport boundary contract for Naia.
#   Naia is transport-agnostic (UDP, WebRTC, local channels) and assumes
#   all transports are unordered/unreliable. Reliability, ordering, and
#   fragmentation belong to the messaging spec, not here.
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
#   Define Naia's assumptions about the transport layer, MTU boundary,
#   and error behavior for malformed/oversize packets.
#
# GLOSSARY:
#   - Transport adapter: Implementation used by Naia to send/receive packets
#   - Packet: Single datagram-like unit delivered by the transport
#   - Packet payload: Bytes Naia asks transport to send in one packet
#   - MTU_SIZE_BYTES: Maximum packet payload allowed by Naia core
#   - Prod: debug_assertions disabled
#   - Debug: debug_assertions enabled
#
# SCOPE:
#   This spec owns:
#   - Naia's assumptions about the transport layer
#   - Naia's packet-size boundary (MTU) and error behavior
#   - Naia's behavior on malformed/oversize inbound packets
#
#   This spec does NOT own:
#   - Socket-crate-specific behavior (naia_client_socket, naia_server_socket)
#   - Message reliability/ordering/fragmentation (see 03_messaging)
#   - Entity replication semantics (see entity suite)
#   - Auth semantics (see 01_connection_lifecycle)
#
# NORMATIVE TRANSPORT RULES:
#   [transport-01] Naia assumes transport is unordered & unreliable
#     - Naia MUST assume packets may be dropped, duplicated, and reordered
#     - Naia MUST NOT rely on:
#       * in-order delivery
#       * exactly-once delivery
#       * guaranteed delivery
#
#   [transport-02] MTU boundary is defined by naia_shared::MTU_SIZE_BYTES
#     - Naia MUST treat MTU_SIZE_BYTES as max size of single packet payload
#     - Naia MUST NOT ask transport to send payload larger than MTU_SIZE_BYTES
#
#   [transport-03] Oversize outbound packet attempt returns Err
#     - If Naia is asked to send data requiring payload > MTU_SIZE_BYTES
#     - Naia MUST return Result::Err from the initiating Naia-layer API
#     - Naia must validate before calling the adapter
#
#   [transport-04] Malformed or oversize inbound packets are dropped
#     - If Naia receives packet > MTU_SIZE_BYTES or malformed:
#       * In Prod: drop silently
#       * In Debug: drop and emit warning (text not part of contract)
#
#   [transport-05] No transport-specific guarantees may leak upward
#     - Higher layers (messaging/replication) MUST behave identically
#       regardless of underlying transport quality
#     - Any guarantee stronger than transport-01 MUST be in messaging spec
#
# ============================================================================

Feature: Transport Layer Contract

  Background:
    Given a Naia test environment is initialized

  # --------------------------------------------------------------------------
  # Rule: Transport is assumed unordered and unreliable
  # --------------------------------------------------------------------------
  # NORMATIVE: Naia MUST assume packets may be dropped, duplicated, and
  # reordered. Naia MUST NOT rely on in-order, exactly-once, or guaranteed
  # delivery.
  # --------------------------------------------------------------------------
  Rule: Transport is assumed unordered and unreliable

    Scenario: Naia tolerates packet reordering
      Given a connected client and server
      And a transport conditioner that reorders packets
      When messages are exchanged
      Then Naia handles reordered packets correctly
      And no panic occurs

    Scenario: Naia tolerates packet drops
      Given a connected client and server
      And a transport conditioner that drops packets
      When messages are exchanged
      Then Naia handles dropped packets correctly
      And no panic occurs

    Scenario: Naia tolerates packet duplication
      Given a connected client and server
      And a transport conditioner that duplicates packets
      When messages are exchanged
      Then Naia handles duplicate packets correctly
      And no panic occurs

  # --------------------------------------------------------------------------
  # Rule: MTU boundary is enforced
  # --------------------------------------------------------------------------
  # NORMATIVE: MTU_SIZE_BYTES is the maximum packet payload. Naia MUST NOT
  # send payloads larger than this.
  # --------------------------------------------------------------------------
  Rule: MTU boundary is enforced

    Scenario: Oversize outbound attempt returns Err
      Given a connected client and server
      When the client attempts to send data exceeding MTU_SIZE_BYTES
      Then the send operation returns an Err result
      And no panic occurs

  # --------------------------------------------------------------------------
  # Rule: Malformed or oversize inbound packets are dropped
  # --------------------------------------------------------------------------
  # NORMATIVE: Malformed or oversize inbound packets are dropped silently
  # in Prod, with warning in Debug.
  # --------------------------------------------------------------------------
  Rule: Malformed or oversize inbound packets are dropped

    Scenario: Malformed inbound packet is dropped
      Given a connected client and server
      When the server receives a malformed packet
      Then the packet is dropped
      And no panic occurs

    Scenario: Oversize inbound packet is dropped
      Given a connected client and server
      When the server receives an oversize packet
      Then the packet is dropped
      And no panic occurs

  # --------------------------------------------------------------------------
  # Rule: Transport guarantees do not leak upward
  # --------------------------------------------------------------------------
  # NORMATIVE: Higher layers MUST behave identically regardless of whether
  # underlying transport happens to be "better" (e.g., local channels).
  # --------------------------------------------------------------------------
  Rule: Transport guarantees do not leak upward

    Scenario: Behavior is identical across transport types
      Given a test scenario definition
      When the scenario runs over UDP transport
      And the scenario runs over local transport
      Then the observable behavior is identical

# ============================================================================
# AMBIGUITIES + PROPOSED CLARIFICATIONS
# ============================================================================
# None identified. The transport spec is clear and well-defined.
# ============================================================================
