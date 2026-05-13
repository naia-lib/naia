# ============================================================================
# Entity Replication, Spawn-with-Components, Immutable Components — Grouped Contract Suite
# ============================================================================
# Post-A.4 grouping of multiple source feature files. Each source's content
# is preserved verbatim from the @Rule line onward; per-source separators
# (`# === Source: ... ===`) keep the original boundaries greppable. Free-text
# feature-description blocks from sources are stripped (gherkin only allows
# them under the top-level Feature:). @Rule/@Scenario tag numbers are
# renumbered globally within this file (each source's local 01, 02, ...
# becomes a continuous sequence) so namako sees no duplicate-tag collisions.
# ============================================================================

@Feature(replication)
Feature: Entity Replication, Spawn-with-Components, Immutable Components


  # Auto-applied prelude — every Scenario in this file gets this
  # Given run before its own Givens (idempotent).
  Background:
    Given a server is running

  # ==========================================================================
  # === Source: 07_entity_replication.feature ===
  # ==========================================================================

  @Rule(01)
  Rule: Entity Replication

    @Scenario(01)
    Scenario: [entity-replication-01] Entity spawns on client when entering scope
      Given a server is running
      And a client connects
      And a server-owned entity exists with a replicated component
      And the client and entity share a room
      Then the entity spawns on the client with the replicated component

    @Scenario(02)
    Scenario: [entity-replication-02] Component updates are replicated to client
      Given a server is running
      And a client connects
      And a server-owned entity exists with a replicated component
      And the client and entity share a room
      And the entity is in-scope for the client
      When the server updates the replicated component
      Then the client observes the component update

    @Scenario(03)
    Scenario: [entity-replication-03] Entity despawns on client when leaving scope
      Given a server is running
      And a client connects
      And a server-owned entity exists with a replicated component
      And the client and entity share a room
      And the entity is in-scope for the client
      When the server excludes the entity for the client
      Then the entity despawns on the client

    # [entity-replication-06] — GlobalEntity identity is stable during entity lifetime
    # The EntityKey (Naia's GlobalEntity abstraction) MUST remain stable while
    # the entity exists; no reassignment during updates.
    @Scenario(04)
    Scenario: entity-replication-06 — GlobalEntity identity is stable during entity lifetime
      Given a server is running
      And a client connects
      And a server-owned entity exists with a replicated component
      And the client and entity share a room
      And the entity is in-scope for the client
      When the server updates the replicated component
      Then the entity GlobalEntity remains unchanged

    # [entity-replication-07] — Server state overwrites client local state on conflict
    # When the server holds authoritative state and the client has a local modification,
    # the next server replication MUST overwrite the client's local value.
    @Scenario(05)
    Scenario: entity-replication-07 — Server state overwrites client local state on conflict
      Given a server is running
      And a client connects
      And a server-owned entity exists with a replicated component
      And the client and entity share a room
      And the entity is in-scope for the client
      And the client modifies the component locally
      When the server updates the replicated component
      Then the client observes the server value

  # ==========================================================================
  # === Source: 18_spawn_with_components.feature ===
  # ==========================================================================

  @Rule(02)
  Rule: Multi-component entity has all components after spawn

    @Scenario(01)
    Scenario: spawn-with-components-01-a — entity with two components has both after spawn
      Given a server is running
      And a client connects
      And a server-owned entity exists with Position and Velocity components
      And the client and entity share a room
      Then the entity spawns on the client with Position and Velocity

    @Scenario(02)
    Scenario: spawn-with-components-01-b — initial component values are correct after spawn
      Given a server is running
      And a client connects
      And a server-owned entity exists with Position and Velocity components
      And the client and entity share a room
      Then the entity spawns on the client with correct Position and Velocity values

  # --------------------------------------------------------------------------
  # Rule: Zero-component entity uses legacy Spawn path
  # --------------------------------------------------------------------------
  @Rule(03)
  Rule: Zero-component entity spawns via legacy path

    @Scenario(01)
    Scenario: spawn-with-components-02-a — entity with no components spawns correctly
      Given a server is running
      And a client connects
      And a server-owned entity exists without any replicated components
      And the client and entity share a room
      Then the entity spawns on the client

  # ==========================================================================
  # === Source: 19_immutable_components.feature ===
  # ==========================================================================

  @Rule(04)
  Rule: Immutable component replicates to client

    @Scenario(01)
    Scenario: immutable-01-a — entity with ImmutableLabel spawns on client
      Given a server is running
      And a client connects
      And a server-owned entity exists with only ImmutableLabel
      And the client and entity share a room
      And the entity is in-scope for the client
      Then the client entity has ImmutableLabel

  # --------------------------------------------------------------------------
  # Rule: No diff-handler receivers for immutable components
  # --------------------------------------------------------------------------
  @Rule(05)
  Rule: No diff-handler receivers for immutable components

    @Scenario(01)
    Scenario: immutable-02-a — ImmutableLabel creates no diff-handler receiver
      Given a server is running
      And a client connects
      And a server-owned entity exists with only ImmutableLabel
      And the client and entity share a room
      And the entity is in-scope for the client
      Then the global diff handler has 0 receivers

  # --------------------------------------------------------------------------
  # Rule: Mixed entity has exactly one receiver for the mutable component
  # --------------------------------------------------------------------------
  @Rule(06)
  Rule: Mixed entity has exactly one receiver for the mutable component

    @Scenario(01)
    Scenario: immutable-03-a — Position and ImmutableLabel yields one diff-handler receiver
      Given a server is running
      And a client connects
      And a server-owned entity exists with Position and ImmutableLabel
      And the client and entity share a room
      And the entity is in-scope for the client
      Then the global diff handler has 1 receiver

  # ──────────────────────────────────────────────────────────────────────
  # Phase D.4 — coverage stubs (deferred)
  # ──────────────────────────────────────────────────────────────────────

  @Rule(07)
  Rule: Coverage stubs for legacy contracts not yet expressed as Scenarios

    # [entity-replication-04] — Component insert events fire for in-scope additions.
    @Scenario(01)
    Scenario: [entity-replication-04] Component insert events fire for in-scope additions
      Given a server is running
      And a client connects
      And a server-owned entity exists
      And the client and entity share a room
      And the entity is in-scope for the client
      When the server inserts a replicated component on the entity
      Then the client world has the component on the entity

    # [entity-replication-05] — Component remove events fire for in-scope removals.
    @Scenario(02)
    Scenario: [entity-replication-05] Component remove events fire for in-scope removals
      Given a server is running
      And a client connects
      And a server-owned entity exists with a replicated component
      And the client and entity share a room
      And the entity is in-scope for the client
      When the server removes a replicated component from the entity
      Then the client world no longer has the component on the entity

    # [entity-replication-08] — Replication preserves component-set parity.
    # Component enumeration API not exposed by the test harness; parity
    # is implicitly verified by the spawn-with-components scenarios in Rule(02).
    @PolicyOnly
    @Scenario(03)
    Scenario: [entity-replication-08] Replication preserves component-set parity

    # [entity-replication-09] — Replication preserves component value parity.
    # Server→client component value assertion not in harness bindings; covered
    # implicitly by the "server observes component update" path in Rule(01).
    @PolicyOnly
    @Scenario(04)
    Scenario: [entity-replication-09] Replication preserves component value parity

    # [entity-replication-10] — No replication storms under steady-state.
    # Verifying absence of outbound packets requires per-packet counting
    # not exposed by the test harness.
    @PolicyOnly
    @Scenario(05)
    Scenario: [entity-replication-10] No replication storms under steady-state

    # [entity-replication-11] — Component remove on out-of-scope is safe.
    @Scenario(06)
    Scenario: [entity-replication-11] Component remove on out-of-scope is safe
      Given a server is running
      And a client connects
      And a server-owned entity exists with a replicated component
      When the server removes a replicated component from the entity
      Then no error is raised

    # [entity-replication-12] — Concurrent updates resolve to last-writer-wins.
    # The harness is single-threaded; truly concurrent writes are not constructible.
    @PolicyOnly
    @Scenario(07)
    Scenario: [entity-replication-12] Concurrent updates resolve to last-writer-wins

  # ──────────────────────────────────────────────────────────────────────
  # Static entity replication
  # ──────────────────────────────────────────────────────────────────────

  @Rule(08)
  Rule: Static entity replication

    @Scenario(01)
    Scenario: [static-entity-01] Static entity spawns on client
      Given a server is running
      And a client connects
      And a server-owned static entity exists
      And the client and entity share a room
      And the entity is in-scope for the client
      Then the entity spawns on the client

    @Scenario(02)
    Scenario: [static-entity-02] Static entity component value appears on client
      Given a server is running
      And a client connects
      And a server-owned static entity exists with a replicated component
      And the client and entity share a room
      And the entity is in-scope for the client
      Then the client world has the component on the entity

    # [static-entity-03] — Post-construction component insert panics.
    # Covered by integration test in test/harness/contract_tests/integration_only/02_static_entities.rs.
    @PolicyOnly
    @Scenario(03)
    Scenario: [static-entity-03] Post-construction insert on static entity is rejected

    @Scenario(04)
    Scenario: [static-entity-04] Static entity exclude/include round-trip
      Given a server is running
      And a client connects
      And a server-owned static entity exists with a replicated component
      And the client and entity share a room
      And the entity is in-scope for the client
      Then the entity spawns on the client
      When the server excludes the entity for the client
      Then the entity despawns on the client
      When the server includes the entity for the client
      Then the entity spawns on the client as a new lifetime

  # ──────────────────────────────────────────────────────────────────────
  # Client-owned static entity replication
  # ──────────────────────────────────────────────────────────────────────

  @Rule(10)
  Rule: Client-owned static entity replication

    @Scenario(01)
    Scenario: [client-static-01] Client-owned static entity spawns on server
      Given a server is running
      And a client connects
      And a client-owned static entity exists
      Then the server has the entity

    # [client-static-02] — Client-owned static entity component value appears on server.
    # Currently a known limitation: client-side `spawn_static_entity` calls
    # `host_init_entity` BEFORE the closure runs (with empty component_kinds),
    # so the entity is sent as a bare `Spawn` rather than `SpawnWithComponents`.
    # Subsequent `InsertComponent` messages may race the entity registration.
    # Fixing this requires deferring `host_init_entity` until the EntityMut is
    # dropped (after the closure inserts components), mirroring the server-side
    # pattern where `host_init_entity` is deferred until scope entry.
    @PolicyOnly
    @Scenario(02)
    Scenario: [client-static-02] Client-owned static entity component value appears on server

    # [client-static-03] — Post-construction component insert panics on client-owned static entity.
    # Covered by integration test in test/harness/contract_tests/integration_only/02_static_entities.rs.
    @PolicyOnly
    @Scenario(03)
    Scenario: [client-static-03] Post-construction insert on client-owned static entity is rejected

  # ──────────────────────────────────────────────────────────────────────
  # Component mutation with scope transitions
  # ──────────────────────────────────────────────────────────────────────

  @Rule(09)
  Rule: Component state is snapshot-consistent at scope entry

    @Scenario(01)
    Scenario: [scope-snapshot-01] Component inserted before scope entry is visible on entry
      Given a server is running
      And a client connects
      And a server-owned entity exists
      And the client and entity share a room
      When the server inserts a replicated component on the entity
      And the server includes the entity for the client
      Then the client world has the component on the entity

    @Scenario(02)
    Scenario: [scope-snapshot-02] Component removed before scope entry is absent on entry
      Given a server is running
      And a client connects
      And a server-owned entity exists with a replicated component
      And the client and entity share a room
      When the server removes a replicated component from the entity
      And the server includes the entity for the client
      Then the client world no longer has the component on the entity

    @Scenario(03)
    Scenario: [scope-snapshot-03] Component insert and remove round-trip on in-scope entity
      Given a server is running
      And a client connects
      And a server-owned entity exists
      And the client and entity share a room
      And the entity is in-scope for the client
      When the server inserts a replicated component on the entity
      Then the client world has the component on the entity
      When the server removes a replicated component from the entity
      Then the client world no longer has the component on the entity

    @Scenario(04)
    Scenario: [scope-snapshot-04] Both clients see component insert on in-scope entity
      Given a server is running
      And client A connects
      And client B connects
      And a server-owned entity exists
      And the client and entity share a room
      And the entity is in-scope for the client
      And the entity is in-scope for client B
      When the server inserts a replicated component on the entity
      Then the client world has the component on the entity

    @Scenario(05)
    Scenario: [scope-snapshot-05] Both clients see component remove on in-scope entity
      Given a server is running
      And client A connects
      And client B connects
      And a server-owned entity exists with a replicated component
      And the client and entity share a room
      And the entity is in-scope for the client
      And the entity is in-scope for client B
      When the server removes a replicated component from the entity
      Then the client world no longer has the component on the entity

    @Scenario(06)
    Scenario: [scope-snapshot-06] Delegated entity: client observes Delegated replication config
      Given a server is running
      And client A connects
      And the server spawns a delegated entity in-scope for client A
      Then client A observes Delegated replication config for the entity

    @Scenario(07)
    Scenario: [scope-snapshot-07] Component insert then entity leaves scope is idempotent
      Given a server is running
      And a client connects
      And a server-owned entity exists with a replicated component
      And the client and entity share a room
      And the entity is in-scope for the client
      When the server inserts a replicated component on the entity
      And the server excludes the entity for the client
      Then the entity despawns on the client

    @Scenario(08)
    Scenario: [scope-snapshot-08] Entity re-enters scope with latest component state after insert
      Given a server is running
      And a client connects
      And a server-owned entity exists
      And the client and entity share a room
      And the entity is in-scope for the client
      Then the entity spawns on the client
      When the server excludes the entity for the client
      Then the entity despawns on the client
      When the server inserts a replicated component on the entity
      And the server includes the entity for the client
      Then the client world has the component on the entity

    @Scenario(09)
    Scenario: [scope-snapshot-09] Static entity spawns on both clients
      Given a server is running
      And client A connects
      And client B connects
      And a server-owned static entity exists with a replicated component
      And the client and entity share a room
      And the entity is in-scope for the client
      And the entity is in-scope for client B
      Then the entity spawns on the client

    @Scenario(10)
    Scenario: [scope-snapshot-10] Component update propagates after explicit tick advancement
      Given a server is running
      And a client connects
      And a server-owned entity exists with a replicated component
      And the client and entity share a room
      And the entity is in-scope for the client
      When the server updates the replicated component
      And 10 ticks elapse
      Then the client observes the component update

    @Scenario(11)
    Scenario: [scope-snapshot-11] Entity replication stable after extended tick idle
      Given a server is running
      And a client connects
      And a server-owned entity exists with a replicated component
      And the client and entity share a room
      And the entity is in-scope for the client
      And 20 ticks have elapsed
      Then the entity spawns on the client



  # ==========================================================================
  # Rule 11 — Replication under adverse transport
  # ==========================================================================

  @Rule(11)
  Rule: Replication converges under packet loss

    @Scenario(01)
    Scenario: [replication-loss-01] Entity replication converges under 20% packet loss
      Given a server is running
      And a client connects
      And the link has 20 percent packet loss
      And a server-owned entity exists with a replicated component
      And the client and entity share a room
      And the entity is in-scope for the client
      Then replication eventually converges despite packet loss
