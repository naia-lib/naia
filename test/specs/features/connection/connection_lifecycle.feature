# Connection Lifecycle Slice 1: Event Ordering & Basic Semantics
# Spec: contracts/01_connection_lifecycle.spec.md
# Obligations: connection-24, connection-25, connection-26, connection-19, connection-21, connection-22
#
# This slice validates observable event ordering on both server and client sides,
# plus rejection semantics. These are foundational behaviors everything else depends on.

Feature: Connection Lifecycle - Event Ordering

  Background:
    Given a server is running with auth required

  # connection-24: AuthEvent -> ConnectEvent -> DisconnectEvent ordering
  # connection-22: Server DisconnectEvent only after ConnectEvent
  Scenario: Server observes events in correct order with auth required
    When a client authenticates and connects
    Then the server observes AuthEvent before ConnectEvent
    And the server has 1 connected client
    When the server disconnects the client
    Then the server observes DisconnectEvent after ConnectEvent
    And the server has 0 connected clients

  # connection-26: Client observes ConnectEvent -> DisconnectEvent
  # connection-21: Client DisconnectEvent only after ConnectEvent
  Scenario: Client observes events in correct order
    When a client authenticates and connects
    Then the client observes ConnectEvent
    And the client is connected
    When the server disconnects the client
    Then the client observes DisconnectEvent after ConnectEvent
    And the client is not connected

  # connection-19: Rejected client emits RejectEvent, not ConnectEvent or DisconnectEvent
  # connection-27: Rejected client observes RejectEvent only
  Scenario: Rejected client observes RejectEvent only
    When a client attempts to connect but is rejected
    Then the client observes RejectEvent
    And the client does not observe ConnectEvent
    And the client does not observe DisconnectEvent
