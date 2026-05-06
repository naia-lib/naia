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
| `06_entity_scopes.rs`          | 3 failing + 1 `#[ignore]` (publish/unpublish vs spawn semantics, scope leave vs despawn distinguishability, re-entry auth status) |
| `10_entity_delegation.rs`      | 1 failing (migration with out-of-scope owner) |

### Closed since carve-out was created
- `auth_denied_emitted_exactly_once_per_transition_into_denied` (2026-05-06): client missed an `Available → Denied` push for `EntityAuthDeniedEvent`. Fixed in `client/src/client.rs`. Behaviour now covered by namako Scenario `[entity-delegation-16] AuthDenied event fires on Available→Denied transition` in `test/specs/features/05_authority.feature`.

`_helpers.rs` is shared scaffolding kept until the last carve-out file
disappears.
