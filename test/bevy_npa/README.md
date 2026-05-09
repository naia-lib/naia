# bevy_npa — Bevy adapter BDD verification

`bevy_npa` is the NPA-based BDD test runner for the Bevy adapter
(`naia-bevy-server` / `naia-bevy-client`).

## Relationship to naia_npa

The main `naia_npa` runner tests the raw `naia-server` / `naia-client` APIs
through the deterministic tick harness in `test/harness/`.  `bevy_npa` mirrors
that architecture but drives the Bevy ECS world instead:

| | `naia_npa` | `bevy_npa` |
|---|---|---|
| World | `naia-demo-world` tick harness | Bevy `App` |
| Entity API | `Server::spawn_entity` | `CommandsExt::enable_replication` |
| Step bindings | `test/tests/src/steps/` | `test/bevy_npa/src/steps.rs` |
| Specs | `test/specs/features/` | `test/bevy_specs/features/` |

## Current state

- **9 scenarios** in `bevy_specs/features/smoke.feature` covering
  connection lifecycle and event ordering.
- Step bindings in `steps.rs` cover connect / disconnect / event-count
  assertions only.
- No entity replication, authority, or resource scenarios yet (tracked in
  `TEST_INFRA_PLAN.md` T4).

## How to run

```sh
# Run all Bevy BDD specs
cargo run --manifest-path test/bevy_npa/Cargo.toml -- run \
  --specs-root test/bevy_specs

# Coverage summary
cargo run --manifest-path test/bevy_npa/Cargo.toml -- coverage \
  --specs-root test/bevy_specs

# Run a single scenario by key
cargo run --manifest-path test/bevy_npa/Cargo.toml -- run \
  --specs-root test/bevy_specs --key smoke-01
```
