# ============================================================================
# Entity Ownership, Publication, Delegation, Authority — Grouped Contract Suite
# ============================================================================
# Post-A.4 grouping of multiple source feature files. Each source's content
# is preserved verbatim from the @Rule line onward; per-source separators
# (`# === Source: ... ===`) keep the original boundaries greppable. Free-text
# feature-description blocks from sources are stripped (gherkin only allows
# them under the top-level Feature:). @Rule/@Scenario tag numbers are
# renumbered globally within this file (each source's local 01, 02, ...
# becomes a continuous sequence) so namako sees no duplicate-tag collisions.
# ============================================================================

@Feature(authority)
Feature: Entity Ownership, Publication, Delegation, Authority


  # Auto-applied prelude — every Scenario in this file gets this
  # Given run before its own Givens (idempotent).
  Background:
    Given a server is running

  # ==========================================================================
  # === Source: 08_entity_ownership.feature ===
  # ==========================================================================

  @Rule(01)
  Rule: Entity Ownership

    @Scenario(01)
    Scenario: Server-owned entity accepts writes only from server
      Given a server is running
      And a client connects
      And a server-owned entity exists with a replicated component
      And the client and entity share a room
      And the entity is in-scope for the client
      When the client attempts to write to the server-owned entity
      Then the write is rejected

    @Scenario(02)
    Scenario: Client-owned entity accepts writes from owning client
      Given a server is running
      And a client connects
      And the client spawns a client-owned entity with a replicated component
      When the client updates the replicated component
      Then the server observes the component update

    @Scenario(03)
    Scenario: Entity has exactly one owner at any moment
      Given a server is running
      And a client connects
      And a server-owned entity exists with a replicated component
      And the client and entity share a room
      And the entity is in-scope for the client
      Then the entity owner is the server

    # [entity-ownership-08] — Owner disconnect despawns all client-owned entities
    # When the owning client disconnects, the server MUST despawn all of that
    # client's owned entities, cleaning up all per-connection scoped state.
    @Scenario(04)
    Scenario: entity-ownership-08 — Owner disconnect despawns all client-owned entities
      Given a server is running
      And a client connects
      And the client spawns a client-owned entity with a replicated component
      When the client disconnects
      Then the server no longer has the entity

    # [entity-ownership-02] — Client-owned entity reports EntityOwner::Client on owning client
    # Client MUST report EntityOwner::Client for entities it owns.
    @Scenario(05)
    Scenario: entity-ownership-02 — Client-owned entity reports EntityOwner::Client on owner
      Given a server is running
      And a client connects
      And the client spawns a client-owned entity with a replicated component
      Then the entity owner is the client

  # ==========================================================================
  # === Source: 09_entity_publication.feature ===
  # ==========================================================================

  @Rule(02)
  Rule: Entity Publication

    @Scenario(01)
    Scenario: Unpublished entity is out-of-scope for non-owners
      Given a server is running
      And client A connects
      And client B connects
      And client A spawns a client-owned entity with Private replication config
      And client A and the entity share a room
      And client B and the entity share a room
      Then the entity is out-of-scope for client B

    @Scenario(02)
    Scenario: Published entity may be in-scope for non-owners
      Given a server is running
      And client A connects
      And client B connects
      And client A spawns a client-owned entity with Public replication config
      And client A and the entity share a room
      And client B and the entity share a room
      Then the entity is in-scope for client B

    @Scenario(03)
    Scenario: Owning client always in-scope regardless of publication state
      Given a server is running
      And client A connects
      And client A spawns a client-owned entity with Private replication config
      And client A and the entity share a room
      Then the entity is in-scope for client A

    # [entity-publication-05] — Unpublish forces immediate OutOfScope for non-owners
    # Published → Unpublished: all non-owners MUST become OutOfScope for the entity.
    @Scenario(04)
    Scenario: entity-publication-05 — Unpublish forces entity out-of-scope for non-owners
      Given a server is running
      And client A connects
      And client B connects
      And client A spawns a client-owned entity with Public replication config
      And client A and the entity share a room
      And client B and the entity share a room
      And the entity is in-scope for client B
      When client A unpublishes the entity
      Then the entity becomes out-of-scope for client B

    # [entity-publication-observability-01] — Published entity reports Public replication_config
    # Publication MUST be observable via replication_config on the owning client.
    @Scenario(05)
    Scenario: entity-publication-observability — Public entity reports Public replication_config
      Given a server is running
      And client A connects
      And client A spawns a client-owned entity with Public replication config
      Then client A observes replication config as Public for the entity

    # [entity-publication-observability-02] — Unpublished entity reports Private replication_config
    # Publication MUST be observable via replication_config on the owning client.
    @Scenario(06)
    Scenario: entity-publication-observability — Private entity reports Private replication_config
      Given a server is running
      And client A connects
      And client A spawns a client-owned entity with Private replication config
      Then client A observes replication config as Private for the entity

  # ==========================================================================
  # === Source: 10_entity_delegation.feature ===
  # ==========================================================================

  @Rule(03)
  Rule: Entity Delegation

    # [entity-delegation-06] — First request wins
    # The first in-scope client to request authority MUST be granted it.
    # A second client requesting while authority is held MUST observe Denied.
    @Scenario(01)
    Scenario: entity-delegation-06 — First request wins; other in-scope clients observe Denied
      Given a server is running
      And client A connects
      And client B connects
      And the server spawns a delegated entity in-scope for both clients
      When client A requests authority for the delegated entity
      Then client A is granted authority for the delegated entity
      And client B is denied authority for the delegated entity

    # [entity-delegation-07/11] — Release transitions Denied back to Available
    # After the authority holder releases, all Denied clients MUST become Available.
    @Scenario(02)
    Scenario: entity-delegation-11 — Release transitions Denied clients back to Available
      Given a server is running
      And client A connects
      And client B connects
      And the server spawns a delegated entity in-scope for both clients
      When client A requests authority for the delegated entity
      And client B requests authority for the delegated entity
      Then client A is granted authority for the delegated entity
      And client B is denied authority for the delegated entity
      When client A releases authority for the delegated entity
      Then client A is available for the delegated entity
      And client B is available for the delegated entity

    # [entity-delegation-13] — Losing scope ends client authority and unblocks others
    # When the authority-holding client loses scope, the server MUST release authority
    # and other in-scope clients MUST transition to Available.
    @Scenario(03)
    Scenario: entity-delegation-13 — Losing scope releases authority and unblocks waiting clients
      Given a server is running
      And client A connects
      And client B connects
      And the server spawns a delegated entity in-scope for both clients
      When client A requests authority for the delegated entity
      And client B requests authority for the delegated entity
      Then client A is granted authority for the delegated entity
      And client B is denied authority for the delegated entity
      When the server removes the delegated entity from client A's scope
      Then the delegated entity is no longer in client A's world
      And client B is available for the delegated entity

    # [entity-delegation-14] — Disconnect releases authority and unblocks others
    # When the authority-holding client disconnects, the server MUST release authority
    # and other in-scope clients MUST transition to Available.
    @Scenario(04)
    Scenario: entity-delegation-14 — Disconnect releases authority and unblocks waiting clients
      Given a server is running
      And client A connects
      And client B connects
      And the server spawns a delegated entity in-scope for both clients
      When client A requests authority for the delegated entity
      And client B requests authority for the delegated entity
      Then client A is granted authority for the delegated entity
      And client B is denied authority for the delegated entity
      When client A disconnects from the server
      Then client B is available for the delegated entity

    # [entity-delegation-17] — Delegation observable via replication_config and authority status
    # Clients MUST be able to observe that an entity is Delegated and query the current
    # authority status as Available when no holder exists.
    @Scenario(05)
    Scenario: entity-delegation-17 — Delegated entity has observable config and Available status
      Given a server is running
      And client A connects
      And the server spawns a delegated entity in-scope for client A
      Then client A observes Delegated replication config for the entity
      And client A observes Available authority status for the entity

  # ==========================================================================
  # === Source: 11_entity_authority.feature ===
  # ==========================================================================

  @Rule(04)
  Rule: Entity Authority

    # [entity-authority-01] — Authority is None for non-delegated entities
    # If replication_config(E) != Delegated, authority(E) MUST be None on clients.
    @Scenario(01)
    Scenario: entity-authority-01 — Non-delegated entity has no authority status
      Given a server is running
      And client A connects
      And the server spawns a non-delegated entity in-scope for client A
      Then client A observes no authority status for the entity

    # [entity-authority-09] — Server may hold authority; all clients observe Denied
    # While the server holds authority, all in-scope clients MUST observe Denied.
    @Scenario(02)
    Scenario: entity-authority-09 — Server holding authority puts all clients in Denied
      Given a server is running
      And client A connects
      And client B connects
      And the server spawns a delegated entity in-scope for both clients
      When the server takes authority for the delegated entity
      Then client A is denied authority for the delegated entity
      And client B is denied authority for the delegated entity

    # [entity-authority-10] — Server override/reset transitions all clients to Available
    # When the server resets authority, all clients MUST transition to Available.
    @Scenario(03)
    Scenario: entity-authority-10 — Server reset transitions all clients to Available
      Given a server is running
      And client A connects
      And client B connects
      And the server spawns a delegated entity in-scope for both clients
      And the server takes authority for the delegated entity
      And client A is denied authority for the delegated entity
      When the server releases authority for the delegated entity
      Then client A is available for the delegated entity
      And client B is available for the delegated entity

    # [entity-authority-06] — release_authority() transitions Granted → Releasing → Available
    # A client that holds authority MUST eventually become Available after releasing.
    @Scenario(04)
    Scenario: entity-authority-06 — Client release transitions Granted to Available
      Given a server is running
      And client A connects
      And the server spawns a delegated entity in-scope for client A
      When client A requests authority for the delegated entity
      Then client A is granted authority for the delegated entity
      When client A releases authority for the delegated entity
      Then client A is available for the delegated entity

    # [entity-authority-16] — Authority grant is observable via event API
    # When the server grants authority, the client MUST receive an authority granted event.
    @Scenario(05)
    Scenario: entity-authority-16 — Client receives authority granted event
      Given a server is running
      And client A connects
      And the server spawns a delegated entity in-scope for client A
      When client A requests authority for the delegated entity
      Then client A receives an authority granted event for the entity

    # [entity-authority-16] — Authority reset is observable via event API
    # When the server releases authority, all in-scope clients MUST receive an
    # authority reset event, signaling the entity returned to Available.
    @Scenario(06)
    Scenario: entity-authority-16 — Client receives authority reset event when server releases
      Given a server is running
      And client A connects
      And the server spawns a delegated entity in-scope for client A
      And the server takes authority for the delegated entity
      And client A is denied authority for the delegated entity
      When the server releases authority for the delegated entity
      Then client A receives an authority reset event for the entity

    # [entity-authority-16] — Authority denied event observable when request is denied
    # When client B requests authority while client A's grant is in flight, client B MUST
    # receive a denied event (Requested → Denied transition emits EntityAuthDeniedEvent).
    # Both clients request back-to-back (no intermediate wait) so B is still in Requested
    # state when the server denies it, triggering the Requested→Denied event.
    @Scenario(07)
    Scenario: entity-authority-16 — Client receives authority denied event when request is denied
      Given a server is running
      And client A connects
      And client B connects
      And the server spawns a delegated entity in-scope for both clients
      When client A requests authority for the delegated entity
      And client B requests authority for the delegated entity
      Then client B receives an authority denied event for the entity

    # [entity-authority-07] — request_authority on non-delegated entity MUST return error
    # Calling request_authority() on a non-delegated entity MUST return an error,
    # not panic. No state mutation should occur.
    @Scenario(08)
    Scenario: entity-authority-07 — Request authority on non-delegated entity returns error
      Given a server is running
      And client A connects
      And the server spawns a non-delegated entity in-scope for client A
      When client A requests authority for the non-delegated entity
      Then the authority request fails with an error

