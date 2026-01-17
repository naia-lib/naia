# ============================================================================
# Connection Lifecycle — Canonical Contract
# ============================================================================
# Source: contracts/01_connection_lifecycle.spec.md
# Last converted: 2026-01-17
#
# Summary:
#   This specification defines the connection state machine and observable
#   events for Naia connections. Covers client/server states, authentication,
#   identity tokens, handshake, tick sync, rejection, disconnect, reconnect,
#   and protocol identity.
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
#   Define the connection state machine and observable events for Naia
#   connections at the core API level.
#
# GLOSSARY:
#   - Client: Naia client instance attempting to establish session with Server
#   - Server: Naia server instance accepting client sessions
#   - Transport: Underlying network mechanism (UDP, WebRTC)
#   - Session: Period from "connected" until "disconnected"
#   - Explicit reject: Server deliberately refuses connection observably
#   - Auth request: Credential payload sent Client → Server out-of-band (HTTP)
#   - Identity token: Opaque one-time token for transport handshake
#   - protocol_id: Deterministic 128-bit identifier for wire-relevant surface
#
# CLIENT-SIDE OBSERVABLE SIGNALS:
#   - ConnectionStatus: No "Rejected" state; rejection is RejectEvent
#   - ConnectEvent: Exactly once per successful session, after handshake
#   - DisconnectEvent: Only if previously connected
#   - RejectEvent: Only on explicit server rejection, not generic failures
#
# SERVER-SIDE OBSERVABLE SIGNALS:
#   - AuthEvent: When require_auth=true and auth request received
#   - ConnectEvent: When session fully established
#   - DisconnectEvent: When established session ends
#
# STATE MACHINES:
#   Client: Disconnected → Connecting → Connected
#   Server (per-client): NoSession → Handshaking → Connected
#
# CORE CONTRACTS:
#   [connection-01] Client states are Disconnected/Connecting/Connected
#   [connection-02] No public "Rejected" state; rejection is an event
#   [connection-03] Server not "Connected" until handshake + tick sync done
#   [connection-04] require_auth=false allows connection without pre-auth
#   [connection-05] Optional app-level auth allowed when not required
#   [connection-06] require_auth=true requires identity token via HTTP first
#   [connection-07] Auth response: 200 OK + token or 401 Unauthorized
#   [connection-08] Server emits exactly one AuthEvent per auth request
#   [connection-09] No Naia-level auth timeout during handshake
#   [connection-10] Token is one-time use, TTL = 1 hour
#   [connection-11] Invalid/expired/used token causes explicit rejection
#   [connection-12] Tokens required for all transports when require_auth=true
#   [connection-13] Token marked consumed on first successful validation
#   [connection-14] Handshake includes tick sync; not Connected until done
#   [connection-14a] protocol_id verified as first check in handshake
#   [connection-15] Client emits ConnectEvent only after handshake finalized
#   [connection-16] Server emits ConnectEvent only after handshake finalized
#   [connection-17] No entity writes until after ConnectEvent
#   [connection-18] Server rejects: no token, invalid token, or server choice
#   [connection-19] On reject: RejectEvent, no ConnectEvent, no DisconnectEvent
#   [connection-20] After RejectEvent, status returns to non-connected
#   [connection-21] Client DisconnectEvent only after ConnectEvent
#   [connection-22] Server DisconnectEvent only after ConnectEvent
#   [connection-23] Disconnect = out-of-scope + client-owned entities despawn
#   [connection-24] require_auth=true: AuthEvent → ConnectEvent → DisconnectEvent
#   [connection-25] require_auth=false: ConnectEvent → DisconnectEvent
#   [connection-26] Client: ConnectEvent → DisconnectEvent
#   [connection-27] Rejected: RejectEvent only, no Connect/Disconnect
#   [connection-28] Reconnect is fresh session, no resumption
#   [connection-29] protocol_id uniquely identifies wire-relevant surface
#   [connection-30] protocol_id is 16-byte little-endian u128
#   [connection-31] protocol_id mismatch = ProtocolMismatch rejection
#   [connection-32] Wire-relevant changes affect protocol_id
#   [connection-33] No partial compatibility, exact match required
#
# ============================================================================

