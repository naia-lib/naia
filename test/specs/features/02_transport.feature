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
# ----------------------------------------------------------------------------
# TRANSPORT ASSUMPTIONS
# ----------------------------------------------------------------------------
#
# Naia assumes transport is unordered and unreliable:
#   - Naia MUST assume packets may be dropped, duplicated, and reordered
#   - Naia MUST NOT rely on:
#     * in-order delivery
#     * exactly-once delivery
#     * guaranteed delivery
#
# ----------------------------------------------------------------------------
# MTU BOUNDARY
# ----------------------------------------------------------------------------
#
# MTU boundary is defined by naia_shared::MTU_SIZE_BYTES:
#   - Naia MUST treat MTU_SIZE_BYTES as max size of single packet payload
#   - Naia MUST NOT ask transport to send payload larger than MTU_SIZE_BYTES
#
# Oversize outbound packet attempt returns Err:
#   - If Naia is asked to send data requiring payload > MTU_SIZE_BYTES
#   - Naia MUST return Result::Err from the initiating Naia-layer API
#   - Naia MUST validate before calling the adapter
#
# ----------------------------------------------------------------------------
# INBOUND PACKET HANDLING
# ----------------------------------------------------------------------------
#
# Malformed or oversize inbound packets are dropped:
#   - If Naia receives packet > MTU_SIZE_BYTES or malformed:
#     * In Prod: drop silently
#     * In Debug: drop and emit warning (text not part of contract)
#
# ----------------------------------------------------------------------------
# TRANSPORT ABSTRACTION GUARANTEE
# ----------------------------------------------------------------------------
#
# No transport-specific guarantees may leak upward:
#   - Higher layers (messaging/replication) MUST behave identically
#     regardless of underlying transport quality
#   - Any guarantee stronger than transport assumptions MUST be in messaging spec
#
# ============================================================================


@Feature(transport_layer_contract)
Feature: Transport Layer Contract

  # --------------------------------------------------------------------------
  # Rule: MTU boundary enforcement for outbound packets
  # --------------------------------------------------------------------------
  # Naia MUST NOT send packets larger than MTU_SIZE_BYTES.
  # Attempts to send oversize payloads MUST return Err before calling adapter.
  # --------------------------------------------------------------------------
  @Rule(01)
  Rule: MTU boundary enforcement for outbound packets

    @Scenario(01)
    Scenario: Server can send packet within MTU limit
      Given a server is running
      And a client connects
      When the server sends a packet within the MTU limit
      Then the operation succeeds

    @Scenario(02)
    Scenario: Client can send packet within MTU limit
      Given a server is running
      And a client connects
      When the client sends a packet within the MTU limit
      Then the operation succeeds

    @Scenario(03)
    Scenario: Server rejects outbound packet exceeding MTU
      Given a server is running
      And a client connects
      When the server attempts to send a packet exceeding MTU
      Then the operation returns an Err result
      And the transport adapter is not called

    @Scenario(04)
    Scenario: Client rejects outbound packet exceeding MTU
      Given a server is running
      And a client connects
      When the client attempts to send a packet exceeding MTU
      Then the operation returns an Err result
      And the transport adapter is not called

  # --------------------------------------------------------------------------
  # Rule: Inbound packet handling for oversize and malformed packets
  # --------------------------------------------------------------------------
  # Packets exceeding MTU_SIZE_BYTES or malformed MUST be dropped.
  # In prod: silent drop. In debug: drop with warning (non-normative text).
  # --------------------------------------------------------------------------
  @Rule(02)
  Rule: Inbound packet handling for oversize and malformed packets

    @Scenario(01)
    Scenario: Server drops inbound packet exceeding MTU
      Given a server is running
      And a client connects
      When the server receives a packet exceeding MTU
      Then the packet is dropped
      And no panic occurs
      And no connection disruption occurs

    @Scenario(02)
    Scenario: Client drops inbound packet exceeding MTU
      Given a server is running
      And a client connects
      When the client receives a packet exceeding MTU
      Then the packet is dropped
      And no panic occurs
      And no connection disruption occurs

    @Scenario(03)
    Scenario: Server drops malformed inbound packet
      Given a server is running
      And a client connects
      When the server receives a malformed packet
      Then the packet is dropped
      And no panic occurs
      And no connection disruption occurs

    @Scenario(04)
    Scenario: Client drops malformed inbound packet
      Given a server is running
      And a client connects
      When the client receives a malformed packet
      Then the packet is dropped
      And no panic occurs
      And no connection disruption occurs

  # --------------------------------------------------------------------------
  # Rule: Transport unreliability tolerance
  # --------------------------------------------------------------------------
  # Naia MUST tolerate packet loss, duplication, and reordering without panic.
  # Higher-layer semantics (reliability, ordering) belong to messaging layer.
  # --------------------------------------------------------------------------
  @Rule(03)
  Rule: Transport unreliability tolerance

    @Scenario(01)
    Scenario: Server tolerates packet loss
      Given a server is running
      And a client connects
      When packets from the client are dropped by the transport
      Then the server continues operating normally
      And no panic occurs

    @Scenario(02)
    Scenario: Client tolerates packet loss
      Given a server is running
      And a client connects
      When packets from the server are dropped by the transport
      Then the client continues operating normally
      And no panic occurs

    @Scenario(03)
    Scenario: Server tolerates duplicate packets
      Given a server is running
      And a client connects
      When the server receives duplicate packets
      Then the server handles them without panic
      And no connection disruption occurs

    @Scenario(04)
    Scenario: Client tolerates duplicate packets
      Given a server is running
      And a client connects
      When the client receives duplicate packets
      Then the client handles them without panic
      And no connection disruption occurs

    @Scenario(05)
    Scenario: Server tolerates reordered packets
      Given a server is running
      And a client connects
      When the server receives packets in a different order than sent
      Then the server handles them without panic
      And no connection disruption occurs

    @Scenario(06)
    Scenario: Client tolerates reordered packets
      Given a server is running
      And a client connects
      When the client receives packets in a different order than sent
      Then the client handles them without panic
      And no connection disruption occurs

  # --------------------------------------------------------------------------
  # Rule: Transport abstraction independence
  # --------------------------------------------------------------------------
  # Higher layers MUST behave identically regardless of transport quality.
  # Transport-specific guarantees MUST NOT leak to application layer.
  # --------------------------------------------------------------------------
  @Rule(04)
  Rule: Transport abstraction independence

    @Scenario(01)
    Scenario: Application behavior is identical across transport types
      Given multiple transport adapters with different quality characteristics
      When the same application logic runs on each transport
      Then observable application behavior is identical
      And no transport-specific guarantees are exposed


