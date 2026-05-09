# Bevy Adapter — Replication Contract Suite
#
# Scope: tests what the Bevy adapters add ON TOP of core Naia:
#   - ECS change-detection bridge (Bevy world mutation → replication)
#   - Bevy event routing: SpawnEntityEvent, DespawnEntityEvent,
#     EntityAuthGrantedEvent, EntityAuthDeniedEvent
#   - CommandsExt / ServerCommandsExt / ClientCommandsExt APIs
#   - Client Bevy Resource<R> mirror for replicated resources

@Feature(bevy_replication)
Feature: Bevy Adapter Replication

  Background:
    Given a server is running

  # ==========================================================================
  # Rule 01 — Bevy ECS bridge and event routing (entity lifecycle)
  # ==========================================================================

  @Rule(01)
  Rule: Bevy ECS bridge and Bevy event routing

    # CommandsExt::enable_replication + room scoping → entity appears in client Bevy world.
    @Scenario(01)
    Scenario: [bevy-replication-01] enable_replication scopes entity into client Bevy world
      Given a server is running
      And a client connects
      And a server entity is spawned in-scope for the client with Position
      Then the entity spawns on the client

    # SpawnEntityEvent<ClientSingleton> fires when a replicated entity enters client scope.
    @Scenario(02)
    Scenario: [bevy-replication-02] SpawnEntityEvent fires on entity scope entry
      Given a server is running
      And a client connects
      And a server entity is spawned in-scope for the client with Position
      Then the client has observed SpawnEntityEvent

    # CommandsExt::disable_replication removes entity from client Bevy world.
    @Scenario(03)
    Scenario: [bevy-replication-03] disable_replication removes entity from client Bevy world
      Given a server is running
      And a client connects
      And a server entity is spawned in-scope for the client with Position
      When the server disables replication for the entity
      Then the entity is absent from the client world

    # DespawnEntityEvent<ClientSingleton> fires when entity leaves client scope.
    @Scenario(04)
    Scenario: [bevy-replication-04] DespawnEntityEvent fires when entity leaves scope
      Given a server is running
      And a client connects
      And a server entity is spawned in-scope for the client with Position
      When the server disables replication for the entity
      Then the client has observed DespawnEntityEvent

    # ECS change-detection bridge: server mutates a Bevy component via world_mut(),
    # Naia's change-detection picks it up and replicates to the client.
    @Scenario(05)
    Scenario: [bevy-replication-05] Bevy world mutation replicates via ECS change detection
      Given a server is running
      And a client connects
      And a server entity is spawned in-scope for the client with Position at the origin
      When the server mutates Position to 42 and 42
      Then the client observes Position 42 and 42

  # ==========================================================================
  # Rule 02 — Authority via Commands APIs
  # ==========================================================================

  @Rule(02)
  Rule: Authority via CommandsExt APIs

    # ServerCommandsExt::give_authority grants Delegated authority to a client.
    @Scenario(01)
    Scenario: [bevy-authority-01] give_authority grants EntityAuthStatus::Granted
      Given a server is running
      And a client connects
      And a server entity is spawned in-scope for the client with Position
      And the entity is configured as Delegated
      When the server grants authority to the client
      Then the client has authority status Granted

    # EntityAuthGrantedEvent<ClientSingleton> fires when the client receives authority.
    @Scenario(02)
    Scenario: [bevy-authority-02] EntityAuthGrantedEvent fires on authority grant
      Given a server is running
      And a client connects
      And a server entity is spawned in-scope for the client with Position
      And the entity is configured as Delegated
      When the server grants authority to the client
      Then the client has observed EntityAuthGrantedEvent

    # ClientCommandsExt::request_authority → server grants, client sees Granted.
    @Scenario(03)
    Scenario: [bevy-authority-03] request_authority results in EntityAuthStatus::Granted
      Given a server is running
      And a client connects
      And a server entity is spawned in-scope for the client with Position
      And the entity is configured as Delegated
      When the client requests authority for the entity
      Then the client has authority status Granted

    # Contended request: first client wins, second sees EntityAuthDeniedEvent.
    @Scenario(04)
    Scenario: [bevy-authority-04] Second requester sees EntityAuthDeniedEvent
      Given a server is running
      And a client connects
      And a second client connects
      And a server entity is spawned in-scope for both clients with Position
      And the entity is configured as Delegated
      When the first client requests authority for the entity
      And the second client requests authority for the entity
      Then the second client has observed EntityAuthDeniedEvent

  # ==========================================================================
  # Rule 03 — Bevy Resource<R> mirror for replicated resources
  # ==========================================================================

  @Rule(03)
  Rule: Client Bevy Resource mirror for replicated resources

    # ServerCommandsExt::replicate_resource → client world gets a Bevy Resource<TestScore>.
    @Scenario(01)
    Scenario: [bevy-resource-01] replicate_resource inserts Bevy Resource on client with correct value
      Given a server is running
      And a client connects
      When the server inserts TestScore home 3 away 1 as a replicated resource
      Then the client has TestScore as a Bevy resource
      And the client TestScore.home equals 3

    # ResMut<TestScore> mutation on the server propagates to the client Bevy Resource.
    @Scenario(02)
    Scenario: [bevy-resource-02] ResMut mutation propagates to client Bevy Resource
      Given a server is running
      And a client connects
      When the server inserts TestScore home 0 away 0 as a replicated resource
      And the server mutates TestScore to home 7 away 2
      Then the client TestScore.home equals 7
      And the client TestScore.away equals 2

    # ClientCommandsExt::request_resource_authority → server sees its own authority as Denied
    # (client holds authority; server's own resource_authority_status = Denied = client is holder).
    @Scenario(03)
    Scenario: [bevy-resource-03] request_resource_authority is observed as Denied by server
      Given a server is running
      And a client connects
      And the server inserts TestPlayerSelection as a delegable resource
      When the client requests authority for TestPlayerSelection
      Then the server observes Denied authority for TestPlayerSelection
