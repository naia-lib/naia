# Integration-only contract carve-out

This directory exists because a small slice of legacy contract tests
could not migrate cleanly to namako's Gherkin Scenarios in
`test/specs/features/`. They survive here as Rust integration tests so
their regression coverage isn't lost when `legacy_tests/` was retired.

## Why a test lives here

Two reasons only:

1. **Known product gap.** The test fails today because the underlying
   product behaviour is incomplete or buggy. Keeping the test makes the
   gap visible (the namako Scenario for the same contract ID is a
   `@Deferred @PolicyOnly` stub). When the product is fixed, the test
   migrates to a namako Scenario and is deleted from here.
2. **Infrastructure placeholder.** The test is `#[ignore]`-ed because
   exercising it requires harness machinery (time manipulation, server
   capacity config, server-generated identity tokens, etc.) that hasn't
   been wired up yet. Same migration path: build the harness piece,
   convert to a Scenario, delete the Rust test.

If a test doesn't fit either category it does NOT belong here — write
it as a Gherkin Scenario.

## Relationship to BDD scenarios

These integration tests are the **correctness oracle**. BDD scenarios in
`test/specs/features/` are behavioral specifications derived from them.

**Migration rule:** Do not delete an integration test until its BDD scenario
has passing green runs. The two layers are complements, not competitors:
integration tests prove the runtime is correct, BDD scenarios prove the
contract is observable and documented.

## Migration deletion criteria

A test in this directory is deleted (and its namako stub upgraded to a
real `@Scenario` exercising the same `[contract-id]`) once:

- The product gap it documents is fixed (test passes), AND
- A `Scenario:` in the matching `test/specs/features/*.feature` file
  exercises the same observable behaviour the Rust test was asserting.

When all seventeen test files in this directory are gone, delete this
README and the `contract_tests/` parent directory (`_helpers.rs` is shared
scaffolding kept until the last carve-out file disappears).

## Current contents (2026-09-30)

Counts measured with `grep -c '#\[ignore'` / `grep -c '#\[test\]'` per file
plus the matching `cargo test -p naia-test-harness --test <target>` runs:
every file is at 0 `#[ignore]`, 171 live tests total, all passing.

| File                           | `#[ignore]` | Live `#[test]`s |
|--------------------------------|-------------|-----------------|
| `00_common.rs`                 | 0 | 15 |
| `01_connection_lifecycle.rs`   | 0 | 23 |
| `02_static_entities.rs`        | 0 | 2 |
| `03_messaging.rs`              | 0 | 40 |
| `06_entity_scopes.rs`          | 0 | 26 |
| `09_entity_ownership.rs`       | 0 | 3 |
| `10_entity_delegation.rs`      | 0 | 19 |
| `11_cross_feature_delegation.rs` | 0 | 2 |
| `12_reconnect_cancel.rs`       | 0 | 5 |
| `g8_real_ack_byte_identity.rs` | 0 | 1 |
| `g9pre_resident_pipelined_byte_identity.rs` | 0 | 10 |
| `lagged_send_drops_final_value.rs` | 0 | 1 |
| `parallel_send_matches_serial.rs` | 0 | 1 |
| `pipeline_bracket.rs`          | 0 | 3 |
| `pipeline_recv_send_independent.rs` | 0 | 7 |
| `router_basic.rs`              | 0 | 3 |
| `world_server_enum_shell.rs`   | 0 | 10 |

- `api_misuse_returns_error_not_panic` (2026-05-06): `give_authority` on a client that is not in scope of a delegated entity returns `Err(NotInScope)` rather than panicking. Product behaviour was already correct; test was `#[ignore]`-ed as an infra placeholder. Converted to namako Scenario `[common-01] give_authority on out-of-scope client returns Err(NotInScope)` in `test/specs/features/00_foundations.feature` Rule(01):Scenario(03).
- `private_replication_only_owner_sees_it` (2026-05-06): When client A spawns an entity with `ClientReplicationConfig::Private`, client B (in the same room) must NOT see the entity. Product behaviour was already correct; test was `#[ignore]`-ed as an infra placeholder. Converted to namako Scenario `[entity-ownership-12] Private client-owned entity stays with owner only` in `test/specs/features/05_authority.feature` Rule(05):Scenario(07).
- `protocol_mismatch_is_deployment_error_not_panic` (2026-05-06): Protocol-id mismatch between server and client must produce a `ProtocolMismatch` rejection, not panic. Product behaviour was correct; test was `#[ignore]`-ed because it used a different protocol-building path. The two Rule(03) Scenarios in `00_foundations.feature` (`[common-02a]` — mismatch produces rejection, and mismatch does not establish connection) already cover this with real `ProtocolId::new(A/B)` step bindings. Rust test deleted.

### Closed since carve-out was created
- `auth_denied_emitted_exactly_once_per_transition_into_denied` (2026-05-06): client missed an `Available → Denied` push for `EntityAuthDeniedEvent`. Fixed in `client/src/client.rs`. Behaviour now covered by namako Scenario `[entity-delegation-16] AuthDenied event fires on Available→Denied transition` in `test/specs/features/05_authority.feature`.
- `migration_yields_no_holder_if_owner_out_of_scope` (2026-05-06): `enable_delegation_client_owned_entity` was overwriting the former owner's explicit scope-exclude with `true` immediately before the holder-assignment check, silently granting authority to a user who had been excluded. Fixed in `server/src/server/world_server.rs` (only initialize the entry when not already explicit). Behaviour now covered by namako Scenario `[entity-delegation-09] Migration yields no holder if owner out of scope`.
- `re_entering_scope_yields_correct_current_auth_status` (2026-05-06): when a client re-entered scope on a delegated entity that already had a holder, the server sent `EnableDelegation` (which defaults to `Available` client-side) but never followed up with `SetAuthority(Denied)` — so the freshly-included client silently observed the wrong status. Fixed in `server/src/server/world_server.rs::apply_scope_for_user` (fan out the current holder's status after re-entry); new `entity_has_holder` accessor on `GlobalWorldManager` / `ServerAuthHandler`. Behaviour now covered by namako Scenario `[entity-delegation-15] Re-entering scope yields current authority status`.
- `publish_unpublish_vs_spawn_despawn_semantics_distinct` (2026-05-06): product was correct; harness bug caused `ServerExpectCtx::has_entity` to return `true` after despawn (registry-only check, never cleaned up). Fixed in `test/harness/src/harness/server_expect_ctx.rs` (also check `server_world_ref().has_entity`). Behaviour now covered by namako Scenario `[entity-scopes-08] Room entry and exit control client visibility lifecycle` in `test/specs/features/04_visibility.feature`.
- `leaving_scope_vs_despawn_distinguishable` (2026-05-06): same harness bug (`has_entity` returned `true` after despawn). Fixed by the same `server_expect_ctx.rs` change. Behaviour now covered by namako Scenario `[entity-scopes-15] Scope leave is reversible; true despawn is permanent` in `test/specs/features/04_visibility.feature`.

### Deferred — empty (2026-09-30)
- `messaging_20_entity_property_buffer_caps` (`03_messaging.rs`): CLOSED 2026-09-30. The 128-message per-entity FIFO cap now exists and the test is live and passing (`integration_only_03_messaging` 40/40, 0 ignored): 130 `EntityCommandMessage`s queue for an out-of-scope entity and exactly the newest 128 deliver on scope entry. The TickBuffered too-far-ahead test (`tick_buffered_channel_discards_too_far_ahead_ticks`) is likewise live in the same passing run — no tick-injection primitive was needed after all.