Feature: Connection Lifecycle

  Background:
    Given a Naia test environment is initialized

  # --------------------------------------------------------------------------
  # Rule: Client state machine
  # --------------------------------------------------------------------------
  # NORMATIVE: Client states are Disconnected, Connecting, Connected.
  # No public "Rejected" state exists.
  # --------------------------------------------------------------------------
  Rule: Client state machine

    Scenario: Client follows standard state progression
      Given a client in Disconnected state
      When the client initiates connection
      Then the client enters Connecting state
      When the handshake completes
      Then the client enters Connected state

    Scenario: No public Rejected state exists
      Given a client is rejected by the server
      Then the client emits RejectEvent
      And the client returns to Disconnected state

  # --------------------------------------------------------------------------
  # Rule: Server state machine per client
  # --------------------------------------------------------------------------
  # NORMATIVE: Server not Connected until handshake + tick sync done.
  # --------------------------------------------------------------------------
  Rule: Server state machine per client

    Scenario: Server transitions to Connected after handshake
      Given a client is handshaking with the server
      When handshake and tick sync complete
      Then the server considers the client Connected

    Scenario: Server does not treat client as Connected during handshake
      Given a client is in handshaking state
      Then the server does not treat client as Connected
      And no entity replication occurs to that client

  # --------------------------------------------------------------------------
  # Rule: Authentication with require_auth=false
  # --------------------------------------------------------------------------
  # NORMATIVE: Clients can connect without pre-auth step.
  # --------------------------------------------------------------------------
  Rule: Authentication with require_auth=false

    Scenario: Client connects without pre-auth when not required
      Given require_auth is false
      When a client attempts to connect
      Then no auth step is required
      And connection may proceed to handshake

  # --------------------------------------------------------------------------
  # Rule: Authentication with require_auth=true
  # --------------------------------------------------------------------------
  # NORMATIVE: Identity token required via HTTP before transport handshake.
  # --------------------------------------------------------------------------
  Rule: Authentication with require_auth=true

    Scenario: Client must obtain token before transport connection
      Given require_auth is true
      When a client attempts to connect
      Then the client must first obtain an identity token via HTTP

    Scenario: Server returns 200 OK with token on valid auth
      Given require_auth is true
      When a client sends valid credentials
      Then the server returns 200 OK with identity token

    Scenario: Server returns 401 Unauthorized on invalid auth
      Given require_auth is true
      When a client sends invalid credentials
      Then the server returns 401 Unauthorized
      And no identity token is provided

    Scenario: Server emits AuthEvent for each auth request
      Given require_auth is true
      When the server receives an auth request
      Then exactly one AuthEvent is emitted

  # --------------------------------------------------------------------------
  # Rule: Identity token properties
  # --------------------------------------------------------------------------
  # NORMATIVE: One-time use, TTL = 1 hour, required for all transports.
  # --------------------------------------------------------------------------
  Rule: Identity token properties

    Scenario: Token can only be used once
      Given a valid identity token
      When the token is used for connection
      Then the token is marked as consumed
      And subsequent use fails

    Scenario: Token expires after TTL
      Given an identity token older than 1 hour
      When the token is used for connection
      Then the connection is rejected

    Scenario: Token required for all transports when auth required
      Given require_auth is true
      When connecting via any transport
      Then the identity token is required

  # --------------------------------------------------------------------------
  # Rule: Transport handshake and tick sync
  # --------------------------------------------------------------------------
  # NORMATIVE: ConnectEvent only after protocol_id verification and tick sync.
  # --------------------------------------------------------------------------
  Rule: Transport handshake and tick sync

    Scenario: Tick sync completes before ConnectEvent
      Given a client is connecting
      When the handshake reaches tick sync
      Then tick sync must complete before ConnectEvent

    Scenario: Client emits ConnectEvent only after handshake finalized
      Given a client is connecting
      When handshake is not yet complete
      Then ConnectEvent is not emitted

    Scenario: Server emits ConnectEvent only after handshake finalized
      Given a client is connecting to the server
      When handshake is not yet complete
      Then the server does not emit ConnectEvent

    Scenario: No entity writes before ConnectEvent
      Given a client is connecting
      When the client has not yet emitted ConnectEvent
      Then no entity replication writes occur

  # --------------------------------------------------------------------------
  # Rule: Protocol identity verification
  # --------------------------------------------------------------------------
  # NORMATIVE: protocol_id verified as first check; mismatch = rejection.
  # --------------------------------------------------------------------------
  Rule: Protocol identity verification

    Scenario: protocol_id verified before other handshake steps
      Given client and server have different protocol_id
      When the client attempts to connect
      Then ProtocolMismatch rejection occurs
      And no further handshake steps occur

    Scenario: Matching protocol_id allows connection to proceed
      Given client and server have matching protocol_id
      When the client attempts to connect
      Then the handshake proceeds to next step

    Scenario: ProtocolMismatch is distinguishable from other rejections
      Given client has mismatched protocol_id
      When rejection occurs
      Then the rejection reason is ProtocolMismatch

    Scenario: Different channel registrations produce different protocol_id
      Given two protocol crates with different channel registrations
      Then they have different protocol_id values

    Scenario: Same protocol crate produces same protocol_id across builds
      Given the same protocol crate source
      When built multiple times
      Then the same protocol_id is produced

  # --------------------------------------------------------------------------
  # Rule: Explicit rejection semantics
  # --------------------------------------------------------------------------
  # NORMATIVE: RejectEvent, no ConnectEvent, no DisconnectEvent.
  # --------------------------------------------------------------------------
  Rule: Explicit rejection semantics

    Scenario: Missing token causes rejection when required
      Given require_auth is true
      When a client connects without identity token
      Then the server rejects the connection

    Scenario: Rejected client emits RejectEvent
      Given a client is rejected
      Then the client emits RejectEvent
      And the client does not emit ConnectEvent
      And the client does not emit DisconnectEvent

    Scenario: After RejectEvent client returns to non-connected
      Given a client received RejectEvent
      Then the client ConnectionStatus is Disconnected

  # --------------------------------------------------------------------------
  # Rule: Disconnect semantics
  # --------------------------------------------------------------------------
  # NORMATIVE: DisconnectEvent only after ConnectEvent.
  # --------------------------------------------------------------------------
  Rule: Disconnect semantics

    Scenario: Client DisconnectEvent only after ConnectEvent
      Given a client has emitted ConnectEvent
      When the client disconnects
      Then the client emits DisconnectEvent

    Scenario: Server DisconnectEvent only after ConnectEvent
      Given the server has emitted ConnectEvent for a client
      When that client disconnects
      Then the server emits DisconnectEvent for that client

    Scenario: Disconnect causes out-of-scope for all entities
      Given a client is connected with entities in scope
      When the client disconnects
      Then the client is out-of-scope for all entities

    Scenario: Client-owned entities despawn on disconnect
      Given a client owns entities
      When the client disconnects
      Then client-owned entities are despawned by the server

  # --------------------------------------------------------------------------
  # Rule: Event ordering with auth required
  # --------------------------------------------------------------------------
  # NORMATIVE: AuthEvent → ConnectEvent → DisconnectEvent.
  # --------------------------------------------------------------------------
  Rule: Event ordering with auth required

    Scenario: Server observes correct event order with auth
      Given require_auth is true
      When a client authenticates and connects
      Then the server observes AuthEvent before ConnectEvent
      When the client disconnects
      Then the server observes DisconnectEvent after ConnectEvent

  # --------------------------------------------------------------------------
  # Rule: Event ordering without auth required
  # --------------------------------------------------------------------------
  # NORMATIVE: ConnectEvent → DisconnectEvent.
  # --------------------------------------------------------------------------
  Rule: Event ordering without auth required

    Scenario: Server observes correct event order without auth
      Given require_auth is false
      When a client connects
      Then the server observes ConnectEvent
      When the client disconnects
      Then the server observes DisconnectEvent after ConnectEvent

  # --------------------------------------------------------------------------
  # Rule: Client event ordering
  # --------------------------------------------------------------------------
  # NORMATIVE: ConnectEvent → DisconnectEvent for successful sessions.
  # --------------------------------------------------------------------------
  Rule: Client event ordering

    Scenario: Client observes correct event order
      Given a client successfully connects
      Then the client observes ConnectEvent
      When the client disconnects
      Then the client observes DisconnectEvent after ConnectEvent

    Scenario: Rejected client observes RejectEvent only
      Given a client is rejected
      Then the client observes RejectEvent
      And the client does not observe ConnectEvent
      And the client does not observe DisconnectEvent

  # --------------------------------------------------------------------------
  # Rule: Reconnect is a fresh session
  # --------------------------------------------------------------------------
  # NORMATIVE: No session resumption; world rebuilt from scratch.
  # --------------------------------------------------------------------------
  Rule: Reconnect is a fresh session

    Scenario: Reconnecting client receives fresh entity spawns
      Given a client was connected with entities
      When the client disconnects and reconnects
      Then the client receives fresh entity spawns

    Scenario: Previous session authority does not carry over
      Given a client held authority before disconnect
      When the client reconnects
      Then authority from previous session is not retained

    Scenario: Server treats reconnecting client as new session
      Given a client disconnected
      When the client reconnects
      Then the server treats it as a new session
      And prior entity state is not resumed

  # --------------------------------------------------------------------------
  # Rule: Protocol identity determinism
  # --------------------------------------------------------------------------
  # NORMATIVE: protocol_id is deterministic and changes with wire-relevant changes.
  # --------------------------------------------------------------------------
  Rule: Protocol identity determinism

    Scenario: Wire-relevant changes produce different protocol_id
      Given a protocol with specific channel configuration
      When the channel configuration changes
      Then the protocol_id changes

    Scenario: Non-wire-relevant changes do not affect protocol_id
      Given a protocol with documentation changes only
      Then the protocol_id does not change

  # --------------------------------------------------------------------------
  # Rule: No partial compatibility
  # --------------------------------------------------------------------------
  # NORMATIVE: Exact protocol_id match required, no negotiation.
  # --------------------------------------------------------------------------
  Rule: No partial compatibility

    Scenario: Breaking protocol change causes ProtocolMismatch
      Given a server with updated protocol
      When an old client attempts to connect
      Then ProtocolMismatch rejection occurs

    Scenario: No extension negotiation occurs
      Given client and server with different protocol_id
      When connection is attempted
      Then no negotiation occurs
      And connection is rejected immediately

# ============================================================================
# AMBIGUITIES + PROPOSED CLARIFICATIONS
# ============================================================================
# None identified. The connection lifecycle spec is comprehensive with clear
# state machines, event ordering, and protocol identity semantics.
# ============================================================================
