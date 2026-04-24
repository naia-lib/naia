# ============================================================================
# Connection Lifecycle — Canonical Contract
# ============================================================================
# Source: contracts/01_connection_lifecycle.spec.md
# Last updated: 2026-01-17
#
# Summary:
#   This specification defines the connection state machine and observable
#   events for Naia connections. Covers client/server states, authentication,
#   identity tokens, handshake, tick sync, rejection, disconnect, reconnect,
#   and protocol identity. Intentionally written at the Naia core API level;
#   engine adapters (bevy) MUST preserve these semantics.
# ============================================================================

# ============================================================================
# NORMATIVE CONTRACT MIRROR
# ============================================================================
#
# PURPOSE:
#   Define the connection state machine and observable events for Naia
#   connections at the core API level. Engine adapters (bevy) MUST
#   preserve these semantics; adapter-specific plumbing is out of scope.
#
# GLOSSARY:
#   - Client: Naia client instance attempting to establish session with Server
#   - Server: Naia server instance accepting client sessions
#   - Transport: Underlying network mechanism (UDP, WebRTC)
#   - Session: Period from "connected" until "disconnected"
#   - Explicit reject: Server deliberately refuses connection observably
#   - Auth request: Credential payload sent Client → Server out-of-band (HTTP)
#   - Identity token: Opaque one-time token for transport handshake
#   - Protocol crate: Shared Rust crate defining message/component/channel registry
#   - protocol_id: Deterministic 128-bit identifier for wire-relevant surface
#   - Wire-relevant surface: Any aspect affecting encoding/decoding/semantics on wire
#
# ----------------------------------------------------------------------------
# OBSERVABLE SIGNALS
# ----------------------------------------------------------------------------
#
# Client-side:
#   - ConnectionStatus: MUST have no "Rejected" state; rejection is RejectEvent
#   - ConnectEvent: Exactly once per successful session, after handshake finalized
#   - DisconnectEvent: Only if previously connected (emitted ConnectEvent)
#   - RejectEvent: Only on explicit server rejection, not generic failures
#
# Server-side:
#   - AuthEvent: When require_auth=true and auth request received (exactly once per request)
#   - ConnectEvent: When session fully established (handshake + tick sync complete)
#   - DisconnectEvent: When established session ends
#
# ----------------------------------------------------------------------------
# STATE MACHINES
# ----------------------------------------------------------------------------
#
# Client states (conceptual):
#   - Disconnected → Connecting → Connected
#   - Client behavior MUST be describable by these states
#   - No public "Rejected" state; rejection is an event only
#
# Server states (per-client-session conceptual):
#   - NoSession → Handshaking → Connected
#   - Server MUST NOT treat client as Connected until handshake finalized
#
# ----------------------------------------------------------------------------
# AUTHENTICATION RULES
# ----------------------------------------------------------------------------
#
# When require_auth=false:
#   - Server MUST allow connection without pre-auth step
#   - Optional app-level auth MAY be supported but not required by Naia
#
# When require_auth=true:
#   - Client MUST obtain identity token via HTTP BEFORE transport connection
#   - Server MUST return 200 OK + token on valid auth, or 401 Unauthorized
#   - Server MUST emit exactly one AuthEvent per auth request
#   - No Naia-level auth timeout during handshake (auth completed before transport)
#
# ----------------------------------------------------------------------------
# IDENTITY TOKEN PROPERTIES
# ----------------------------------------------------------------------------
#
#   - One-time use: MUST NOT be used successfully more than once
#   - TTL = 1 hour from issuance
#   - On first successful validation, server MUST mark token as consumed
#   - Expired/used/invalid token MUST cause explicit rejection
#   - Required for ALL transports when require_auth=true
#
# ----------------------------------------------------------------------------
# HANDSHAKE AND TICK SYNC
# ----------------------------------------------------------------------------
#
# Handshake ordering:
#   1. Transport connection established
#   2. protocol_id exchange and comparison (HARD GATE)
#   3. Auth validation (if require_auth=true)
#   4. Tick synchronization
#   5. ConnectEvent emitted (connection ready)
#
# Timing rules:
#   - Client MUST emit ConnectEvent only after handshake finalized
#   - Server MUST emit ConnectEvent only after handshake finalized
#   - MUST NOT deliver entity replication writes until after ConnectEvent
#
# ----------------------------------------------------------------------------
# PROTOCOL IDENTITY
# ----------------------------------------------------------------------------
#
# protocol_id derivation (MUST include):
#   - Channel registry: kinds, modes, directions, registration order
#   - Message type registry: type IDs, field schemas, field order, registration order
#   - Request/Response type registry: type IDs, field schemas, registration order
#   - Component type registry: type IDs, field schemas, replicated field order, registration order
#   - Naia wire protocol version
#
# Stability guarantees:
#   - MUST be deterministic: identical source → identical protocol_id
#   - MUST change if any wire-relevant surface changes
#   - MAY remain same for non-wire-relevant changes (docs, non-replicated fields)
#
# Wire encoding:
#   - 16-byte (128-bit) unsigned integer, little-endian byte order
#
# Handshake gate:
#   - protocol_id comparison MUST occur BEFORE:
#     - ConnectEvent, entity replication, messages, auth validation
#   - Mismatch MUST cause ProtocolMismatch rejection
#   - No partial compatibility, no negotiation, exact match required
#
# ----------------------------------------------------------------------------
# REJECTION SEMANTICS
# ----------------------------------------------------------------------------
#
# Server MUST explicitly reject when:
#   - require_auth=true and no identity token presented
#   - Token is invalid/expired/already-used
#   - protocol_id mismatch (ProtocolMismatch)
#   - Server otherwise denies before session establishment
#
# On rejection:
#   - Client MUST emit RejectEvent
#   - Client MUST NOT emit ConnectEvent
#   - Client MUST NOT emit DisconnectEvent
#   - After RejectEvent, ConnectionStatus MUST return to non-connected state
#
# ----------------------------------------------------------------------------
# DISCONNECT SEMANTICS
# ----------------------------------------------------------------------------
#
#   - DisconnectEvent (client/server) MUST only emit if ConnectEvent was emitted
#   - On disconnect: client is out-of-scope for all entities
#   - Client-owned entities MUST be despawned by server
#
# ----------------------------------------------------------------------------
# EVENT ORDERING GUARANTEES
# ----------------------------------------------------------------------------
#
# With require_auth=true (server):
#   AuthEvent → ConnectEvent → DisconnectEvent
#
# With require_auth=false (server):
#   ConnectEvent → DisconnectEvent
#
# Client (all modes):
#   ConnectEvent → DisconnectEvent (successful session)
#   RejectEvent only (rejected attempt, no Connect/Disconnect)
#
# ----------------------------------------------------------------------------
# RECONNECT SEMANTICS
# ----------------------------------------------------------------------------
#
#   - Reconnect is a FRESH session, no resumption
#   - Server treats reconnecting client as new session
#   - Prior entity state, authority, buffered data discarded
#   - Client receives fresh entity spawns (not updates)
#   - Authority state starts fresh (no carryover)
#
# ----------------------------------------------------------------------------
# NON-GOALS / OUT OF SCOPE
# ----------------------------------------------------------------------------
#
#   - Exact HTTP route/headers/body format of auth request
#   - Transport-specific wire details for token conveyance
#   - Engine adapter (bevy) implementation details
#   - Retry/backoff policies for connection attempts
#   - Session resumption / state persistence across reconnects
#   - Wire format details for protocol identity exchange
#
# ============================================================================


