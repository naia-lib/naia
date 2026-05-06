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

## Migration deletion criteria

A test in this directory is deleted (and its namako stub upgraded to a
real `@Scenario` exercising the same `[contract-id]`) once:

- The product gap it documents is fixed (test passes), AND
- A `Scenario:` in the matching `test/specs/features/*.feature` file
  exercises the same observable behaviour the Rust test was asserting.

When all five files in this directory are gone, delete this README and
the `contract_tests/` parent directory.

## Current contents (2026-05-06)

| File                           | Status                                  |
|--------------------------------|-----------------------------------------|
| `00_common.rs`                 | 2 `#[ignore]` policy-stamp tests        |
| `01_connection_lifecycle.rs`   | 4 `#[ignore]` (capacity/heartbeat/token)|
| `03_messaging.rs`              | 2 `#[ignore]` deferred (protocol mismatch fast-fail, TickBuffered too-far-ahead) + 1 `#[ignore]` product-gap (EntityProperty cap FIFO — messaging-20) |
| `06_entity_scopes.rs`          | 1 `#[ignore]` (scope_change_queue_drains_within_n_ticks) |
| `10_entity_delegation.rs`      | (no failures — see closed entries below) |

### Closed since carve-out was created
- `auth_denied_emitted_exactly_once_per_transition_into_denied` (2026-05-06): client missed an `Available → Denied` push for `EntityAuthDeniedEvent`. Fixed in `client/src/client.rs`. Behaviour now covered by namako Scenario `[entity-delegation-16] AuthDenied event fires on Available→Denied transition` in `test/specs/features/05_authority.feature`.
- `migration_yields_no_holder_if_owner_out_of_scope` (2026-05-06): `enable_delegation_client_owned_entity` was overwriting the former owner's explicit scope-exclude with `true` immediately before the holder-assignment check, silently granting authority to a user who had been excluded. Fixed in `server/src/server/world_server.rs` (only initialize the entry when not already explicit). Behaviour now covered by namako Scenario `[entity-delegation-09] Migration yields no holder if owner out of scope`.
- `re_entering_scope_yields_correct_current_auth_status` (2026-05-06): when a client re-entered scope on a delegated entity that already had a holder, the server sent `EnableDelegation` (which defaults to `Available` client-side) but never followed up with `SetAuthority(Denied)` — so the freshly-included client silently observed the wrong status. Fixed in `server/src/server/world_server.rs::apply_scope_for_user` (fan out the current holder's status after re-entry); new `entity_has_holder` accessor on `GlobalWorldManager` / `ServerAuthHandler`. Behaviour now covered by namako Scenario `[entity-delegation-15] Re-entering scope yields current authority status`.
- `publish_unpublish_vs_spawn_despawn_semantics_distinct` (2026-05-06): product was correct; harness bug caused `ServerExpectCtx::has_entity` to return `true` after despawn (registry-only check, never cleaned up). Fixed in `test/harness/src/harness/server_expect_ctx.rs` (also check `server_world_ref().has_entity`). Behaviour now covered by namako Scenario `[entity-scopes-08] Room entry and exit control client visibility lifecycle` in `test/specs/features/04_visibility.feature`.
- `leaving_scope_vs_despawn_distinguishable` (2026-05-06): same harness bug (`has_entity` returned `true` after despawn). Fixed by the same `server_expect_ctx.rs` change. Behaviour now covered by namako Scenario `[entity-scopes-15] Scope leave is reversible; true despawn is permanent` in `test/specs/features/04_visibility.feature`.

### Deferred (product gap — no unpark plan yet)
- `messaging_20_entity_property_buffer_caps` (`03_messaging.rs`): tests a 128-message FIFO eviction cap in `RemoteEntityWaitlist` that does not exist. `RemoteEntityWaitlist` only has a 60-second TTL. Test premise also incorrect (entity enters room during spawn so it is immediately in scope; messages would never be buffered). Requires: implement per-entity FIFO cap + fix test setup + write namako Scenario for `[messaging-20]`.

`_helpers.rs` is shared scaffolding kept until the last carve-out file
disappears.
