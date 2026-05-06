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
| `03_messaging.rs`              | 3 failing (protocol mismatch fast-fail, TickBuffered too-far-ahead, EntityProperty cap FIFO) |
| `06_entity_scopes.rs`          | 2 failing + 1 `#[ignore]` (publish/unpublish vs spawn semantics, scope leave vs despawn distinguishability) |
| `10_entity_delegation.rs`      | (no failures — see closed entries below) |

### Closed since carve-out was created
- `auth_denied_emitted_exactly_once_per_transition_into_denied` (2026-05-06): client missed an `Available → Denied` push for `EntityAuthDeniedEvent`. Fixed in `client/src/client.rs`. Behaviour now covered by namako Scenario `[entity-delegation-16] AuthDenied event fires on Available→Denied transition` in `test/specs/features/05_authority.feature`.
- `migration_yields_no_holder_if_owner_out_of_scope` (2026-05-06): `enable_delegation_client_owned_entity` was overwriting the former owner's explicit scope-exclude with `true` immediately before the holder-assignment check, silently granting authority to a user who had been excluded. Fixed in `server/src/server/world_server.rs` (only initialize the entry when not already explicit). Behaviour now covered by namako Scenario `[entity-delegation-09] Migration yields no holder if owner out of scope`.
- `re_entering_scope_yields_correct_current_auth_status` (2026-05-06): when a client re-entered scope on a delegated entity that already had a holder, the server sent `EnableDelegation` (which defaults to `Available` client-side) but never followed up with `SetAuthority(Denied)` — so the freshly-included client silently observed the wrong status. Fixed in `server/src/server/world_server.rs::apply_scope_for_user` (fan out the current holder's status after re-entry); new `entity_has_holder` accessor on `GlobalWorldManager` / `ServerAuthHandler`. Behaviour now covered by namako Scenario `[entity-delegation-15] Re-entering scope yields current authority status`.

`_helpers.rs` is shared scaffolding kept until the last carve-out file
disappears.
