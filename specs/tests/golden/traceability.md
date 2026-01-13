# Contract Traceability Matrix

**Generated:** 1970-01-01 00:00 UTC

This matrix shows the bidirectional mapping between contracts and tests.

---

## Contracts → Tests

| Contract | Test Function | Test File | Status |
|----------|---------------|-----------|--------|
| `client-events-00` | `accessing_an_entity_after_despawn_is_safely_rejected` | 12_server_events_api.rs | COVERED |
| `client-events-01` | `(manual check)` | 13_client_events_api.rs | COVERED |
| `client-events-02` | `(manual check)` | 13_client_events_api.rs | COVERED |
| `client-events-03` | `client_never_sees_update_or_remove_events_for_entities_that_were_never_in_scope` | 13_client_events_api.rs | COVERED |
| `client-events-04` | `client_never_sees_update_or_remove_events_for_entities_that_were_never_in_scope` | 13_client_events_api.rs | COVERED |
| `client-events-05` | `client_never_sees_update_or_insert_events_before_seeing_a_spawn_event` | 13_client_events_api.rs | COVERED |
| `client-events-06` | `client_never_sees_update_or_insert_events_before_seeing_a_spawn_event` | 13_client_events_api.rs | COVERED |
| `client-events-07` | `client_never_sees_update_or_insert_events_before_seeing_a_spawn_event` | 13_client_events_api.rs | COVERED |
| `client-events-08` | `(manual check)` | 13_client_events_api.rs | COVERED |
| `client-events-09` | `(manual check)` | 13_client_events_api.rs | COVERED |
| `client-events-10` | `client_message_events_are_grouped_and_typed_correctly_per_channel` | 13_client_events_api.rs | COVERED |
| `client-events-11` | `(manual check)` | 13_client_events_api.rs | COVERED |
| `client-events-12` | `(manual check)` | 13_client_events_api.rs | COVERED |
| `commands-01` | `command_history_preserves_and_replays_commands_after_correction` | 04_time_ticks_commands.rs | COVERED |
| `commands-02` | `command_history_preserves_and_replays_commands_after_correction` | 04_time_ticks_commands.rs | COVERED |
| `commands-03` | `switching_channel_reliability_only_changes_documented_semantics` | 04_time_ticks_commands.rs | COVERED |
| `commands-03a` | `command_sequence_is_assigned_to_tick_buffered_messages` | 04_time_ticks_commands.rs | COVERED |
| `commands-03b` | `commands_applied_in_sequence_order` | 04_time_ticks_commands.rs | COVERED |
| `commands-03c` | `(manual check)` | 04_time_ticks_commands.rs | COVERED |
| `commands-03d` | `duplicate_commands_are_dropped` | 04_time_ticks_commands.rs | COVERED |
| `commands-04` | `command_history_discards_old_commands_beyond_its_window` | 04_time_ticks_commands.rs | COVERED |
| `commands-05` | `extreme_jitter_and_reordering_preserve_channel_contracts` | 02_transport.rs | COVERED |
| `common-01` | `api_misuse_returns_error_not_panic` | 00_common.rs | COVERED |
| `common-02` | `remote_untrusted_input_does_not_panic` | 00_common.rs | COVERED |
| `common-02a` | `(manual check)` | 00_common.rs | COVERED |
| `common-03` | `(manual check)` | 00_common.rs | COVERED |
| `common-04` | `warnings_are_debug_only_and_non_normative` | 00_common.rs | COVERED |
| `common-05` | `determinism_under_deterministic_inputs` | 00_common.rs | COVERED |
| `common-06` | `per_tick_operations_resolve_deterministically` | 00_common.rs | COVERED |
| `common-07` | `tests_do_not_assert_on_logs` | 00_common.rs | COVERED |
| `common-08` | `test_obligation_template_followed` | 00_common.rs | COVERED |
| `common-09` | `observable_signals_are_defined` | 00_common.rs | COVERED |
| `common-10` | `fixed_invariants_are_locked` | 00_common.rs | COVERED |
| `common-11` | `configurable_defaults_can_be_overridden` | 00_common.rs | COVERED |
| `common-11a` | `new_constants_start_as_invariants` | 00_common.rs | COVERED |
| `common-12` | `reading_metrics_does_not_influence_behavior` | 00_common.rs | COVERED |
| `common-12a` | `test_tolerance_constants_documented` | 00_common.rs | COVERED |
| `common-13` | `metrics_do_not_affect_replicated_state` | 00_common.rs | COVERED |
| `common-14` | `reconnect_is_fresh_session` | 00_common.rs | COVERED |
| `connection-01` | `basic_connect_disconnect_lifecycle` | 01_connection_lifecycle.rs | COVERED |
| `connection-02` | `basic_connect_disconnect_lifecycle` | 01_connection_lifecycle.rs | COVERED |
| `connection-03` | `connect_event_ordering_stable` | 01_connection_lifecycle.rs | COVERED |
| `connection-04` | `connect_event_ordering_stable` | 01_connection_lifecycle.rs | COVERED |
| `connection-05` | `disconnect_idempotent_and_clean` | 01_connection_lifecycle.rs | COVERED |
| `connection-06` | `disconnect_idempotent_and_clean` | 01_connection_lifecycle.rs | COVERED |
| `connection-07` | `(manual check)` | 01_connection_lifecycle.rs | COVERED |
| `connection-08` | `(manual check)` | 01_connection_lifecycle.rs | COVERED |
| `connection-09` | `(manual check)` | 01_connection_lifecycle.rs | COVERED |
| `connection-10` | `basic_connect_disconnect_lifecycle` | 01_connection_lifecycle.rs | COVERED |
| `connection-11` | `(manual check)` | 01_connection_lifecycle.rs | COVERED |
| `connection-12` | `(manual check)` | 01_connection_lifecycle.rs | COVERED |
| `connection-13` | `(manual check)` | 01_connection_lifecycle.rs | COVERED |
| `connection-14` | `(manual check)` | 01_connection_lifecycle.rs | COVERED |
| `connection-14a` | `protocol_id_verified_before_connect_event` | 01_connection_lifecycle.rs | COVERED |
| `connection-15` | `(manual check)` | 01_connection_lifecycle.rs | COVERED |
| `connection-16` | `(manual check)` | 01_connection_lifecycle.rs | COVERED |
| `connection-17` | `(manual check)` | 01_connection_lifecycle.rs | COVERED |
| `connection-18` | `(manual check)` | 01_connection_lifecycle.rs | COVERED |
| `connection-19` | `(manual check)` | 01_connection_lifecycle.rs | COVERED |
| `connection-20` | `(manual check)` | 01_connection_lifecycle.rs | COVERED |
| `connection-21` | `(manual check)` | 01_connection_lifecycle.rs | COVERED |
| `connection-22` | `(manual check)` | 01_connection_lifecycle.rs | COVERED |
| `connection-23` | `malformed_identity_token_rejected` | 01_connection_lifecycle.rs | COVERED |
| `connection-24` | `malformed_identity_token_rejected` | 01_connection_lifecycle.rs | COVERED |
| `connection-25` | `(manual check)` | 01_connection_lifecycle.rs | COVERED |
| `connection-26` | `(manual check)` | 01_connection_lifecycle.rs | COVERED |
| `connection-27` | `(manual check)` | 01_connection_lifecycle.rs | COVERED |
| `connection-28` | `reconnect_is_fresh_session` | 01_connection_lifecycle.rs | COVERED |
| `connection-29` | `same_protocol_produces_same_id` | 01_connection_lifecycle.rs | COVERED |
| `connection-30` | `(manual check)` | 01_connection_lifecycle.rs | COVERED |
| `connection-31` | `matched_protocol_id_allows_connection` | 01_connection_lifecycle.rs | COVERED |
| `connection-32` | `(manual check)` | 01_connection_lifecycle.rs | COVERED |
| `connection-33` | `(manual check)` | 01_connection_lifecycle.rs | COVERED |
| `entity-authority-01` | `client_request_authority_on_non_delegated_returns_err_not_delegated` | 10_entity_delegation.rs | COVERED |
| `entity-authority-02` | `holder_can_mutate_delegated_entity` | 11_entity_authority.rs | COVERED |
| `entity-authority-03` | `server_held_authority_is_indistinguishable_from_client_is_denied` | 11_entity_authority.rs | COVERED |
| `entity-authority-04` | `request_authority_available_grants_to_requester_and_denies_everyone_else` | 11_entity_authority.rs | COVERED |
| `entity-authority-05` | `request_authority_available_grants_to_requester_and_denies_everyone_else` | 11_entity_authority.rs | COVERED |
| `entity-authority-06` | `holder_release_authority_transitions_everyone_to_available` | 11_entity_authority.rs | COVERED |
| `entity-authority-07` | `release_authority_when_not_holder_fails_err_not_holder` | 11_entity_authority.rs | COVERED |
| `entity-authority-08` | `request_authority_available_grants_to_requester_and_denies_everyone_else` | 11_entity_authority.rs | COVERED |
| `entity-authority-09` | `server_held_authority_is_indistinguishable_from_client_is_denied` | 11_entity_authority.rs | COVERED |
| `entity-authority-10` | `give_authority_assigns_to_client_and_denies_everyone_else` | 11_entity_authority.rs | COVERED |
| `entity-authority-11` | `(manual check)` | 11_entity_authority.rs | COVERED |
| `entity-authority-12` | `holder_release_authority_transitions_everyone_to_available` | 11_entity_authority.rs | COVERED |
| `entity-authority-13` | `disable_delegation_clears_authority_semantics` | 10_entity_delegation.rs | COVERED |
| `entity-authority-14` | `server_give_authority_requires_scope` | 11_entity_authority.rs | COVERED |
| `entity-authority-15` | `(manual check)` | 11_entity_authority.rs | COVERED |
| `entity-authority-16` | `give_authority_assigns_to_client_and_denies_everyone_else` | 11_entity_authority.rs | COVERED |
| `entity-delegation-01` | `cannot_delegate_client_owned_unpublished_err_not_published` | 10_entity_delegation.rs | COVERED |
| `entity-delegation-02` | `cannot_delegate_client_owned_unpublished_err_not_published` | 10_entity_delegation.rs | COVERED |
| `entity-delegation-03` | `enable_delegation_makes_entity_available_for_all_in_scope_clients` | 10_entity_delegation.rs | COVERED |
| `entity-delegation-04` | `delegating_client_owned_published_migrates_identity_without_despawn_spawn` | 10_entity_delegation.rs | COVERED |
| `entity-delegation-05` | `delegating_client_owned_published_migrates_identity_without_despawn_spawn` | 10_entity_delegation.rs | COVERED |
| `entity-delegation-06` | `migration_assigns_initial_authority_to_owner_if_owner_in_scope` | 10_entity_delegation.rs | COVERED |
| `entity-delegation-07` | `migration_assigns_initial_authority_to_owner_if_owner_in_scope` | 10_entity_delegation.rs | COVERED |
| `entity-delegation-08` | `migration_yields_no_holder_if_owner_out_of_scope` | 10_entity_delegation.rs | COVERED |
| `entity-delegation-09` | `migration_yields_no_holder_if_owner_out_of_scope` | 10_entity_delegation.rs | COVERED |
| `entity-delegation-10` | `authority_releases_when_holder_goes_out_of_scope` | 06_entity_scopes.rs | COVERED |
| `entity-delegation-11` | `authority_releases_when_holder_disconnects` | 06_entity_scopes.rs | COVERED |
| `entity-delegation-12` | `after_migration_writes_follow_delegated_rules` | 10_entity_delegation.rs | COVERED |
| `entity-delegation-13` | `disable_delegation_while_client_holds_authority` | 10_entity_delegation.rs | COVERED |
| `entity-delegation-14` | `auth_granted_emitted_exactly_once_on_available_to_granted` | 10_entity_delegation.rs | COVERED |
| `entity-delegation-15` | `auth_granted_emitted_exactly_once_on_available_to_granted` | 10_entity_delegation.rs | COVERED |
| `entity-delegation-16` | `disable_delegation_while_client_holds_authority` | 10_entity_delegation.rs | COVERED |
| `entity-delegation-17` | `disable_delegation_clears_authority_semantics` | 10_entity_delegation.rs | COVERED |
| `entity-ownership-01` | `entity_has_exactly_one_owner_at_creation` | 08_entity_ownership.rs | COVERED |
| `entity-ownership-02` | `unauthorized_client_write_does_not_affect_server_state` | 08_entity_ownership.rs | COVERED |
| `entity-ownership-03` | `client_writes_to_nondelegated_server_entity_are_ignored` | 08_entity_ownership.rs | COVERED |
| `entity-ownership-04` | `client_owned_entity_does_not_emit_authority_events` | 08_entity_ownership.rs | COVERED |
| `entity-ownership-05` | `write_to_unowned_entity_returns_error` | 08_entity_ownership.rs | COVERED |
| `entity-ownership-06` | `client_sees_other_clients_entities_as_server_owned` | 08_entity_ownership.rs | COVERED |
| `entity-ownership-07` | `local_mutation_persists_until_server_update` | 08_entity_ownership.rs | COVERED |
| `entity-ownership-08` | `local_only_component_persists_until_despawn` | 08_entity_ownership.rs | COVERED |
| `entity-ownership-09` | `removing_server_component_from_unowned_entity_returns_error` | 08_entity_ownership.rs | COVERED |
| `entity-ownership-10` | `server_owned_entity_cannot_become_client_owned` | 08_entity_ownership.rs | COVERED |
| `entity-ownership-11` | `enabling_delegation_transfers_ownership_to_server` | 08_entity_ownership.rs | COVERED |
| `entity-ownership-12` | `owning_client_retains_entities_across_scope_changes` | 08_entity_ownership.rs | COVERED |
| `entity-ownership-13` | `client_disconnect_despawns_owned_entities` | 08_entity_ownership.rs | COVERED |
| `entity-ownership-14` | `no_writes_for_out_of_scope_entities` | 08_entity_ownership.rs | COVERED |
| `entity-publication-01` | `client_owned_entities_emit_no_authority_events` | 09_entity_publication.rs | COVERED |
| `entity-publication-02` | `client_owned_unpublished_is_visible_only_to_owner` | 09_entity_publication.rs | COVERED |
| `entity-publication-03` | `client_owned_entities_emit_no_authority_events` | 09_entity_publication.rs | COVERED |
| `entity-publication-04` | `(manual check)` | 09_entity_publication.rs | COVERED |
| `entity-publication-05` | `publish_toggle_published_to_unpublished_forcibly_despawns_for_non_owners` | 09_entity_publication.rs | COVERED |
| `entity-publication-06` | `client_owned_published_may_be_scoped_to_non_owners` | 09_entity_publication.rs | COVERED |
| `entity-publication-07` | `client_owned_unpublished_is_visible_only_to_owner` | 09_entity_publication.rs | COVERED |
| `entity-publication-08` | `publish_toggle_published_to_unpublished_forcibly_despawns_for_non_owners` | 09_entity_publication.rs | COVERED |
| `entity-publication-09` | `client_owned_published_rejects_non_owner_mutations` | 09_entity_publication.rs | COVERED |
| `entity-publication-10` | `(manual check)` | 09_entity_publication.rs | COVERED |
| `entity-publication-11` | `(manual check)` | 09_entity_publication.rs | COVERED |
| `entity-replication-01` | `entities_only_replicate_when_room_scope_match` | 06_entity_scopes.rs | COVERED |
| `entity-replication-02` | `(manual check)` | 06_entity_scopes.rs | COVERED |
| `entity-replication-03` | `(manual check)` | 06_entity_scopes.rs | COVERED |
| `entity-replication-04` | `(manual check)` | 06_entity_scopes.rs | COVERED |
| `entity-replication-05` | `no_updates_before_spawn_and_none_after_despawn` | 07_entity_replication.rs | COVERED |
| `entity-replication-06` | `component_insertion_after_initial_spawn` | 07_entity_replication.rs | COVERED |
| `entity-replication-07` | `(manual check)` | 07_entity_replication.rs | COVERED |
| `entity-replication-08` | `component_updates_propagate_consistently_across_clients` | 07_entity_replication.rs | COVERED |
| `entity-replication-09` | `stable_logical_identity_across_clients_in_steady_state` | 07_entity_replication.rs | COVERED |
| `entity-replication-10` | `(manual check)` | 07_entity_replication.rs | COVERED |
| `entity-replication-11` | `entering_scope_mid_lifetime_yields_consistent_snapshot` | 06_entity_scopes.rs | COVERED |
| `entity-replication-12` | `(manual check)` | 06_entity_scopes.rs | COVERED |
| `entity-scopes-01` | `entities_only_replicate_when_room_scope_match` | 06_entity_scopes.rs | COVERED |
| `entity-scopes-02` | `moving_user_between_rooms_updates_scope` | 06_entity_scopes.rs | COVERED |
| `entity-scopes-03` | `moving_entity_between_rooms_updates_scope` | 06_entity_scopes.rs | COVERED |
| `entity-scopes-04` | `(manual check)` | 06_entity_scopes.rs | COVERED |
| `entity-scopes-05` | `(manual check)` | 06_entity_scopes.rs | COVERED |
| `entity-scopes-06` | `authority_releases_when_holder_goes_out_of_scope` | 06_entity_scopes.rs | COVERED |
| `entity-scopes-07` | `authority_releases_when_holder_goes_out_of_scope` | 06_entity_scopes.rs | COVERED |
| `entity-scopes-08` | `authority_releases_when_holder_disconnects` | 06_entity_scopes.rs | COVERED |
| `entity-scopes-09` | `moving_user_between_rooms_updates_scope` | 06_entity_scopes.rs | COVERED |
| `entity-scopes-10` | `moving_entity_between_rooms_updates_scope` | 06_entity_scopes.rs | COVERED |
| `entity-scopes-11` | `re_entering_scope_yields_correct_current_auth_status` | 06_entity_scopes.rs | COVERED |
| `entity-scopes-12` | `manual_user_scope_exclude_hides_entity_despite_shared_room` | 06_entity_scopes.rs | COVERED |
| `entity-scopes-13` | `re_entering_scope_yields_correct_current_auth_status` | 06_entity_scopes.rs | COVERED |
| `entity-scopes-14` | `entering_scope_mid_lifetime_yields_consistent_snapshot` | 06_entity_scopes.rs | COVERED |
| `entity-scopes-15` | `(manual check)` | 06_entity_scopes.rs | COVERED |
| `messaging-01` | `(manual check)` | 12_server_events_api.rs | COVERED |
| `messaging-02` | `(manual check)` | 12_server_events_api.rs | COVERED |
| `messaging-03` | `misusing_channel_types_yields_defined_failure` | 03_messaging.rs | COVERED |
| `messaging-04` | `protocol_id_verified_before_connect_event` | 01_connection_lifecycle.rs | COVERED |
| `messaging-05` | `(manual check)` | 03_messaging.rs | COVERED |
| `messaging-06` | `unordered_unreliable_channel_shows_best_effort_semantics` | 03_messaging.rs | COVERED |
| `messaging-07` | `sequenced_unreliable_channel_discards_late_outdated_updates` | 03_messaging.rs | COVERED |
| `messaging-08` | `client_to_server_request_yields_exactly_one_response` | 03_messaging.rs | COVERED |
| `messaging-09` | `ordered_reliable_channel_ignores_duplicated_packets` | 03_messaging.rs | COVERED |
| `messaging-10` | `sequenced_reliable_channel_only_exposes_the_latest_message_in_a_stream` | 03_messaging.rs | COVERED |
| `messaging-11` | `(manual check)` | 03_messaging.rs | COVERED |
| `messaging-12` | `channel_separation_for_different_message_types` | 03_messaging.rs | COVERED |
| `messaging-13` | `channel_separation_for_different_message_types` | 03_messaging.rs | COVERED |
| `messaging-14` | `channel_separation_for_different_message_types` | 03_messaging.rs | COVERED |
| `messaging-15` | `(manual check)` | 12_server_events_api.rs | COVERED |
| `messaging-15-a` | `(manual check)` | 03_messaging.rs | COVERED |
| `messaging-16` | `(manual check)` | 12_server_events_api.rs | COVERED |
| `messaging-17` | `ordered_reliable_channel_ignores_duplicated_packets` | 03_messaging.rs | COVERED |
| `messaging-18` | `(manual check)` | 13_client_events_api.rs | COVERED |
| `messaging-19` | `sending_messages_or_requests_on_a_disconnected_or_rejected_connection_is_safe` | 12_server_events_api.rs | COVERED |
| `messaging-20` | `sending_messages_or_requests_on_a_disconnected_or_rejected_connection_is_safe` | 12_server_events_api.rs | COVERED |
| `messaging-21` | `request_id_uniqueness` | 03_messaging.rs | COVERED |
| `messaging-22` | `response_matching_to_request` | 03_messaging.rs | COVERED |
| `messaging-23` | `request_timeout_semantics` | 03_messaging.rs | COVERED |
| `messaging-24` | `disconnect_cancels_pending_requests` | 03_messaging.rs | COVERED |
| `messaging-25` | `request_deduplication` | 03_messaging.rs | COVERED |
| `messaging-26` | `rpc_ordering_on_ordered_channel` | 03_messaging.rs | COVERED |
| `messaging-27` | `fire_and_forget_request` | 03_messaging.rs | COVERED |
| `observability-01` | `metrics_do_not_affect_replicated_state_correctness` | 05_observability_metrics.rs | COVERED |
| `observability-01a` | `querying_metrics_does_not_affect_tick_pacing` | 05_observability_metrics.rs | COVERED |
| `observability-02` | `metrics_apis_safe_after_construction` | 05_observability_metrics.rs | COVERED |
| `observability-03` | `rtt_must_be_non_negative` | 05_observability_metrics.rs | COVERED |
| `observability-04` | `rtt_stable_under_normal_conditions` | 05_observability_metrics.rs | COVERED |
| `observability-05` | `throughput_must_be_non_negative` | 05_observability_metrics.rs | COVERED |
| `observability-06` | `bandwidth_exposes_both_directions` | 05_observability_metrics.rs | COVERED |
| `observability-07` | `metrics_cleanup_on_disconnect` | 05_observability_metrics.rs | COVERED |
| `observability-08` | `time_source_monotonic_consistency` | 05_observability_metrics.rs | COVERED |
| `observability-09` | `per_direction_metrics_consistency` | 05_observability_metrics.rs | COVERED |
| `observability-10` | `metrics_queryable_without_feature_flags` | 05_observability_metrics.rs | COVERED |
| `server-events-00` | `(manual check)` | 12_server_events_api.rs | COVERED |
| `server-events-01` | `(manual check)` | 12_server_events_api.rs | COVERED |
| `server-events-02` | `component_update_events_reflect_correct_multiplicity_per_user` | 12_server_events_api.rs | COVERED |
| `server-events-03` | `component_update_events_reflect_correct_multiplicity_per_user` | 12_server_events_api.rs | COVERED |
| `server-events-04` | `(manual check)` | 12_server_events_api.rs | COVERED |
| `server-events-05` | `(manual check)` | 12_server_events_api.rs | COVERED |
| `server-events-06` | `(manual check)` | 12_server_events_api.rs | COVERED |
| `server-events-07` | `accessing_non_existent_entity_yields_safe_failure_not_panic` | 12_server_events_api.rs | COVERED |
| `server-events-08` | `accessing_non_existent_entity_yields_safe_failure_not_panic` | 12_server_events_api.rs | COVERED |
| `server-events-09` | `accessing_an_entity_after_despawn_is_safely_rejected` | 12_server_events_api.rs | COVERED |
| `server-events-10` | `accessing_an_entity_after_despawn_is_safely_rejected` | 12_server_events_api.rs | COVERED |
| `server-events-11` | `mutating_out_of_scope_entity_for_a_given_user_is_ignored_or_errors_predictably` | 12_server_events_api.rs | COVERED |
| `server-events-12` | `mutating_out_of_scope_entity_for_a_given_user_is_ignored_or_errors_predictably` | 12_server_events_api.rs | COVERED |
| `server-events-13` | `sending_messages_or_requests_on_a_disconnected_or_rejected_connection_is_safe` | 12_server_events_api.rs | COVERED |
| `time-01` | `deterministic_replay_of_a_scenario` | 04_time_ticks_commands.rs | COVERED |
| `time-02` | `server_and_client_tick_indices_advance_monotonically` | 04_time_ticks_commands.rs | COVERED |
| `time-03` | `server_and_client_tick_indices_advance_monotonically` | 04_time_ticks_commands.rs | COVERED |
| `time-04` | `pausing_and_resuming_time_does_not_create_extra_ticks` | 04_time_ticks_commands.rs | COVERED |
| `time-05` | `pausing_and_resuming_time_does_not_create_extra_ticks` | 04_time_ticks_commands.rs | COVERED |
| `time-06` | `command_history_preserves_and_replays_commands_after_correction` | 04_time_ticks_commands.rs | COVERED |
| `time-07` | `tick_index_wraparound_does_not_break_progression_or_ordering` | 04_time_ticks_commands.rs | COVERED |
| `time-08` | `tick_index_wraparound_does_not_break_progression_or_ordering` | 04_time_ticks_commands.rs | COVERED |
| `time-09` | `sequence_number_wraparound_for_channels_preserves_ordering_semantics` | 04_time_ticks_commands.rs | COVERED |
| `time-10` | `long_running_scenario_maintains_stable_memory_and_state` | 04_time_ticks_commands.rs | COVERED |
| `time-11` | `long_running_scenario_maintains_stable_memory_and_state` | 04_time_ticks_commands.rs | COVERED |
| `time-12` | `reported_ping_remains_bounded_under_jitter_and_loss` | 04_time_ticks_commands.rs | COVERED |
| `transport-01` | `extreme_jitter_and_reordering_preserve_channel_contracts` | 02_transport.rs | COVERED |
| `transport-02` | `extreme_jitter_and_reordering_preserve_channel_contracts` | 02_transport.rs | COVERED |
| `transport-03` | `out_of_order_packet_handling_does_not_regress_to_older_state` | 02_transport.rs | COVERED |
| `transport-04` | `fragment_loss_causes_older_state_until_a_full_later_update` | 02_transport.rs | COVERED |
| `transport-05` | `schema_incompatibility_produces_immediate_clear_failure` | 02_transport.rs | COVERED |
| `world-integration-01` | `server_world_integration_receives_every_insert_update_remove_exactly_once` | 14_world_integration.rs | COVERED |
| `world-integration-02` | `server_world_integration_receives_every_insert_update_remove_exactly_once` | 14_world_integration.rs | COVERED |
| `world-integration-03` | `server_world_integration_receives_every_insert_update_remove_exactly_once` | 14_world_integration.rs | COVERED |
| `world-integration-04` | `client_world_integration_stays_in_lockstep_with_naias_view` | 14_world_integration.rs | COVERED |
| `world-integration-05` | `client_world_integration_stays_in_lockstep_with_naias_view` | 14_world_integration.rs | COVERED |
| `world-integration-06` | `(manual check)` | 14_world_integration.rs | COVERED |
| `world-integration-07` | `(manual check)` | 14_world_integration.rs | COVERED |
| `world-integration-08` | `(manual check)` | 14_world_integration.rs | COVERED |
| `world-integration-09` | `(manual check)` | 14_world_integration.rs | COVERED |

