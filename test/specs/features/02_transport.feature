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


Feature: Transport Layer Contract

  # All executable scenarios deferred until step bindings implemented.

