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
#   engine adapters (hecs/bevy) MUST preserve these semantics.
# ============================================================================

# ============================================================================
# NORMATIVE CONTRACT MIRROR
# ============================================================================
#
# PURPOSE:
#   Define the connection state machine and observable events for Naia
#   connections at the core API level. Engine adapters (hecs/bevy) MUST
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
#   - Engine adapter (bevy/hecs) implementation details
#   - Retry/backoff policies for connection attempts
#   - Session resumption / state persistence across reconnects
#   - Wire format details for protocol identity exchange
#
# ============================================================================

Feature: Connection Lifecycle

  Background:
    Given a Naia test environment is initialized

  # --------------------------------------------------------------------------
  # Rule: Client state machine
  # --------------------------------------------------------------------------
  # Client states are Disconnected, Connecting, Connected.
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
  # Server not Connected until handshake + tick sync done.
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
  # Clients can connect without pre-auth step.
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
  # Identity token required via HTTP before transport handshake.
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
  # One-time use, TTL = 1 hour, required for all transports.
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
  # Rule: Handshake and tick sync
  # --------------------------------------------------------------------------
  # ConnectEvent only after protocol_id verification and tick sync.
  # --------------------------------------------------------------------------
  Rule: Handshake and tick sync

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
  # Rule: Protocol identity is a strict handshake gate
  # --------------------------------------------------------------------------
  # protocol_id verified as first check; mismatch = ProtocolMismatch rejection.
  # Exact match required, no negotiation, no partial compatibility.
  # --------------------------------------------------------------------------
  Rule: Protocol identity is a strict handshake gate

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

    Scenario: Breaking protocol change causes ProtocolMismatch
      Given a server with updated protocol
      When an old client attempts to connect
      Then ProtocolMismatch rejection occurs

    Scenario: No extension negotiation occurs
      Given client and server with different protocol_id
      When connection is attempted
      Then no negotiation occurs
      And connection is rejected immediately

  # --------------------------------------------------------------------------
  # Rule: Rejection semantics
  # --------------------------------------------------------------------------
  # RejectEvent emitted, no ConnectEvent, no DisconnectEvent.
  # --------------------------------------------------------------------------
  Rule: Rejection semantics

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
  # DisconnectEvent only after ConnectEvent.
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
  # Rule: Event ordering
  # --------------------------------------------------------------------------
  # require_auth=true: AuthEvent → ConnectEvent → DisconnectEvent
  # require_auth=false: ConnectEvent → DisconnectEvent
  # Rejected: RejectEvent only, no Connect/Disconnect
  # --------------------------------------------------------------------------
  Rule: Event ordering

    Scenario: Server observes correct event order with auth
      Given require_auth is true
      When a client authenticates and connects
      Then the server observes AuthEvent before ConnectEvent
      When the client disconnects
      Then the server observes DisconnectEvent after ConnectEvent

    Scenario: Server observes correct event order without auth
      Given require_auth is false
      When a client connects
      Then the server observes ConnectEvent
      When the client disconnects
      Then the server observes DisconnectEvent after ConnectEvent

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
  # No session resumption; world rebuilt from scratch.
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
  # protocol_id is deterministic; changes with wire-relevant changes.
  # --------------------------------------------------------------------------
  Rule: Protocol identity determinism

    Scenario: Wire-relevant changes produce different protocol_id
      Given a protocol with specific channel configuration
      When the channel configuration changes
      Then the protocol_id changes

    Scenario: Non-wire-relevant changes do not affect protocol_id
      Given a protocol with documentation changes only
      Then the protocol_id does not change

# ============================================================================
# DEFERRED TESTS
# ============================================================================
# Items that cannot be tested with current harness capabilities.
# ============================================================================
#
# Rule: Identity token properties
#   Assertions:
#     - Token TTL enforcement (1 hour expiry)
#     - Token replay detection across process restarts
#   Harness needs: Time manipulation / clock injection
#
# Rule: Protocol identity wire format
#   Assertions:
#     - protocol_id is encoded as 16 bytes little-endian on wire
#     - Different component schemas produce different protocol_id
#   Harness needs: Wire-level packet inspection
#
# Rule: Authentication HTTP flow
#   Assertions:
#     - HTTP 200 OK with token body format
#     - HTTP 401 Unauthorized response format
#   Harness needs: HTTP request/response interception
#
# ============================================================================

# ============================================================================
# AMBIGUITIES + PROPOSED CLARIFICATIONS
# ============================================================================
# None identified.
# ============================================================================
