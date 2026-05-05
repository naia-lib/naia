# ============================================================================
# Replicated Resources — Canonical Contract
# ============================================================================
# Source: _AGENTS/RESOURCES_PLAN.md (v2.1)
# Last converted: 2026-05-05
#
# Summary:
#   This specification defines the Replicated Resource primitive — Naia's
#   server↔client-replicated singleton (per-`World`), modelled on Bevy's
#   `Resource` concept and implemented internally as a hidden 1-component
#   entity. Resources reuse the entity replication pipeline 100%; the only
#   new code paths are a kind-marker registry, a TypeId↔Entity map per
#   side, auto-scoping, and Bevy-side mirror systems.
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
#   Define a per-`World` singleton "Resource" type with diff-tracked,
#   per-field replication, supporting both server-authoritative and
#   client-authoritative-after-delegation modes, exposed via Bevy's
#   standard `Res<R>`/`ResMut<R>` ergonomics with zero entity/component
#   semantics visible to the user.
#
# GLOSSARY:
#   - Resource type R: A `#[derive(Replicate)]` struct registered as a
#     resource via `protocol.add_resource::<R>()`.
#   - Resource entity: The hidden naia entity that holds R as its sole
#     replicated component. Never exposed to user code.
#   - Resource registry: The per-`World` `TypeId<R>↔GlobalEntity` map
#     maintained on both sender and receiver sides.
#   - Mirror system: The bevy-adapter sync system that bridges the bevy
#     `Resource` storage and the resource-entity component storage.
#
# ----------------------------------------------------------------------------
# REGISTRATION
# ----------------------------------------------------------------------------
#
# Resources are protocol-time-registered:
#   - `protocol.add_resource::<R>()` registers R as both a ComponentKind
#     (via add_component::<R>()) AND in the ResourceKinds marker table.
#   - Idempotent — re-registering the same type is a no-op.
#
# Receiver-side discovery is implicit:
#   - On `SpawnWithComponents` arrival, if any component kind is in
#     `protocol.resource_kinds`, the receiver records the entity in its
#     `ResourceRegistry` keyed by the corresponding TypeId.
#   - Zero wire overhead — no marker component, no extra bits.
#
# ----------------------------------------------------------------------------
# LIFECYCLE
# ----------------------------------------------------------------------------
#
# Resources can be inserted/removed at runtime:
#   - `commands.replicate_resource(value)` (Bevy: dynamic ID pool)
#   - `commands.replicate_resource_static(value)` (Bevy: static ID pool)
#   - `commands.remove_replicated_resource::<R>()`
#   - Core API mirrors via `server.insert_resource`, `insert_static_resource`,
#     `remove_resource`.
#
# Re-inserting an already-existing resource is rejected:
#   - Returns `ResourceAlreadyExists` error.
#   - Existing value unchanged.
#
# ----------------------------------------------------------------------------
# REPLICATION
# ----------------------------------------------------------------------------
#
# Per-field diff tracking:
#   - Mutating a single `Property<T>` field on a resource transmits only
#     that field, identical to component update semantics.
#   - Multiple mutations to the same field within one tick coalesce.
#
# Initial sync on connect:
#   - Resource entities are auto-scoped to every connected user. A new
#     client receives all currently-existing resources in the first
#     replication packet after handshake.
#
# Late-join InsertResourceEvent:
#   - When a client first observes a resource (whether the resource was
#     just inserted or has existed since startup), it fires exactly one
#     `InsertResourceEvent<R>` from that client's perspective. Mirrors
#     `InsertComponentEvent<C>` late-join semantics. (D20)
#
# ----------------------------------------------------------------------------
# AUTHORITY (server-authoritative default; opt-in delegation)
# ----------------------------------------------------------------------------
#
# Server-authoritative resources (default):
#   - Server can mutate via `server.resource_mut::<R>()` /
#     `ResMut<R>` in Bevy systems; mutations replicate to all clients.
#   - Client mutations via `ResMut<R>` modify the local mirror but do
#     NOT propagate. Next incoming server update overwrites the local
#     change. No error returned. (D18 — soft rejection, mirrors
#     `RemoteOwnedProperty::DerefMut`.)
#
# Delegable resources (opt-in via `ReplicationConfig::delegated()`):
#   - Configured at insert time via
#     `commands.configure_replicated_resource::<R>(config)`.
#   - Client requests authority via
#     `commands.request_resource_authority::<R>(&mut client)`.
#   - On grant, client can mutate via `ResMut<R>`; mutations propagate
#     to the server.
#   - Server-side mutation while client holds authority returns
#     `AuthorityError::ClientHoldsAuthority`. (D16)
#   - Client releases via `commands.release_resource_authority::<R>()`.
#
# Disconnect-with-authority:
#   - When a client holding resource authority disconnects (gracefully
#     or not), authority reverts to `EntityAuthStatus::Available` (next
#     requester or `server.resource_take_authority` reclaims).
#   - Resource entity is NOT despawned; last-committed value persists.
#     (D15, mirrors entity behavior at server_auth_handler.rs:155.)
#
# Authority status check:
#   - User checks via `commands.resource_authority::<R>(&client)` before
#     mutating, mirroring entity-delegation usage. (D19 — manual check,
#     no SystemParam auto-gate.)
#
# ----------------------------------------------------------------------------
# PRIORITY
# ----------------------------------------------------------------------------
#
# Per-resource priority via existing entity-priority API:
#   - Default gain: 1.0 (same as any entity).
#   - Tunable via `server.resource_priority_mut::<R>().set_gain(f32)` /
#     `boost_once(f32)`. (D9)
#   - No new "Resource" priority tier; consistent with Component story.
#
# ----------------------------------------------------------------------------
# MULTI-WORLD ISOLATION
# ----------------------------------------------------------------------------
#
# Resources are per-`World`:
#   - Inserting `R` in `world_a` does not affect `world_b`'s `R`.
#   - Each World has its own `ResourceRegistry`.
#   - Mirrors entity per-World scoping.
#
# ----------------------------------------------------------------------------
# BEVY ADAPTER ERGONOMICS — ZERO ENTITY/COMPONENT SEMANTICS VISIBLE
# ----------------------------------------------------------------------------
#
# User-facing event types (mirroring component events):
#   - `InsertResourceEvent<R>` (server) / `InsertResourceEvent<T, R>` (client)
#   - `UpdateResourceEvent<R>` / `UpdateResourceEvent<T, R>`
#   - `RemoveResourceEvent<R>` / `RemoveResourceEvent<T, R>`
#   - NO `entity` field on any resource event.
#   - Registered via `app.add_resource_events::<R>()` (server) or
#     `app.add_resource_events::<T, R>()` (client) on the existing
#     `AppRegisterComponentEvents` trait. (D17)
#
# Event suppression for resource entities:
#   - The bevy-adapter event filter routes resource entities out of
#     `SpawnEntityEvent` / `DespawnEntityEvent` / component events; the
#     equivalent resource events fire instead.
#   - Users see ZERO entity-level events for resource entities. (D13)
# ============================================================================

