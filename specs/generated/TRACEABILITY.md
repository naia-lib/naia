# Contract Traceability Matrix

**Generated:** 2026-01-12 02:13 UTC

This matrix shows the bidirectional mapping between contracts and tests.

---

## Contracts → Tests

| Contract | Test Function | Test File | Status |
|----------|---------------|-----------|--------|
| `client-events-00` | `accessing_an_entity_after_despawn_is_safely_rejected` | events_world_integration.rs | COVERED |
| `client-events-01` | `(manual check)` | events_world_integration.rs | COVERED |
| `client-events-02` | `(manual check)` | events_world_integration.rs | COVERED |
| `client-events-03` | `client_never_sees_update_or_remove_events_for_entities_that_were_never_in_scope` | events_world_integration.rs | COVERED |
| `client-events-04` | `client_never_sees_update_or_remove_events_for_entities_that_were_never_in_scope` | events_world_integration.rs | COVERED |
| `client-events-05` | `client_never_sees_update_or_insert_events_before_seeing_a_spawn_event` | events_world_integration.rs | COVERED |
| `client-events-06` | `client_never_sees_update_or_insert_events_before_seeing_a_spawn_event` | events_world_integration.rs | COVERED |
| `client-events-07` | `client_never_sees_update_or_insert_events_before_seeing_a_spawn_event` | events_world_integration.rs | COVERED |
| `client-events-08` | `(manual check)` | events_world_integration.rs | COVERED |
| `client-events-09` | `(manual check)` | events_world_integration.rs | COVERED |
| `client-events-10` | `client_message_events_are_grouped_and_typed_correctly_per_channel` | events_world_integration.rs | COVERED |
| `client-events-11` | `(manual check)` | events_world_integration.rs | COVERED |
| `client-events-12` | `(manual check)` | events_world_integration.rs | COVERED |
| `commands-01` | `command_history_preserves_and_replays_commands_after_correction` | time_ticks_transport.rs | COVERED |
| `commands-02` | `command_history_preserves_and_replays_commands_after_correction` | time_ticks_transport.rs | COVERED |
| `commands-03` | `command_history_discards_old_commands_beyond_its_window` | time_ticks_transport.rs | COVERED |
| `commands-04` | `command_history_discards_old_commands_beyond_its_window` | time_ticks_transport.rs | COVERED |
| `commands-05` | `extreme_jitter_and_reordering_preserve_channel_contracts` | time_ticks_transport.rs | COVERED |
| `connection-01` | `basic_connect_disconnect_lifecycle` | connection_auth_identity.rs | COVERED |
| `connection-02` | `basic_connect_disconnect_lifecycle` | connection_auth_identity.rs | COVERED |
| `connection-03` | `connect_event_ordering_stable` | connection_auth_identity.rs | COVERED |
| `connection-04` | `connect_event_ordering_stable` | connection_auth_identity.rs | COVERED |
| `connection-05` | `disconnect_idempotent_and_clean` | connection_auth_identity.rs | COVERED |
| `connection-06` | `disconnect_idempotent_and_clean` | connection_auth_identity.rs | COVERED |
| `connection-07` | `(manual check)` | connection_auth_identity.rs | COVERED |
| `connection-08` | `(manual check)` | connection_auth_identity.rs | COVERED |
| `connection-09` | `(manual check)` | connection_auth_identity.rs | COVERED |
| `connection-10` | `basic_connect_disconnect_lifecycle` | connection_auth_identity.rs | COVERED |
| `connection-11` | `(manual check)` | connection_auth_identity.rs | COVERED |
| `connection-12` | `(manual check)` | connection_auth_identity.rs | COVERED |
| `connection-13` | `(manual check)` | connection_auth_identity.rs | COVERED |
| `connection-14` | `(manual check)` | connection_auth_identity.rs | COVERED |
| `connection-15` | `(manual check)` | connection_auth_identity.rs | COVERED |
| `connection-16` | `(manual check)` | connection_auth_identity.rs | COVERED |
| `connection-17` | `(manual check)` | connection_auth_identity.rs | COVERED |
| `connection-18` | `(manual check)` | connection_auth_identity.rs | COVERED |
| `connection-19` | `(manual check)` | connection_auth_identity.rs | COVERED |
| `connection-20` | `(manual check)` | connection_auth_identity.rs | COVERED |
| `connection-21` | `(manual check)` | connection_auth_identity.rs | COVERED |
| `connection-22` | `(manual check)` | connection_auth_identity.rs | COVERED |
| `connection-23` | `malformed_identity_token_rejected` | connection_auth_identity.rs | COVERED |
| `connection-24` | `malformed_identity_token_rejected` | connection_auth_identity.rs | COVERED |
| `connection-25` | `(manual check)` | connection_auth_identity.rs | COVERED |
| `connection-26` | `(manual check)` | connection_auth_identity.rs | COVERED |
| `connection-27` | `(manual check)` | connection_auth_identity.rs | COVERED |
| `entity-authority-01` | `client_request_authority_on_non_delegated_returns_err_not_delegated` | entity_delegation_toggle.rs | COVERED |
| `entity-authority-02` | `non_holder_cannot_mutate_delegated_entity` | entity_authority_client_ops.rs | COVERED |
| `entity-authority-03` | `server_held_authority_is_indistinguishable_from_client_is_denied` | entity_authority_server_ops.rs | COVERED |
| `entity-authority-04` | `request_authority_available_grants_to_requester_and_denies_everyone_else` | entity_authority_client_ops.rs | COVERED |
| `entity-authority-05` | `request_authority_available_grants_to_requester_and_denies_everyone_else` | entity_authority_client_ops.rs | COVERED |
| `entity-authority-06` | `holder_release_authority_transitions_everyone_to_available` | entity_authority_client_ops.rs | COVERED |
| `entity-authority-07` | `release_authority_when_not_holder_fails_err_not_holder` | entity_authority_client_ops.rs | COVERED |
| `entity-authority-08` | `request_authority_available_grants_to_requester_and_denies_everyone_else` | entity_authority_client_ops.rs | COVERED |
| `entity-authority-09` | `give_authority_assigns_to_client_and_denies_everyone_else` | entity_authority_server_ops.rs | COVERED |
| `entity-authority-10` | `give_authority_assigns_to_client_and_denies_everyone_else` | entity_authority_server_ops.rs | COVERED |
| `entity-authority-11` | `(manual check)` | entity_authority_server_ops.rs | COVERED |
| `entity-authority-12` | `holder_release_authority_transitions_everyone_to_available` | entity_authority_client_ops.rs | COVERED |
| `entity-authority-13` | `disable_delegation_clears_authority_semantics` | entity_delegation_toggle.rs | COVERED |
| `entity-authority-14` | `server_give_authority_requires_scope` | entity_authority_server_ops.rs | COVERED |
| `entity-authority-15` | `(manual check)` | entity_authority_server_ops.rs | COVERED |
| `entity-authority-16` | `give_authority_assigns_to_client_and_denies_everyone_else` | entity_authority_server_ops.rs | COVERED |
| `entity-delegation-01` | `server_owned_undelegated_accepts_only_server_writes` | entity_delegation_toggle.rs | COVERED |
| `entity-delegation-02` | `(manual check)` | entity_delegation_toggle.rs | COVERED |
| `entity-delegation-03` | `enable_delegation_makes_entity_available_for_all_in_scope_clients` | entity_delegation_toggle.rs | COVERED |
| `entity-delegation-04` | `delegating_client_owned_published_migrates_identity_without_despawn_spawn` | entity_migration_and_events.rs | COVERED |
| `entity-delegation-05` | `delegating_client_owned_published_migrates_identity_without_despawn_spawn` | entity_migration_and_events.rs | COVERED |
| `entity-delegation-06` | `migration_assigns_initial_authority_to_owner_if_owner_in_scope` | entity_migration_and_events.rs | COVERED |
| `entity-delegation-07` | `migration_assigns_initial_authority_to_owner_if_owner_in_scope` | entity_migration_and_events.rs | COVERED |
| `entity-delegation-08` | `migration_yields_no_holder_if_owner_out_of_scope` | entity_migration_and_events.rs | COVERED |
| `entity-delegation-09` | `migration_yields_no_holder_if_owner_out_of_scope` | entity_migration_and_events.rs | COVERED |
| `entity-delegation-10` | `no_auth_events_for_non_delegated_entities_ever` | entity_migration_and_events.rs | COVERED |
| `entity-delegation-11` | `no_auth_events_for_non_delegated_entities_ever` | entity_migration_and_events.rs | COVERED |
| `entity-delegation-12` | `after_migration_writes_follow_delegated_rules` | entity_migration_and_events.rs | COVERED |
| `entity-delegation-13` | `disable_delegation_while_client_holds_authority` | entity_delegation_toggle.rs | COVERED |
| `entity-delegation-14` | `auth_granted_emitted_exactly_once_on_available_to_granted` | entity_migration_and_events.rs | COVERED |
| `entity-delegation-15` | `auth_granted_emitted_exactly_once_on_available_to_granted` | entity_migration_and_events.rs | COVERED |
| `entity-delegation-16` | `disable_delegation_while_client_holds_authority` | entity_delegation_toggle.rs | COVERED |
| `entity-delegation-17` | `server_owned_undelegated_has_no_authority_status_and_no_auth_events` | entity_delegation_toggle.rs | COVERED |
| `entity-publication-01` | `client_owned_published_rejects_non_owner_mutations` | entity_client_owned.rs | COVERED |
| `entity-publication-02` | `client_owned_unpublished_is_visible_only_to_owner` | entity_client_owned.rs | COVERED |
| `entity-publication-03` | `client_owned_published_may_be_scoped_to_non_owners` | entity_client_owned.rs | COVERED |
| `entity-publication-04` | `(manual check)` | entity_client_owned.rs | COVERED |
| `entity-publication-05` | `publish_toggle_published_to_unpublished_forcibly_despawns_for_non_owners` | entity_client_owned.rs | COVERED |
| `entity-publication-06` | `client_owned_published_may_be_scoped_to_non_owners` | entity_client_owned.rs | COVERED |
| `entity-publication-07` | `client_owned_unpublished_is_visible_only_to_owner` | entity_client_owned.rs | COVERED |
| `entity-publication-08` | `publish_toggle_published_to_unpublished_forcibly_despawns_for_non_owners` | entity_client_owned.rs | COVERED |
| `entity-publication-09` | `client_owned_published_rejects_non_owner_mutations` | entity_client_owned.rs | COVERED |
| `entity-publication-10` | `(manual check)` | entity_client_owned.rs | COVERED |
| `entity-publication-11` | `(manual check)` | entity_client_owned.rs | COVERED |
| `entity-replication-01` | `stable_logical_identity_across_clients_in_steady_state` | entities_lifetime_identity.rs | COVERED |
| `entity-replication-02` | `despawn_semantics` | entities_lifetime_identity.rs | COVERED |
| `entity-replication-03` | `server_spawned_public_entity_replicates_to_all_scoped_clients` | entities_lifetime_identity.rs | COVERED |
| `entity-replication-04` | `no_updates_before_spawn_and_none_after_despawn` | entities_lifetime_identity.rs | COVERED |
| `entity-replication-05` | `no_updates_before_spawn_and_none_after_despawn` | entities_lifetime_identity.rs | COVERED |
| `entity-replication-06` | `component_insertion_after_initial_spawn` | entities_lifetime_identity.rs | COVERED |
| `entity-replication-07` | `(manual check)` | rooms_scope_snapshot.rs | COVERED |
| `entity-replication-08` | `component_updates_propagate_consistently_across_clients` | entities_lifetime_identity.rs | COVERED |
| `entity-replication-09` | `stable_logical_identity_across_clients_in_steady_state` | entities_lifetime_identity.rs | COVERED |
| `entity-replication-10` | `(manual check)` | rooms_scope_snapshot.rs | COVERED |
| `entity-replication-11` | `entering_scope_mid_lifetime_yields_consistent_snapshot` | rooms_scope_snapshot.rs | COVERED |
| `entity-replication-12` | `component_updates_propagate_consistently_across_clients` | entities_lifetime_identity.rs | COVERED |
| `entity-scopes-01` | `server_spawned_public_entity_replicates_to_all_scoped_clients` | entities_lifetime_identity.rs | COVERED |
| `entity-scopes-02` | `server_spawned_public_entity_replicates_to_all_scoped_clients` | entities_lifetime_identity.rs | COVERED |
| `entity-scopes-03` | `(manual check)` | integration_transport_parity.rs | COVERED |
| `entity-scopes-04` | `(manual check)` | integration_transport_parity.rs | COVERED |
| `entity-scopes-05` | `(manual check)` | entities_lifetime_identity.rs | COVERED |
| `entity-scopes-06` | `(manual check)` | entities_lifetime_identity.rs | COVERED |
| `entity-scopes-07` | `despawn_semantics` | entities_lifetime_identity.rs | COVERED |
| `entity-scopes-08` | `authority_releases_when_holder_disconnects` | entity_scope_coupling.rs | COVERED |
| `entity-scopes-09` | `despawn_semantics` | entities_lifetime_identity.rs | COVERED |
| `entity-scopes-10` | `server_spawned_public_entity_replicates_to_all_scoped_clients` | entities_lifetime_identity.rs | COVERED |
| `entity-scopes-11` | `re_entering_scope_yields_correct_current_auth_status` | entity_scope_coupling.rs | COVERED |
| `entity-scopes-12` | `scope_leave_and_re_enter_semantics` | entities_lifetime_identity.rs | COVERED |
| `entity-scopes-13` | `re_entering_scope_yields_correct_current_auth_status` | entity_scope_coupling.rs | COVERED |
| `entity-scopes-14` | `entering_scope_mid_lifetime_yields_consistent_snapshot` | rooms_scope_snapshot.rs | COVERED |
| `entity-scopes-15` | `(manual check)` | rooms_scope_snapshot.rs | COVERED |
| `messaging-01` | `(manual check)` | events_world_integration.rs | COVERED |
| `messaging-02` | `(manual check)` | events_world_integration.rs | COVERED |
| `messaging-03` | `misusing_channel_types_yields_defined_failure` | events_world_integration.rs | COVERED |
| `messaging-04` | `misusing_channel_types_yields_defined_failure` | events_world_integration.rs | COVERED |
| `messaging-05` | `client_message_events_are_grouped_and_typed_correctly_per_channel` | events_world_integration.rs | COVERED |
| `messaging-06` | `client_message_events_are_grouped_and_typed_correctly_per_channel` | events_world_integration.rs | COVERED |
| `messaging-07` | `sequenced_unreliable_channel_discards_late_outdated_updates` | messaging_channels.rs | COVERED |
| `messaging-08` | `reliable_server_to_clients_broadcast_respects_rooms` | messaging_channels.rs | COVERED |
| `messaging-09` | `per_channel_ordering` | messaging_channels.rs | COVERED |
| `messaging-10` | `sequenced_reliable_channel_only_exposes_the_latest_message_in_a_stream` | messaging_channels.rs | COVERED |
| `messaging-11` | `(manual check)` | protocol_schema_versioning.rs | COVERED |
| `messaging-12` | `tick_buffered_channel_groups_messages_by_tick` | messaging_channels.rs | COVERED |
| `messaging-13` | `channel_separation_for_different_message_types` | protocol_schema_versioning.rs | COVERED |
| `messaging-14` | `tick_buffered_channel_discards_messages_for_ticks_that_are_too_old` | messaging_channels.rs | COVERED |
| `messaging-15` | `(manual check)` | events_world_integration.rs | COVERED |
| `messaging-16` | `(manual check)` | events_world_integration.rs | COVERED |
| `messaging-17` | `(manual check)` | events_world_integration.rs | COVERED |
| `messaging-18` | `(manual check)` | events_world_integration.rs | COVERED |
| `messaging-19` | `sending_messages_or_requests_on_a_disconnected_or_rejected_connection_is_safe` | events_world_integration.rs | COVERED |
| `messaging-20` | `sending_messages_or_requests_on_a_disconnected_or_rejected_connection_is_safe` | events_world_integration.rs | COVERED |
| `observability-01` | - | - | **UNCOVERED** |
| `observability-02` | - | - | **UNCOVERED** |
| `observability-03` | - | - | **UNCOVERED** |
| `observability-04` | - | - | **UNCOVERED** |
| `observability-05` | - | - | **UNCOVERED** |
| `observability-06` | - | - | **UNCOVERED** |
| `observability-07` | - | - | **UNCOVERED** |
| `observability-08` | - | - | **UNCOVERED** |
| `observability-09` | - | - | **UNCOVERED** |
| `server-events-00` | `(manual check)` | events_world_integration.rs | COVERED |
| `server-events-01` | `(manual check)` | events_world_integration.rs | COVERED |
| `server-events-02` | `component_update_events_reflect_correct_multiplicity_per_user` | events_world_integration.rs | COVERED |
| `server-events-03` | `component_update_events_reflect_correct_multiplicity_per_user` | events_world_integration.rs | COVERED |
| `server-events-04` | `(manual check)` | events_world_integration.rs | COVERED |
| `server-events-05` | `(manual check)` | events_world_integration.rs | COVERED |
| `server-events-06` | `(manual check)` | events_world_integration.rs | COVERED |
| `server-events-07` | `accessing_non_existent_entity_yields_safe_failure_not_panic` | events_world_integration.rs | COVERED |
| `server-events-08` | `accessing_non_existent_entity_yields_safe_failure_not_panic` | events_world_integration.rs | COVERED |
| `server-events-09` | `accessing_an_entity_after_despawn_is_safely_rejected` | events_world_integration.rs | COVERED |
| `server-events-10` | `accessing_an_entity_after_despawn_is_safely_rejected` | events_world_integration.rs | COVERED |
| `server-events-11` | `mutating_out_of_scope_entity_for_a_given_user_is_ignored_or_errors_predictably` | events_world_integration.rs | COVERED |
| `server-events-12` | `mutating_out_of_scope_entity_for_a_given_user_is_ignored_or_errors_predictably` | events_world_integration.rs | COVERED |
| `server-events-13` | `sending_messages_or_requests_on_a_disconnected_or_rejected_connection_is_safe` | events_world_integration.rs | COVERED |
| `time-01` | `deterministic_replay_of_a_scenario` | time_ticks_transport.rs | COVERED |
| `time-02` | `server_and_client_tick_indices_advance_monotonically` | time_ticks_transport.rs | COVERED |
| `time-03` | `server_and_client_tick_indices_advance_monotonically` | time_ticks_transport.rs | COVERED |
| `time-04` | `pausing_and_resuming_time_does_not_create_extra_ticks` | time_ticks_transport.rs | COVERED |
| `time-05` | `pausing_and_resuming_time_does_not_create_extra_ticks` | time_ticks_transport.rs | COVERED |
| `time-06` | `command_history_preserves_and_replays_commands_after_correction` | time_ticks_transport.rs | COVERED |
| `time-07` | `tick_index_wraparound_does_not_break_progression_or_ordering` | time_ticks_transport.rs | COVERED |
| `time-08` | `tick_index_wraparound_does_not_break_progression_or_ordering` | time_ticks_transport.rs | COVERED |
| `time-09` | `sequence_number_wraparound_for_channels_preserves_ordering_semantics` | time_ticks_transport.rs | COVERED |
| `time-10` | `long_running_scenario_maintains_stable_memory_and_state` | time_ticks_transport.rs | COVERED |
| `time-11` | `long_running_scenario_maintains_stable_memory_and_state` | time_ticks_transport.rs | COVERED |
| `time-12` | `very_aggressive_heartbeat_timeout_still_leads_to_clean_disconnect` | time_ticks_transport.rs | COVERED |
| `transport-01` | `(manual check)` | integration_transport_parity.rs | COVERED |
| `transport-02` | `(manual check)` | integration_transport_parity.rs | COVERED |
| `transport-03` | `(manual check)` | integration_transport_parity.rs | COVERED |
| `transport-04` | `(manual check)` | integration_transport_parity.rs | COVERED |
| `transport-05` | `schema_incompatibility_produces_immediate_clear_failure` | protocol_schema_versioning.rs | COVERED |
| `world-integration-01` | `server_world_integration_receives_every_insert_update_remove_exactly_once` | events_world_integration.rs | COVERED |
| `world-integration-02` | `server_world_integration_receives_every_insert_update_remove_exactly_once` | events_world_integration.rs | COVERED |
| `world-integration-03` | `server_world_integration_receives_every_insert_update_remove_exactly_once` | events_world_integration.rs | COVERED |
| `world-integration-04` | `client_world_integration_stays_in_lockstep_with_naias_view` | events_world_integration.rs | COVERED |
| `world-integration-05` | `client_world_integration_stays_in_lockstep_with_naias_view` | events_world_integration.rs | COVERED |
| `world-integration-06` | `(manual check)` | events_world_integration.rs | COVERED |
| `world-integration-07` | `(manual check)` | events_world_integration.rs | COVERED |
| `world-integration-08` | `(manual check)` | events_world_integration.rs | COVERED |
| `world-integration-09` | `(manual check)` | integration_transport_parity.rs | COVERED |

