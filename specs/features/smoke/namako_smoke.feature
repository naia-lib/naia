# Namako Smoke Test Feature
# This is a minimal vertical slice to validate the end-to-end Namako pipeline:
#   namako manifest → namako lint → namako run → namako verify → namako update-cert

Feature: Namako Smoke Test
  Verifies the core Namako v1 pipeline works end-to-end.

  Scenario: Server starts and accepts a connecting client
    Given a server is running
    When a client connects
    Then the server has 1 connected client

  Scenario: Server can disconnect a client
    Given a server is running
    And a client connects
    When the server disconnects the client
    Then the server has 0 connected clients
