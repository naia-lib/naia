# Namako Smoke Test Feature
# This is a minimal vertical slice to validate the end-to-end Namako pipeline:
#   namako manifest → namako lint → namako run → namako verify → namako update-cert

@Feature(namako_smoke_test)
Feature: Namako Smoke Test
  Verifies the core Namako v1 pipeline works end-to-end.

  @Scenario(01)
  Scenario: Server starts and accepts a connecting client
    Given a server is running
    When a client connects
    Then the server has 1 connected client

  @Scenario(02)
  Scenario: Server can disconnect a client
    Given a server is running
    And a client connects
    When the server disconnects the client
    Then the server has 0 connected clients
  @Scenario(03)
  Scenario: Multiple clients can connect to server
    Given a server is running
    When a client connects
    And a client connects
    And a client connects
    Then the server has 3 connected clients

  @Scenario(04)
  Scenario: Server tracks client count accurately
    Given a server is running
    Then the server has 0 connected clients
    When a client connects
    Then the server has 1 connected client
    When a client connects
    Then the server has 2 connected clients

  @Scenario(05)
  Scenario: Connecting client is in connected state
    Given a server is running
    When a client connects
    Then the client is connected

  @Scenario(06)
  Scenario: Disconnecting client is no longer connected
    Given a server is running
    And a client connects
    When the server disconnects the client
    Then the client is not connected

  @Scenario(07)
  Scenario: Server and client observe connect events
    Given a server is running
    When a client connects
    Then the server has observed ConnectEvent
    And the client has observed ConnectEvent
  @Scenario(08)
  Scenario: Server and client observe disconnect events
    Given a server is running
    And a client connects
    When the server disconnects the client
    Then the server has observed DisconnectEvent
    And the client has observed DisconnectEvent

  @Scenario(09)
  Scenario: Event ordering is correct on disconnect
    Given a server is running
    And a client connects
    When the server disconnects the client
    Then the server observed ConnectEvent before DisconnectEvent
    And the client observed ConnectEvent before DisconnectEvent

