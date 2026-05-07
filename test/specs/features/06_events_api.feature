# ============================================================================
# Server/Client Events API, World Integration, Priority Accumulator — Grouped Contract Suite
# ============================================================================
# Post-A.4 grouping of multiple source feature files. Each source's content
# is preserved verbatim from the @Rule line onward; per-source separators
# (`# === Source: ... ===`) keep the original boundaries greppable. Free-text
# feature-description blocks from sources are stripped (gherkin only allows
# them under the top-level Feature:). @Rule/@Scenario tag numbers are
# renumbered globally within this file (each source's local 01, 02, ...
# becomes a continuous sequence) so namako sees no duplicate-tag collisions.
# ============================================================================

@Feature(events_api)
Feature: Server/Client Events API, World Integration, Priority Accumulator


  # Auto-applied prelude — every Scenario in this file gets this
  # Given run before its own Givens (idempotent).
  Background:
    Given a server is running

  # ==========================================================================
  # === Source: 12_server_events_api.feature ===
  # ==========================================================================

  @Rule(01)
  Rule: Server Events API

    # [server-events-07] — Entity spawn events are per-user and in-scope only
    # When E enters scope for user U, the server MUST fire exactly one SpawnEntityEvent for (U, E).
    # Out-of-scope users MUST NOT receive spawn events.
    @Scenario(01)
    Scenario: [server-events-07] Server spawn event fires for in-scope user only
      Given a server is running
      And client A connects
      And a server-owned entity enters scope for client A
      Then the server observes a spawn event for client A

    # [server-events-09] — Despawn events are exactly-once per user lifecycle
    # When E leaves scope for U, exactly one despawn/exit event MUST fire for (U, E).
    @Scenario(02)
    Scenario: [server-events-09] Server despawn event fires when entity leaves scope
      Given a server is running
      And client A connects
      And a server-owned entity enters scope for client A
      And the server has observed a spawn event for client A
      When the server removes the entity from client A's scope
      Then the server observes a despawn event for client A

    # [server-events-XX] — Authority grant events are observable server-side
    # When the server grants authority to a client, an EntityAuthGrantEvent MUST fire
    # on the server for the granting user and entity.
    @Scenario(03)
    Scenario: server-events-XX — Server observes authority grant event when client is granted
      Given a server is running
      And client A connects
      And the server spawns a delegated entity in-scope for client A
      When client A requests authority for the delegated entity
      Then the server observes an authority grant event for client A

    # [server-events-XX] — Authority reset events are observable server-side
    # When a client releases authority, a ServerEntityAuthResetEvent MUST fire on the
    # server for the entity, signaling the authority returned to Available.
    @Scenario(04)
    Scenario: server-events-XX — Server observes authority reset event when client releases
      Given a server is running
      And client A connects
      And the server spawns a delegated entity in-scope for client A
      When client A requests authority for the delegated entity
      Then client A is granted authority for the delegated entity
      When client A releases authority for the delegated entity
      Then the server observes an authority reset event

    # [server-events-XX] — Publish events are observable server-side
    # When a client makes its entity Public, a ServerPublishEntityEvent MUST fire
    # on the server for the publishing client and entity.
    @Scenario(05)
    Scenario: server-events-XX — Server observes publish event when client publishes entity
      Given a server is running
      And client A connects
      And client A spawns a client-owned entity with Private replication config
      When client A publishes the entity
      Then the server observes a publish event for client A

  # ==========================================================================
  # === Source: 13_client_events_api.feature ===
  # ==========================================================================

  @Rule(02)
  Rule: Client Events API

    # [client-events-04] — Spawn is the first event for an entity lifetime
    # The client MUST receive a SpawnEntityEvent when an entity enters scope.
    @Scenario(01)
    Scenario: [client-events-04] Client receives spawn event when entity enters scope
      Given a server is running
      And a client connects
      And a server-owned entity exists
      And the client and entity share a room
      Then the client receives a spawn event for the entity

    # [client-events-09] — Scope transitions are reflected as spawn/despawn events
    # Leaving scope MUST emit Despawn; re-entering scope MUST emit a new Spawn.
    @Scenario(02)
    Scenario: [client-events-09] Scope leave emits Despawn; re-enter emits Spawn
      Given a server is running
      And a client connects
      And a server-owned entity exists
      And the client and entity share a room
      And the entity is in-scope for the client
      When the server excludes the entity for the client
      Then the client receives a despawn event for the entity
      When the server includes the entity for the client
      Then the client receives a spawn event for the entity

    # [client-events-07] — Component update events are one-shot per applied change
    # When the server updates a replicated component, the client Events API MUST
    # surface exactly one component update event for that applied change.
    @Scenario(03)
    Scenario: [client-events-07] Client receives component update event via Events API
      Given a server is running
      And a client connects
      And a server-owned entity exists with a replicated component
      And the client and entity share a room
      And the entity is in-scope for the client
      When the server updates the replicated component
      Then the client receives a component update event for the entity

    # [client-events-08] — Component remove events are one-shot per applied removal
    # When the server removes a replicated component from an in-scope entity, the
    # client Events API MUST surface exactly one component remove event for that change.
    @Scenario(04)
    Scenario: [client-events-08] Client receives component remove event via Events API
      Given a server is running
      And a client connects
      And a server-owned entity exists with a replicated component
      And the client and entity share a room
      And the entity is in-scope for the client
      When the server removes the replicated component
      Then the client receives a component remove event for the entity

    # [client-events-06] — Component insert events are one-shot per applied insertion
    # When the server inserts a replicated component into an already-in-scope entity,
    # the client Events API MUST surface exactly one component insert event.
    @Scenario(05)
    Scenario: [client-events-06] Client receives component insert event via Events API
      Given a server is running
      And a client connects
      And a server-owned entity exists without a replicated component
      And the client and entity share a room
      And the entity is in-scope for the client
      When the server inserts the replicated component
      Then the client receives a component insert event for the entity

  # ==========================================================================
  # === Source: 14_world_integration.feature ===
  # ==========================================================================

  @Rule(03)
  Rule: World Integration

    # [world-integration-01/04] — Client world mirrors Naia view; scope drives presence
    # After scope changes, entity presence in the client world MUST match Naia's view.
    @Scenario(01)
    Scenario: [world-integration-04] Entity presence in client world mirrors scope state
      Given a server is running
      And a client connects
      And a server-owned entity exists
      And the client and entity share a room
      Then the entity is in-scope for the client
      When the server excludes the entity for the client
      Then the entity is out-of-scope for the client
      When the server includes the entity for the client
      Then the entity is in-scope for the client

    # [world-integration-05] — Late-joining client world is built from current server state
    # A second client joining a running game MUST see current entities, not stale state.
    @Scenario(02)
    Scenario: [world-integration-05] Late-joining client receives current server snapshot
      Given a server is running
      And a client connects
      And a server-owned entity exists with a replicated component
      And the client and entity share a room
      And the entity is in-scope for the client
      When a second client connects and the entity enters scope for it
      Then the second client has the entity in its world

    # [world-integration-07] — Component type correctness: values match the server's authoritative state
    # The client's replicated component values MUST match what the server wrote.
    @Scenario(03)
    Scenario: [world-integration-07] Component values in client world match server state
      Given a server is running
      And a client connects
      And a server-owned entity exists with a replicated component
      And the client and entity share a room
      And the entity is in-scope for the client
      When the server updates the replicated component
      Then the client observes the component update

    # [world-integration-09] — Component removal is reflected in client world
    # When the server removes a replicated component from an in-scope entity,
    # the client's world MUST no longer contain that component value.
    @Scenario(04)
    Scenario: [world-integration-09] Component removal propagates to client world
      Given a server is running
      And a client connects
      And a server-owned entity exists with a replicated component
      And the client and entity share a room
      And the entity is in-scope for the client
      When the server removes the replicated component
      Then the client world no longer has the component on the entity

    # [world-integration-08] — Component insert is reflected in client world
    # When the server inserts a replicated component into an already-in-scope entity,
    # the client's world MUST converge to include that component.
    @Scenario(05)
    Scenario: [world-integration-08] Component insert propagates to client world
      Given a server is running
      And a client connects
      And a server-owned entity exists without a replicated component
      And the client and entity share a room
      And the entity is in-scope for the client
      When the server inserts the replicated component
      Then the client world has the component on the entity

    # [world-integration-06] — Disconnect cleans External World fully
    # Zero-leak lifecycle cleanup: after client disconnect, the client world
    # MUST NOT retain entities from the session.
    @Scenario(06)
    Scenario: [world-integration-06] Disconnect cleans client world of all server entities
      Given a server is running
      And a client connects
      And a server-owned entity exists
      And the client and entity share a room
      And the entity is in-scope for the client
      When the client disconnects
      Then the entity despawns on the client

  # ==========================================================================
  # === Source: 20_priority_accumulator.feature ===
  # ==========================================================================

  @Rule(04)
  Rule: Spawn-burst drains under budget

    @Scenario(01)
    Scenario: priority-accumulator-01-a — 16-entity spawn burst eventually reaches client
      Given a server is running
      And a client connects
      When the server spawns 16 entities in one tick and scopes them for the client
      Then the client eventually observes all 16 spawned entities

  # --------------------------------------------------------------------------
  # Rule: Gain persistence across send (B-BDD-6)
  # set_gain(N) survives the send cycle.
  # --------------------------------------------------------------------------
  @Rule(05)
  Rule: Gain override persists across send cycle

    @Scenario(01)
    Scenario: priority-accumulator-02-a — global gain stays set after entity replicates
      Given a server is running
      And a client connects
      And a server-owned entity exists with a replicated component
      And the client and entity share a room
      And the entity is in-scope for the client
      When the server sets the global priority gain on the last entity to 5.0
      Then the global priority gain on the last entity is 5.0
      And the client eventually sees the last entity
      And the global priority gain on the last entity is still 5.0

  # --------------------------------------------------------------------------
  # Rule: Per-entity value convergence under cross-entity reorder (B-BDD-8)
  # Clients converge to the latest server value for each entity even when
  # the send-time priority sort reorders bundles across entities.
  # --------------------------------------------------------------------------
  @Rule(06)
  Rule: Per-entity value convergence under cross-entity reorder

    @Scenario(01)
    Scenario: priority-accumulator-03-a — interleaved mutations converge per-entity
      Given a server is running
      And a client connects
      And two server-owned entities A and B exist each with a replicated component in-scope for the client
      When the server mutates entity A's component to x=10 y=20
      And the server mutates entity B's component to x=30 y=40
      And the server mutates entity A's component to x=50 y=60
      Then the client eventually observes entity A at x=50 y=60
      And the client eventually observes entity B at x=30 y=40


  # ──────────────────────────────────────────────────────────────────────
  # Phase D.7 — coverage stubs (deferred)
  # ──────────────────────────────────────────────────────────────────────

  @Rule(07)
  Rule: Coverage stubs for legacy contracts not yet expressed as Scenarios

    @Deferred
    @Scenario(01)
    Scenario: [server-events-00] Server events API surface contract

    @Deferred
    @Scenario(02)
    Scenario: [server-events-01] ConnectEvent fires per accepted client

    @Deferred
    @Scenario(03)
    Scenario: [server-events-02] DisconnectEvent fires on client drop

    @Deferred
    @Scenario(04)
    Scenario: [server-events-03] AuthEvent surfaces on server-side request

    @Deferred
    @Scenario(05)
    Scenario: [server-events-04] TickEvent fires per server tick

    @Deferred
    @Scenario(06)
    Scenario: [server-events-05] MessageEvent fires per inbound message

    @Deferred
    @Scenario(07)
    Scenario: [server-events-06] RequestEvent surfaces request payload

    @Deferred
    @Scenario(08)
    Scenario: [server-events-08] Per-user event isolation (no cross-user leakage)

    @Deferred
    @Scenario(09)
    Scenario: [server-events-10] Authority denied event observable on server

    @Deferred
    @Scenario(10)
    Scenario: [server-events-11] Authority release event observable on server

    @Deferred
    @Scenario(11)
    Scenario: [server-events-12] Publish event observable on server

    @Deferred
    @Scenario(12)
    Scenario: [server-events-13] Unpublish event observable on server

    @Deferred
    @Scenario(13)
    Scenario: [client-events-00] Client events API surface contract

    @Deferred
    @Scenario(14)
    Scenario: [client-events-01] ConnectEvent fires on accepted handshake

    @Deferred
    @Scenario(15)
    Scenario: [client-events-02] DisconnectEvent fires on link loss

    @Deferred
    @Scenario(16)
    Scenario: [client-events-03] RejectEvent fires on protocol mismatch

    @Deferred
    @Scenario(17)
    Scenario: [client-events-05] TickEvent fires per client tick

    @Deferred
    @Scenario(18)
    Scenario: [client-events-10] Authority granted event surfaces

    @Deferred
    @Scenario(19)
    Scenario: [client-events-11] Authority denied event surfaces

    @Deferred
    @Scenario(20)
    Scenario: [client-events-12] Authority reset event surfaces

    @Deferred
    @Scenario(21)
    Scenario: [world-integration-01] Entity insertion into ECS world via Events API

    @Deferred
    @Scenario(22)
    Scenario: [world-integration-02] Component insertion mirrors server insert

    @Deferred
    @Scenario(23)
    Scenario: [world-integration-03] Component removal mirrors server remove

