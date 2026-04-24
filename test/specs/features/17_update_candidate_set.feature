# ============================================================================
# Update Candidate Set — Canonical Contract
# ============================================================================
# Source: contracts/17_update_candidate_set.spec.md
#
# Summary:
#   Phase 3 optimization: replace the O(entities) per-tick full scan in
#   get_updatable_world with a per-connection dirty set populated at mutation
#   time. Obligations cover behavioral equivalence, idle-is-zero, drain
#   confirmation, and out-of-scope no-candidate.
#
# Behavioral transparency:
#   All scenarios pass on the legacy full-scan path before Phase 3 is
#   implemented. total_dirty_update_count() returns 0 on the legacy path
#   (no dirty set exists). The Phase 3 path populates and drains the set;
#   after each tick the count is also 0. Scenarios are regression guards.
# ============================================================================

@Feature(update_candidate_set)
Feature: Update Candidate Set

  # --------------------------------------------------------------------------
  # Rule: Idle entity produces no dirty update candidates
  # After a tick with no mutations, the dirty-candidate set is empty.
  # --------------------------------------------------------------------------
  @Rule(01)
  Rule: Idle entity produces no dirty update candidates

    @Scenario(01)
    Scenario: update-candidate-01-a — idle tick leaves dirty count at zero
      Given a server is running
      And a client connects
      And a server-owned entity exists with a replicated component
      And the client and entity share a room
      And the entity is in-scope for the client
      When the server performs an idle tick
      Then the total dirty update candidate count is 0

  # --------------------------------------------------------------------------
  # Rule: Mutation candidate drains in tick
  # After a mutation + tick, the dirty set is back at 0 and the update landed.
  # --------------------------------------------------------------------------
  @Rule(02)
  Rule: Mutation candidate drains in tick

    @Scenario(01)
    Scenario: update-candidate-02-a — mutation drains and update reaches client
      Given a server is running
      And a client connects
      And a server-owned entity exists with a replicated component
      And the client and entity share a room
      And the entity is in-scope for the client
      When the server updates the replicated component
      Then the total dirty update candidate count is 0
      And the client observes the component update

  # --------------------------------------------------------------------------
  # Rule: Out-of-scope mutation produces no dirty candidate
  # --------------------------------------------------------------------------
  @Rule(03)
  Rule: Out-of-scope mutation produces no dirty candidate

    @Scenario(01)
    Scenario: update-candidate-03-a — mutation on entity not in shared room produces no candidate
      Given a server is running
      And a client connects
      And a server-owned entity exists with a replicated component
      And the entity is not in the client's room
      When the server updates the replicated component
      Then the total dirty update candidate count is 0
