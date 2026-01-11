# CLAUDE.md

Naia is a cross-platform Rust networking engine for multiplayer games. Architecture follows the [Tribes 2 Networking model](https://www.gamedevs.org/uploads/tribes-networking-model.pdf).

## Spec-Driven Development (CRITICAL)

**Specs are the source of truth.** If implementation differs from spec, the implementation is wrong.

```
specs/*.md (contracts) → test/tests/*.rs (E2E tests) → Implementation
```

**The SDD Loop:**
1. **SPEC**: Define/find contract `[contract-id]` in `specs/`
2. **TEST**: Write test with `/// Contract: [contract-id]` annotation
3. **IMPL**: Make test pass with minimal code changes
4. **VALID**: Update spec's Test Obligations section

## Essential Commands

```bash
# Spec operations (run from specs/)
./spec_tool.sh lint           # Validate specs (MUST pass before commits)
./spec_tool.sh coverage       # Check contract test coverage
./spec_tool.sh gen-test <id>  # Generate test skeleton for contract

# Testing
cargo test --package naia-test              # E2E tests
cargo test --package naia-test <test_name>  # Single test

# Quality gates
cargo clippy --no-deps && cargo fmt -- --check
```

## Crate Map

| Crate | Purpose |
|-------|---------|
| `shared/` | Core abstractions, Protocol, serde |
| `client/` | Client networking |
| `server/` | Server networking |
| `socket/` | Transport layer (WebRTC) |
| `test/` | E2E harness + tests |
| `adapters/` | Bevy/hecs integrations |

## Test Harness Pattern

```rust
/// Contract: [entity-scopes-07]
#[test]
fn entity_scopes_07_scenario_name() {
    let mut scenario = Scenario::new();
    scenario.server_start(ServerConfig::default(), protocol());

    scenario.mutate(|ctx| { /* setup/action */ });
    scenario.expect(|ctx| { /* verify */ Some(()) });
}
```

**Rules:** Must alternate `mutate()` ↔ `expect()`. Use `test_client_config()` for clients.

## Key References

| Document                                | Purpose |
|-----------------------------------------|---------|
| `specs/NAIA_SPEC_DRIVEN_DEVELOPMENT.md` | Full SDD process, tooling, workflows |
| `specs/CONTRACT_REGISTRY.md`            | All 185 contract IDs indexed |
| `specs/NAIA_SPECS.md`                   | Bundled specifications |
| `specs/0_README.md`                     | Master glossary |

## Cost Optimization

**DO:**
- Use `grep`/`sed` for multi-file changes over individual edits
- Read specific line ranges (`offset`/`limit`) not full files
- Run parallel tool calls when operations are independent
- Reference existing patterns; don't repeat code in prompts

**DON'T:**
- Read NAIA_SPECS.md (3000+ lines) unless searching for specific contract
- Generate tests without checking existing patterns in `test/tests/`
- Over-engineer; implement minimal code to pass tests

## Workflow Quick Reference

**New feature:** Read spec → `gen-test` → implement test → implement code → validate
**Bug fix:** Find contract → check/write test → fix code → verify
**Spec work:** Edit `specs/*.md` → `lint` → `bundle` → `registry`

## Constraints

- Demos excluded from workspace (feature conflicts)
- Wasm: use `wbindgen` OR `mquad`, not both
- All tick math must be wrap-safe (u16 wraps at 65535)
- Specs use RFC 2119: MUST/MUST NOT/MAY/SHOULD/SHALL