@Feature(connection_lifecycle)
Feature: Connection Lifecycle

  # --------------------------------------------------------------------------
  # Rule: Event ordering
  # --------------------------------------------------------------------------
  # require_auth=false: ConnectEvent → DisconnectEvent
  # --------------------------------------------------------------------------
  @Rule(01)
  Rule: Event ordering

    @Scenario(01)
    Scenario: Server observes ConnectEvent when client connects
      Given a server is running
      When a client connects
      Then the server has observed ConnectEvent

    @Scenario(02)
    Scenario: Client observes ConnectEvent when connected
      Given a server is running
      When a client connects
      Then the client has observed ConnectEvent

    @Scenario(03)
    Scenario: Client observes DisconnectEvent after disconnect
      Given a server is running
      And a client connects
      When the server disconnects the client
      Then the client has observed DisconnectEvent

    @Scenario(04)
    Scenario: DisconnectEvent occurs only after ConnectEvent on server
      Given a server is running
      And a client connects
      When the server disconnects the client
      Then the server observed ConnectEvent before DisconnectEvent

    @Scenario(05)
    Scenario: DisconnectEvent occurs only after ConnectEvent on client
      Given a server is running
      And a client connects
      When the server disconnects the client
      Then the client observed ConnectEvent before DisconnectEvent

    # [connection-lifecycle-21] — Client DisconnectEvent ordering via polling assertion
    # Polling variant of the ordering guarantee: waits for disconnect then checks order.
    @Scenario(06)
    Scenario: connection-21 — Client observes DisconnectEvent only after ConnectEvent
      Given a server is running
      And a client connects
      When the server disconnects the client
      Then the client observes DisconnectEvent after ConnectEvent

    # [connection-lifecycle-connect] — Client observes ConnectEvent via polling
    # Polling variant of the client ConnectEvent assertion.
    @Scenario(07)
    Scenario: connection-lifecycle — Client observes ConnectEvent polling variant
      Given a server is running
      When a connected client
      Then the client observes ConnectEvent

  # --------------------------------------------------------------------------
  # Rule: Disconnect semantics
  # --------------------------------------------------------------------------
  # DisconnectEvent only after ConnectEvent.
  # --------------------------------------------------------------------------
  @Rule(02)
  Rule: Disconnect semantics

    @Scenario(01)
    Scenario: Server observes DisconnectEvent when client disconnects
      Given a server is running
      And a client connects
      When the server disconnects the client
      Then the server has observed DisconnectEvent

    @Scenario(02)
    Scenario: Connected client count decreases after disconnect
      Given a server is running
      And a client connects
      Then the server has 1 connected client
      When the server disconnects the client
      Then the server has 0 connected clients

    @Scenario(03)
    Scenario: Server can connect multiple clients
      Given a server is running
      When a client connects
      And a client connects
      Then the server has 2 connected clients

    @Scenario(04)
    Scenario: Server can disconnect one of multiple clients
      Given a server is running
      And a client connects
      And a client connects
      When the server disconnects the client
      Then the server has 1 connected client

    @Scenario(05)
    Scenario: Client is connected after successful connection
      Given a server is running
      When a client connects
      Then the client is connected

    @Scenario(06)
    Scenario: Client is not connected after disconnect
      Given a server is running
      And a client connects
      When the server disconnects the client
      Then the client is not connected

    # [connection-lifecycle-users-count] — Server has no users after all disconnect
    # After all clients disconnect, the server MUST report zero connected users.
    @Scenario(07)
    Scenario: connection-lifecycle — Server has no connected users after all clients disconnect
      Given a server is running
      And a client connects
      When the server disconnects the client
      Then the server has no connected users

  # --------------------------------------------------------------------------
  # Rule: Auth-required event ordering
  # --------------------------------------------------------------------------
  # require_auth=true: AuthEvent → ConnectEvent → DisconnectEvent
  # --------------------------------------------------------------------------
  @Rule(03)
  Rule: Auth-required event ordering

    @Scenario(01)
    Scenario: Server observes AuthEvent before ConnectEvent
      Given a server is running with auth required
      When a client authenticates and connects
      Then the server observes AuthEvent before ConnectEvent

    @Scenario(02)
    Scenario: Rejected client observes RejectEvent not ConnectEvent
      Given a server is running with auth required
      When a client attempts to connect but is rejected
      Then the client observes RejectEvent
      And the client does not observe ConnectEvent
      And the client does not observe DisconnectEvent

    @Scenario(03)
    Scenario: Server full event ordering with disconnect
      Given a server is running with auth required
      When a client authenticates and connects
      When the server disconnects the client
      Then the server observes DisconnectEvent after ConnectEvent


# ============================================================================
# DEFERRED TESTS
# ============================================================================
# All other scenarios deferred until step bindings are implemented.
# See contracts/01_connection_lifecycle.spec.md for full scenario list.
# ============================================================================