---

## Tests → Contracts

| Test File | Test Function | Contracts Verified |
|-----------|---------------|--------------------|
| 00_common.rs | test_obligation_template_followed, | common-01,common-02,common-03,common-04,common-05,common-06,common-07,common-08,common-09,common-10,common-11,common-12,common-13,common-14, |
| 01_connection_lifecycle.rs | (check manually) | connection-01,connection-02,connection-03,connection-04,connection-05,connection-06,connection-07,connection-08,connection-09,connection-10,connection-11,connection-12,connection-13,connection-14,connection-15,connection-16,connection-17,connection-18,connection-19,connection-20,connection-21,connection-22,connection-23,connection-24,connection-25,connection-26,connection-27,connection-28,connection-29,connection-30,connection-31,connection-32,connection-33,messaging-04, |
| 02_transport.rs | (check manually) | commands-05,transport-01,transport-02,transport-03,transport-04,transport-05, |
| 03_messaging.rs | (check manually) | messaging-03,messaging-04,messaging-05,messaging-06,messaging-07,messaging-08,messaging-09,messaging-10,messaging-11,messaging-12,messaging-13,messaging-14,messaging-17,messaging-21,messaging-22,messaging-23,messaging-24,messaging-25,messaging-26,messaging-27, |
| 04_time_ticks_commands.rs | (check manually) | commands-01,commands-02,commands-03,commands-04,commands-05,time-01,time-02,time-03,time-04,time-05,time-06,time-07,time-08,time-09,time-10,time-11,time-12,transport-05, |
| 05_observability_metrics.rs | (check manually) | observability-01,observability-02,observability-03,observability-04,observability-05,observability-06,observability-07,observability-08,observability-09,observability-10, |
| 06_entity_scopes.rs | (check manually) | entity-delegation-10,entity-delegation-11,entity-replication-01,entity-replication-02,entity-replication-03,entity-replication-04,entity-replication-11,entity-replication-12,entity-scopes-01,entity-scopes-02,entity-scopes-03,entity-scopes-04,entity-scopes-05,entity-scopes-06,entity-scopes-07,entity-scopes-08,entity-scopes-09,entity-scopes-10,entity-scopes-11,entity-scopes-12,entity-scopes-13,entity-scopes-14,entity-scopes-15, |
| 07_entity_replication.rs | (check manually) | entity-replication-01,entity-replication-02,entity-replication-03,entity-replication-04,entity-replication-05,entity-replication-06,entity-replication-07,entity-replication-08,entity-replication-09,entity-replication-10,entity-replication-12,entity-scopes-01,entity-scopes-02,entity-scopes-07,entity-scopes-09,entity-scopes-10,entity-scopes-13, |
| 08_entity_ownership.rs | (check manually) | entity-ownership-01,entity-ownership-02,entity-ownership-03,entity-ownership-04,entity-ownership-05,entity-ownership-06,entity-ownership-07,entity-ownership-08,entity-ownership-09,entity-ownership-10,entity-ownership-11,entity-ownership-12,entity-ownership-13,entity-ownership-14, |
| 09_entity_publication.rs | (check manually) | entity-publication-01,entity-publication-02,entity-publication-03,entity-publication-04,entity-publication-05,entity-publication-06,entity-publication-07,entity-publication-08,entity-publication-09,entity-publication-10,entity-publication-11, |
| 10_entity_delegation.rs | (check manually) | entity-authority-01,entity-authority-13,entity-delegation-01,entity-delegation-02,entity-delegation-03,entity-delegation-04,entity-delegation-05,entity-delegation-06,entity-delegation-07,entity-delegation-08,entity-delegation-09,entity-delegation-10,entity-delegation-11,entity-delegation-12,entity-delegation-13,entity-delegation-14,entity-delegation-15,entity-delegation-16,entity-delegation-17, |
| 11_entity_authority.rs | (check manually) | entity-authority-02,entity-authority-03,entity-authority-04,entity-authority-05,entity-authority-06,entity-authority-07,entity-authority-08,entity-authority-09,entity-authority-10,entity-authority-11,entity-authority-12,entity-authority-14,entity-authority-15,entity-authority-16, |
| 12_server_events_api.rs | (check manually) | client-events-00,entity-scopes-10,messaging-01,messaging-02,messaging-15,messaging-16,messaging-19,messaging-20,server-events-00,server-events-01,server-events-02,server-events-03,server-events-04,server-events-05,server-events-06,server-events-07,server-events-08,server-events-09,server-events-10,server-events-11,server-events-12,server-events-13, |
| 13_client_events_api.rs | (check manually) | client-events-00,client-events-01,client-events-02,client-events-03,client-events-04,client-events-05,client-events-06,client-events-07,client-events-08,client-events-09,client-events-10,client-events-11,client-events-12,entity-scopes-02,entity-scopes-05,messaging-05,messaging-06,messaging-17,messaging-18, |
| 14_world_integration.rs | (check manually) | entity-scopes-01,entity-scopes-03,entity-scopes-04,world-integration-01,world-integration-02,world-integration-03,world-integration-04,world-integration-05,world-integration-06,world-integration-07,world-integration-08,world-integration-09, |

---

## Summary

- **Total Contracts:** 237
- **Contracts with Tests:** 227
- **Coverage:** 95%