---

## Tests → Contracts

| Test File | Test Function | Contracts Verified |
|-----------|---------------|--------------------|
| connection_auth_identity.rs | (check manually) | connection-01,connection-02,connection-03,connection-04,connection-05,connection-06,connection-07,connection-08,connection-09,connection-10,connection-11,connection-12,connection-13,connection-14,connection-15,connection-16,connection-17,connection-18,connection-19,connection-20,connection-21,connection-22,connection-23,connection-24,connection-25,connection-26,connection-27, |
| entities_lifetime_identity.rs | (check manually) | entity-replication-01,entity-replication-02,entity-replication-03,entity-replication-04,entity-replication-05,entity-replication-06,entity-replication-08,entity-replication-09,entity-replication-12,entity-scopes-01,entity-scopes-02,entity-scopes-05,entity-scopes-06,entity-scopes-07,entity-scopes-09,entity-scopes-10,entity-scopes-12, |
| entity_authority_client_ops.rs | denied_client_request_authority_fails_err_not_available,holder_can_mutate_delegated_entity,holder_release_authority_transitions_everyone_to_available,non_holder_cannot_mutate_delegated_entity,request_authority_available_grants_to_requester_and_denies_everyone_else, | entity-authority-02,entity-authority-04,entity-authority-05,entity-authority-06,entity-authority-07,entity-authority-08,entity-authority-12, |
| entity_authority_server_ops.rs | (check manually) | entity-authority-03,entity-authority-09,entity-authority-10,entity-authority-11,entity-authority-12,entity-authority-14,entity-authority-15,entity-authority-16, |
| entity_client_owned.rs | (check manually) | entity-publication-01,entity-publication-02,entity-publication-03,entity-publication-04,entity-publication-05,entity-publication-06,entity-publication-07,entity-publication-08,entity-publication-09,entity-publication-10,entity-publication-11, |
| entity_delegation_toggle.rs | (check manually) | entity-authority-01,entity-authority-13,entity-delegation-01,entity-delegation-02,entity-delegation-03,entity-delegation-13,entity-delegation-16,entity-delegation-17, |
| entity_migration_and_events.rs | after_migration_writes_follow_delegated_rules,auth_denied_emitted_exactly_once_per_transition_into_denied,auth_granted_emitted_exactly_once_on_available_to_granted,auth_lost_emitted_exactly_once_per_transition_out_of_granted,cannot_delegate_client_owned_unpublished_err_not_published, | entity-delegation-01,entity-delegation-02,entity-delegation-03,entity-delegation-04,entity-delegation-05,entity-delegation-06,entity-delegation-07,entity-delegation-08,entity-delegation-09,entity-delegation-10,entity-delegation-11,entity-delegation-12,entity-delegation-13,entity-delegation-14,entity-delegation-15,entity-delegation-16,entity-delegation-17, |
| entity_scope_coupling.rs | authority_releases_when_holder_disconnects,authority_releases_when_holder_goes_out_of_scope, | entity-delegation-10,entity-delegation-11,entity-scopes-06,entity-scopes-07,entity-scopes-08,entity-scopes-09,entity-scopes-11,entity-scopes-12,entity-scopes-13, |
| events_world_integration.rs | (check manually) | client-events-00,client-events-01,client-events-02,client-events-03,client-events-04,client-events-05,client-events-06,client-events-07,client-events-08,client-events-09,client-events-10,client-events-11,client-events-12,entity-scopes-02,entity-scopes-05,entity-scopes-10,messaging-01,messaging-02,messaging-03,messaging-04,messaging-05,messaging-06,messaging-15,messaging-16,messaging-17,messaging-18,messaging-19,messaging-20,server-events-00,server-events-01,server-events-02,server-events-03,server-events-04,server-events-05,server-events-06,server-events-07,server-events-08,server-events-09,server-events-10,server-events-11,server-events-12,server-events-13,world-integration-01,world-integration-02,world-integration-03,world-integration-04,world-integration-05,world-integration-06,world-integration-07,world-integration-08, |
| harness_scenarios.rs | (check manually) | entity-replication-01,entity-scopes-01, |
| integration_transport_parity.rs | (check manually) | entity-scopes-01,entity-scopes-03,entity-scopes-04,transport-01,transport-02,transport-03,transport-04,world-integration-01,world-integration-02,world-integration-03,world-integration-09, |
| messaging_channels.rs | (check manually) | messaging-05,messaging-06,messaging-07,messaging-08,messaging-09,messaging-10,messaging-12,messaging-14,messaging-17, |
| protocol_schema_versioning.rs | (check manually) | messaging-07,messaging-08,messaging-09,messaging-10,messaging-11,messaging-12,messaging-13,messaging-14,transport-01,transport-02,transport-03,transport-04,transport-05, |
| rooms_scope_snapshot.rs | (check manually) | entity-replication-01,entity-replication-02,entity-replication-03,entity-replication-04,entity-replication-05,entity-replication-06,entity-replication-07,entity-replication-08,entity-replication-09,entity-replication-10,entity-replication-11,entity-replication-12,entity-scopes-01,entity-scopes-02,entity-scopes-03,entity-scopes-04,entity-scopes-05,entity-scopes-06,entity-scopes-07,entity-scopes-08,entity-scopes-09,entity-scopes-10,entity-scopes-11,entity-scopes-12,entity-scopes-13,entity-scopes-14,entity-scopes-15, |
| time_ticks_transport.rs | (check manually) | commands-01,commands-02,commands-03,commands-04,commands-05,time-01,time-02,time-03,time-04,time-05,time-06,time-07,time-08,time-09,time-10,time-11,time-12,transport-01,transport-02,transport-03,transport-04,transport-05, |

---

## Summary

- **Total Contracts:** 185
- **Contracts with Tests:** 176
- **Coverage:** 95%
