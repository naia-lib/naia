# Contract Coverage Gap Analysis

**Generated:** 2026-01-12
**Phase:** 2 (Gap Analysis)

---

## Summary

| Metric | Value |
|--------|-------|
| Total Contracts | 185 |
| Covered | 102 |
| Uncovered | 83 |
| Coverage | **55%** |

---

## Uncovered by Domain

| Domain | Uncovered | Notes |
|--------|-----------|-------|
| connection-* | 13 | Even-numbered contracts (02,04,06...) - likely paired with existing tests |
| messaging-* | 11 | Mostly higher numbers (11-20) - advanced/edge cases |
| observability-* | 9 | ALL uncovered (01-09) - entire domain untested |
| entity-delegation-* | 8 | Scattered gaps |
| client-events-* | 7 | API drain semantics |
| entity-authority-* | 7 | Complex state machine gaps |
| entity-publication-* | 6 | Publication state transitions |
| server-events-* | 6 | API drain semantics |
| world-integration-* | 5 | Integration adapter contracts |
| entity-replication-* | 4 | Core replication gaps |
| entity-scopes-* | 3 | Scope predicate gaps |
| time-* | 3 | Time/tick edge cases |
| commands-* | 1 | commands-05 only |
| transport-* | 0 | Fully covered |

---

## Risk Buckets

### HIGH RISK (Must test next)
State machines, ordering guarantees, authority transitions, scope predicates, disconnect/error behavior.

| Contract | Why High Risk | Target File |
|----------|---------------|-------------|
| entity-authority-01 | Authority predicate foundation | entity_authority_*.rs |
| entity-delegation-02 | Delegation state machine entry | entity_delegation_toggle.rs |
| entity-delegation-04 | Migration continuity | entity_migration_and_events.rs |
| entity-delegation-05 | Migration continuity | entity_migration_and_events.rs |
| entity-scopes-09 | Scope-on-despawn semantics | rooms_scope_snapshot.rs |
| entity-scopes-10 | Entity movement scoping | rooms_scope_snapshot.rs |
| entity-replication-06 | Update-before-insert buffer | entities_lifetime_identity.rs |
| connection-02 | Handshake state machine | connection_auth_identity.rs |
| connection-06 | Disconnect cleanup | connection_auth_identity.rs |
| server-events-01 | Drain semantics foundation | events_world_integration.rs |

### MEDIUM RISK (Schema/versioning, transport corner cases)

| Contract | Why Medium | Target File |
|----------|------------|-------------|
| messaging-11 through messaging-20 | Advanced channel semantics | messaging_channels.rs |
| client-events-01,02,04,06,07,09,12 | API drain edge cases | events_world_integration.rs |
| world-integration-02,03,05,07,08 | Adapter ordering | events_world_integration.rs |

### LOW RISK (Can defer)

| Contract | Why Low | Notes |
|----------|---------|-------|
| observability-01 through 09 | Metrics/debug only | May not need E2E tests |
| time-03, time-05, time-08 | Tick edge cases | Covered implicitly |
| commands-05 | Command history edge | Covered implicitly |

---

## Top 10 Next Contracts to Test

| # | Contract | Risk | Why | Target File | Existing Coverage? |
|---|----------|------|-----|-------------|-------------------|
| 1 | entity-authority-01 | HIGH | Foundation for all authority contracts | entity_authority_client_ops.rs | Partial (02,04,05 covered) |
| 2 | entity-delegation-02 | HIGH | Delegation precondition | entity_delegation_toggle.rs | No |
| 3 | entity-scopes-09 | HIGH | Despawn-on-scope-exit | rooms_scope_snapshot.rs | Referenced but not direct |
| 4 | entity-scopes-10 | HIGH | Entity room movement | rooms_scope_snapshot.rs | Referenced but not direct |
| 5 | entity-replication-06 | HIGH | Update buffering | entities_lifetime_identity.rs | No |
| 6 | connection-02 | HIGH | Handshake continuation | connection_auth_identity.rs | connection-01 covered |
| 7 | connection-06 | HIGH | Disconnect cleanup | connection_auth_identity.rs | connection-05 covered |
| 8 | server-events-01 | HIGH | Drain foundation | events_world_integration.rs | server-events-00 covered |
| 9 | entity-delegation-04 | HIGH | Migration identity | entity_migration_and_events.rs | 03 covered |
| 10 | entity-delegation-05 | HIGH | Migration completion | entity_migration_and_events.rs | 03 covered |

---

## Suggested Phase 3 Batch (Next 20 Contracts)

**Batch 1: Authority & Delegation State Machines** (8 contracts)
- entity-authority-01, 08, 11, 12, 13, 15, 16
- entity-delegation-02

**Batch 2: Scopes & Replication Core** (7 contracts)
- entity-scopes-09, 10, 13
- entity-replication-06, 09, 11, 12

**Batch 3: Events API Completion** (6 contracts)
- server-events-01, 03, 06
- client-events-01, 02, 04

---

## Notes

1. **Observability domain (9 contracts)**: Entirely untested. These are metrics/debug contracts that may not require E2E harness tests. Consider unit tests or marking as "documentation-only."

2. **Even-numbered connection contracts**: Pattern suggests these are "continuation" or "edge case" contracts paired with odd-numbered ones that ARE covered. Review spec to confirm.

3. **Messaging 11-20**: Higher-numbered messaging contracts likely cover advanced features (fragmentation, protocol edge cases). Lower priority unless bugs surface.

4. **Traceability limitation**: Current tooling maps each contract to FIRST matching test only. Multiple tests may cover same contract.
