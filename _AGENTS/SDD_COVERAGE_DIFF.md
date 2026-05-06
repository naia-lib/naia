# SDD Migration Coverage Diff

> **AUTO-GENERATED** by `_AGENTS/scripts/coverage_diff.py`. To refresh:
> `python3 _AGENTS/scripts/coverage_diff.py --markdown > _AGENTS/SDD_COVERAGE_DIFF.md`
>
> Living artifact for Phase D progress: every contract ID currently in
> the **Pending migration** table is a Phase D target. When the table
> empties, the parity gate for Phase F (delete legacy_tests) is met.

- Legacy (215 contract IDs in legacy_tests/)
- Namako: 135 contract IDs in features/
- Both: **133**
- Legacy-only (PENDING migration): **82**
- Namako-only (new in SDD): 2

## Pending migration

| Contract ID | Source files |
|---|---|
| `client-events-00` | test/harness/legacy_tests/13_client_events_api.rs |
| `client-events-01` | test/harness/legacy_tests/13_client_events_api.rs |
| `client-events-02` | test/harness/legacy_tests/13_client_events_api.rs |
| `client-events-03` | test/harness/legacy_tests/13_client_events_api.rs |
| `client-events-05` | test/harness/legacy_tests/13_client_events_api.rs |
| `client-events-10` | test/harness/legacy_tests/13_client_events_api.rs |
| `client-events-11` | test/harness/legacy_tests/13_client_events_api.rs |
| `client-events-12` | test/harness/legacy_tests/13_client_events_api.rs |
| `connection-04` | test/harness/legacy_tests/01_connection_lifecycle.rs |
| `connection-06` | test/harness/legacy_tests/01_connection_lifecycle.rs |
| `connection-08` | test/harness/legacy_tests/01_connection_lifecycle.rs |
| `connection-09` | test/harness/legacy_tests/01_connection_lifecycle.rs |
| `connection-10` | test/harness/legacy_tests/01_connection_lifecycle.rs |
| `connection-11` | test/harness/legacy_tests/01_connection_lifecycle.rs |
| `connection-14` | test/harness/legacy_tests/01_connection_lifecycle.rs |
| `connection-16` | test/harness/legacy_tests/01_connection_lifecycle.rs |
| `connection-18` | test/harness/legacy_tests/01_connection_lifecycle.rs |
| `connection-20` | test/harness/legacy_tests/01_connection_lifecycle.rs |
| `connection-22` | test/harness/legacy_tests/01_connection_lifecycle.rs |
| `connection-24` | test/harness/legacy_tests/01_connection_lifecycle.rs |
| `connection-26` | test/harness/legacy_tests/01_connection_lifecycle.rs |
| `entity-authority-02` | test/harness/legacy_tests/11_entity_authority.rs |
| `entity-authority-03` | test/harness/legacy_tests/11_entity_authority.rs |
| `entity-authority-04` | test/harness/legacy_tests/11_entity_authority.rs |
| `entity-authority-05` | test/harness/legacy_tests/11_entity_authority.rs |
| `entity-authority-08` | test/harness/legacy_tests/11_entity_authority.rs |
| `entity-authority-11` | test/harness/legacy_tests/11_entity_authority.rs |
| `entity-authority-12` | test/harness/legacy_tests/11_entity_authority.rs |
| `entity-authority-13` | test/harness/legacy_tests/11_entity_authority.rs |
| `entity-authority-14` | test/harness/legacy_tests/11_entity_authority.rs |
| `entity-authority-15` | test/harness/legacy_tests/11_entity_authority.rs |
| `entity-delegation-01` | test/harness/legacy_tests/10_entity_delegation.rs |
| `entity-delegation-02` | test/harness/legacy_tests/10_entity_delegation.rs |
| `entity-delegation-03` | test/harness/legacy_tests/10_entity_delegation.rs |
| `entity-delegation-04` | test/harness/legacy_tests/10_entity_delegation.rs |
| `entity-delegation-05` | test/harness/legacy_tests/10_entity_delegation.rs |
| `entity-delegation-07` | test/harness/legacy_tests/10_entity_delegation.rs |
| `entity-delegation-08` | test/harness/legacy_tests/10_entity_delegation.rs |
| `entity-delegation-09` | test/harness/legacy_tests/10_entity_delegation.rs |
| `entity-delegation-10` | test/harness/legacy_tests/10_entity_delegation.rs |
| `entity-delegation-11` | test/harness/legacy_tests/10_entity_delegation.rs |
| `entity-delegation-12` | test/harness/legacy_tests/10_entity_delegation.rs |
| `entity-delegation-15` | test/harness/legacy_tests/10_entity_delegation.rs |
| `entity-delegation-16` | test/harness/legacy_tests/10_entity_delegation.rs |
| `entity-ownership-01` | test/harness/legacy_tests/08_entity_ownership.rs |
| `entity-ownership-03` | test/harness/legacy_tests/08_entity_ownership.rs |
| `entity-ownership-04` | test/harness/legacy_tests/08_entity_ownership.rs |
| `entity-ownership-05` | test/harness/legacy_tests/08_entity_ownership.rs |
| `entity-ownership-06` | test/harness/legacy_tests/08_entity_ownership.rs |
| `entity-ownership-07` | test/harness/legacy_tests/08_entity_ownership.rs |
| `entity-ownership-09` | test/harness/legacy_tests/08_entity_ownership.rs |
| `entity-ownership-10` | test/harness/legacy_tests/08_entity_ownership.rs |
| `entity-ownership-11` | test/harness/legacy_tests/08_entity_ownership.rs |
| `entity-ownership-12` | test/harness/legacy_tests/08_entity_ownership.rs |
| `entity-ownership-13` | test/harness/legacy_tests/08_entity_ownership.rs |
| `entity-ownership-14` | test/harness/legacy_tests/08_entity_ownership.rs |
| `entity-publication-01` | test/harness/legacy_tests/09_entity_publication.rs |
| `entity-publication-02` | test/harness/legacy_tests/09_entity_publication.rs |
| `entity-publication-03` | test/harness/legacy_tests/09_entity_publication.rs |
| `entity-publication-04` | test/harness/legacy_tests/09_entity_publication.rs |
| `entity-publication-06` | test/harness/legacy_tests/09_entity_publication.rs |
| `entity-publication-07` | test/harness/legacy_tests/09_entity_publication.rs |
| `entity-publication-08` | test/harness/legacy_tests/09_entity_publication.rs |
| `entity-publication-09` | test/harness/legacy_tests/09_entity_publication.rs |
| `entity-publication-10` | test/harness/legacy_tests/09_entity_publication.rs |
| `entity-publication-11` | test/harness/legacy_tests/09_entity_publication.rs |
| `server-events-00` | test/harness/legacy_tests/12_server_events_api.rs |
| `server-events-01` | test/harness/legacy_tests/12_server_events_api.rs |
| `server-events-02` | test/harness/legacy_tests/12_server_events_api.rs |
| `server-events-03` | test/harness/legacy_tests/12_server_events_api.rs |
| `server-events-04` | test/harness/legacy_tests/12_server_events_api.rs |
| `server-events-05` | test/harness/legacy_tests/12_server_events_api.rs |
| `server-events-06` | test/harness/legacy_tests/12_server_events_api.rs |
| `server-events-08` | test/harness/legacy_tests/12_server_events_api.rs |
| `server-events-10` | test/harness/legacy_tests/12_server_events_api.rs |
| `server-events-11` | test/harness/legacy_tests/12_server_events_api.rs |
| `server-events-12` | test/harness/legacy_tests/12_server_events_api.rs |
| `server-events-13` | test/harness/legacy_tests/12_server_events_api.rs |
| `world-integration-01` | test/harness/legacy_tests/14_world_integration.rs |
| `world-integration-02` | test/harness/legacy_tests/14_world_integration.rs |
| `world-integration-03` | test/harness/legacy_tests/14_world_integration.rs |
| `world-integration-04` | test/harness/legacy_tests/14_world_integration.rs |