Feature: Replicated Resources

  # --------------------------------------------------------------------------
  # Registration & basic insert/observe (dynamic + static)
  # --------------------------------------------------------------------------

  Scenario: server inserts a dynamic resource and a connected client observes it
    Given a Naia protocol with replicated resource type "Score"
    And a server and one connected client
    When the server inserts Score { home: 0, away: 0 } as a dynamic resource
    And one full replication round trip elapses
    Then the client's Score is present
    And the client's Score.home equals 0
    And the client's Score.away equals 0

  Scenario: server inserts a static resource and a connected client observes it
    Given a Naia protocol with replicated resource type "MatchState"
    And a server and one connected client
    When the server inserts MatchState { phase: "lobby" } as a static resource
    And one full replication round trip elapses
    Then the client's MatchState is present
    And the client's MatchState.phase equals "lobby"
    And the wire ID for the MatchState resource entity has is_static set to true

  Scenario: client connects after the resource was already inserted
    Given a Naia protocol with replicated resource type "Score"
    And a server with Score { home: 5, away: 2 } already inserted at startup
    When a client connects and the handshake completes
    Then the client's Score is present within the first replication packet
    And the client's Score.home equals 5
    And the client's InsertResourceEvent for Score fired exactly once

  Scenario: re-inserting an already-existing resource is rejected
    Given a Naia protocol with replicated resource type "Score"
    And a server with Score { home: 0, away: 0 } already inserted
    When the server attempts to insert Score again
    Then the operation returns a ResourceAlreadyExists error
    And the existing Score value is unchanged

  # --------------------------------------------------------------------------
  # Per-field diff updates
  # --------------------------------------------------------------------------

  Scenario: single field update transmits only the dirty field
    Given a Naia protocol with replicated resource type "Score"
    And a server with Score { home: 0, away: 0 } and one connected client
    And the initial replication round trip has elapsed
    When the server mutates Score.home to 3
    And one replication round trip elapses
    Then the client's Score.home equals 3
    And the client's Score.away equals 0
    And the most recent server-to-client packet contains exactly one Score field update bit set

  Scenario: multiple sequential field updates coalesce within a tick
    Given a Naia protocol with replicated resource type "Score"
    And a server with Score { home: 0, away: 0 } and one connected client
    And the initial replication round trip has elapsed
    When the server mutates Score.home to 1, then 2, then 3 within the same tick
    And one tick elapses
    Then the most recent server-to-client packet contains exactly one Score.home update
    And the client's Score.home equals 3

  # --------------------------------------------------------------------------
  # Removal and re-insertion
  # --------------------------------------------------------------------------

  Scenario: server removes a resource and the client observes the removal
    Given a Naia protocol with replicated resource type "MatchState"
    And a server with MatchState { phase: "lobby" } and one connected client
    And the initial replication round trip has elapsed
    When the server removes MatchState
    And one replication round trip elapses
    Then the client's MatchState is absent
    And the client's RemoveResourceEvent for MatchState fired exactly once

  Scenario: insert, remove, re-insert with different value
    Given a Naia protocol with replicated resource type "MatchState"
    And a server with one connected client
    When the server inserts MatchState { phase: "lobby" } as static
    And one replication round trip elapses
    Then the client's MatchState.phase equals "lobby"

    When the server removes MatchState
    And one replication round trip elapses
    Then the client's MatchState is absent

    When the server inserts MatchState { phase: "match" } as static
    And one replication round trip elapses
    Then the client's MatchState.phase equals "match"

  # --------------------------------------------------------------------------
  # Authority delegation (V1 client-authoritative)
  # --------------------------------------------------------------------------

  Scenario: client requests authority on a delegable resource and receives it
    Given a Naia protocol with delegable replicated resource type "PlayerSelection"
    And a server with PlayerSelection { selected_id: 0 } and connected client "alice"
    And the initial replication round trip has elapsed
    When alice requests authority on PlayerSelection
    And one replication round trip elapses
    Then alice's authority status for PlayerSelection is "Granted"

  Scenario: client-held authority allows client mutation that propagates to server
    Given a Naia protocol with delegable replicated resource type "PlayerSelection"
    And a server with PlayerSelection { selected_id: 0 } and connected client "alice"
    And alice holds authority on PlayerSelection
    When alice mutates PlayerSelection.selected_id to 7
    And one replication round trip elapses
    Then the server's PlayerSelection.selected_id equals 7

  Scenario: server-side mutation rejected while client holds authority
    Given a Naia protocol with delegable replicated resource type "PlayerSelection"
    And a server with PlayerSelection { selected_id: 0 } and connected client "alice"
    And alice holds authority on PlayerSelection
    When the server attempts to mutate PlayerSelection.selected_id to 99
    Then the attempt returns AuthorityError::ClientHoldsAuthority
    And the value remains 0

  Scenario: client releases authority and server reclaims
    Given a Naia protocol with delegable replicated resource type "PlayerSelection"
    And a server with PlayerSelection { selected_id: 0 } and connected client "alice"
    And alice holds authority on PlayerSelection
    And alice has set selected_id to 5
    When alice releases authority on PlayerSelection
    And one replication round trip elapses
    Then the server-side authority status for PlayerSelection is "Available"
    And subsequent client mutations from alice are rejected with AuthorityError::ServerHoldsAuthority

  Scenario: client disconnects while holding authority — authority reverts to Available, value persists
    Given a Naia protocol with delegable replicated resource type "PlayerSelection"
    And a server with PlayerSelection { selected_id: 0 } and connected client "alice"
    And alice holds authority on PlayerSelection
    And alice has set selected_id to 5
    When alice disconnects ungracefully
    And the server's disconnect-detection elapses
    Then the server's authority status for PlayerSelection is "Available"
    And the resource value remains the last value alice committed (5)
    And the resource is not despawned

  # --------------------------------------------------------------------------
  # Soft rejection of client writes to server-authoritative resources (D18)
  # --------------------------------------------------------------------------

  Scenario: client mutation of server-authoritative resource is silently dropped locally and overwritten by next server update
    Given a Naia protocol with replicated resource type "Score"
    And a server with Score { home: 0, away: 0 } and connected client "alice"
    And the initial replication round trip has elapsed
    When alice mutates Score.home to 99 via ResMut<Score>
    Then no replication packet is sent from alice carrying the Score.home change
    And alice's local Score.home immediately reads as 99 (local mirror was modified)
    When the server later mutates Score.home to 1
    And one replication round trip elapses
    Then alice's local Score.home equals 1 (server overwrote the local change)
    And no AuthorityError was returned at any step

  # --------------------------------------------------------------------------
  # Per-resource priority (existing entity-priority API)
  # --------------------------------------------------------------------------

  Scenario: per-resource priority gain affects send ordering under bandwidth pressure
    Given a Naia protocol with replicated resource type "Score"
    And a server with Score and 5000 dynamic entities each with Position
    And the server has set the priority gain for Score to 10.0
    And one connected client with constrained outbound bandwidth of 8 KB/tick
    And the initial replication round trip has elapsed
    When the server mutates Score.home and Position on every entity in the same tick
    Then the next outbound packet contains the Score update before any Position update

  Scenario: default priority gain is 1.0
    Given a Naia protocol with replicated resource type "Score"
    And a server with Score
    Then the server's reported priority gain for Score is 1.0

  # --------------------------------------------------------------------------
  # Multi-world isolation
  # --------------------------------------------------------------------------

  Scenario: resources in different worlds do not bleed across
    Given a Naia protocol with replicated resource type "Score"
    And a server with worlds "world_a" and "world_b" both registering Score
    When the server inserts Score { home: 1, away: 0 } in world_a
    And the server inserts Score { home: 100, away: 0 } in world_b
    Then world_a's Score.home equals 1
    And world_b's Score.home equals 100
    And mutating world_a's Score does not change world_b's Score

  # --------------------------------------------------------------------------
  # Late-join InsertResourceEvent firing (D20)
  # --------------------------------------------------------------------------

  Scenario: late-joining client receives InsertResourceEvent for pre-existing resource
    Given a Naia protocol with replicated resource type "Score"
    And a server with Score { home: 7, away: 3 } already inserted at startup
    And client "alice" already connected
    When client "bob" connects after the resource was inserted
    And the connection handshake completes
    And bob's first replication packet arrives
    Then bob receives exactly one InsertResourceEvent<Main, Score>
    And bob's Res<Score>.home equals 7
    And alice did NOT receive a duplicate InsertResourceEvent for Score on bob's connection

  # --------------------------------------------------------------------------
  # Bevy adapter ergonomics — user sees ZERO entity/component semantics
  # --------------------------------------------------------------------------

  Scenario: server-side standard Bevy ResMut mutation replicates
    Given a Bevy server App with `add_resource_events::<Score>()` registered
    And `commands.replicate_resource(&mut server, Score::new(0, 0))` has been called
    And one connected client
    And the initial replication round trip has elapsed
    When a server system runs `*res_mut.home = 10` via `ResMut<Score>`
    And one replication round trip elapses
    Then the client's `Res<Score>.home` equals 10

  Scenario: client-side resource appears as a standard Bevy Res
    Given a Bevy client App with `add_resource_events::<Main, Score>()` registered
    And the server has inserted Score { home: 5, away: 2 }
    When the client connects and the initial replication round trip elapses
    Then a client system reading `Res<Score>` sees home=5, away=2

  Scenario: user receives InsertResourceEvent / UpdateResourceEvent / RemoveResourceEvent — never SpawnEntityEvent
    Given a Bevy server App and connected Bevy client with Score replicated
    When the server inserts, mutates, then removes Score
    And replication completes
    Then the client received exactly one InsertResourceEvent<Main, Score>
    And the client received at least one UpdateResourceEvent<Main, Score>
    And the client received exactly one RemoveResourceEvent<Main, Score>
    And the client received zero SpawnEntityEvent<Main> attributable to Score
    And the client received zero DespawnEntityEvent<Main> attributable to Score
    And the client received zero InsertComponentEvent<Main, Score>

  Scenario: client requests authority via Commands extension (Bevy ergonomics)
    Given a Bevy server App with delegable PlayerSelection and connected Bevy client "alice"
    When alice's Bevy system runs `commands.request_resource_authority::<PlayerSelection>(&mut client)`
    And one replication round trip elapses
    Then alice's `commands.resource_authority::<PlayerSelection>(&client)` returns Some(Granted)
    And alice can mutate `ResMut<PlayerSelection>` and the change replicates to the server
