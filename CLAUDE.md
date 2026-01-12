# CLAUDE.md

Naia is a cross-platform Rust networking engine for multiplayer games. Architecture follows the [Tribes 2 Networking model](https://www.gamedevs.org/uploads/tribes-networking-model.pdf).

## Current State (2026-01-11)

| Metric | Value |
|--------|-------|
| Contracts with compiling tests | **185/236 (78%)** |
| Tests with `todo!()` | **0** |
| Uncovered contracts | **51** |
| Phase A | **IN PROGRESS** |

**Goal:** Phase A - write compiling E2E tests for all 236 contracts.

**Recent:** Spec hardening added 51 new contracts (protocol_id, command sequence, error taxonomy). These need tests before Phase B can begin.

## Test File Organization (1:1 Mapping)

Test files map directly to spec files for instant traceability:

```
specs/contracts/N_domain.md  →  test/tests/0N_domain.rs
```

| Spec | Test File |
|------|-----------|
| `1_connection_lifecycle.md` | `01_connection_lifecycle.rs` |
| `2_transport.md` | `02_transport.rs` |
| `3_messaging.md` | `03_messaging.rs` |
| ... | ... |
| `14_world_integration.md` | `14_world_integration.rs` |

**To find tests for a contract:** Open the matching numbered test file

## Session Startup Protocol

**Execute this sequence at session start:**

```bash
# 1. Check current coverage
./specs/spec_tool.sh coverage

# 2. Check for blocked work
grep -r "todo!" test/tests/*.rs

# 3. Read PLAN.md for next actions
# 4. Begin work following SDD loop
```

## Spec-Driven Development (CRITICAL)

**Specs are the source of truth.** If implementation differs from spec, the implementation is wrong.

```
specs/contracts/*.md (contracts) → test/tests/*.rs (E2E tests) → Implementation
```

### Two-Phase Development Process

**Phase A: Complete Test Coverage (CURRENT PRIORITY)**
- Write compiling E2E tests for ALL spec contracts
- Tests MUST compile with NO `todo!()` macros
- Tests are allowed to FAIL - that indicates implementation gaps
- Goal: `spec_tool.sh coverage` shows 185/185 (100%)

**Phase B: Fix Implementation**
- Run all tests, observe failures
- Systematically fix implementation to make tests pass
- Failing tests are the bug tracker

**Key insight:** A `todo!()` in a test is a **specification gap**, not an implementation bug. Write what you *expect* to happen, and let the test fail if the implementation is wrong.

**The SDD Loop (Phase A):**
1. **SPEC**: Find contract `[contract-id]` in `specs/contracts/`
2. **TEST**: Write compiling test with `/// Contract: [contract-id]` annotation
3. **VERIFY**: Ensure test compiles (allowed to fail)
4. **VALID**: Run `./specs/spec_tool.sh coverage` to verify annotation

## Essential Commands

```bash
# Spec operations (run from project root)
./specs/spec_tool.sh lint           # Validate specs
./specs/spec_tool.sh coverage       # Check contract test coverage
./specs/spec_tool.sh traceability   # Generate contract↔test matrix
./specs/spec_tool.sh gen-test <id>  # Generate test skeleton

# Testing
cargo test --package naia-test                        # All E2E tests
cargo test --package naia-test --test <file>          # Single test file
cargo test --package naia-test <test_name>            # Single test
cargo test --package naia-test <test_name> -- --nocapture  # With output

# Debugging (detailed network event tracing)
cargo test --package naia-test --features e2e_debug <test_name> -- --nocapture

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
/// Contract: [domain-NN]
#[test]
fn contract_name_scenario() {
    let mut scenario = Scenario::new();
    scenario.server_start(ServerConfig::default(), protocol());

    // Setup: create room, connect clients
    let room_key = scenario.mutate(|ctx| ctx.server(|s| s.make_room().key()));
    let client_key = client_connect(&mut scenario, &room_key, "Client",
        Auth::new("user", "pass"), test_client_config(), protocol.clone());

    // Action
    scenario.mutate(|ctx| { /* trigger behavior */ });

    // Verify
    scenario.expect(|ctx| {
        let result = ctx.server(|s| /* check state */);
        result.then_some(())
    });
}
```

**Rules:**
- Must alternate `mutate()` ↔ `expect()`
- Use `test_client_config()` for all clients
- Multi-contract annotation: `/// Contract: [a-01], [b-02]`
- If a spec requires APIs not exposed in the harness, **IMPLEMENT them** in `test/src/harness/`

## Token Optimization (CRITICAL)

**DO:**
- Use Grep first, then targeted Read with offset/limit
- Run parallel tool calls for independent operations
- Run full test file, not individual tests: `cargo test --package naia-test --test 11_entity_authority`
- Reference existing test patterns; don't repeat code

**DON'T:**
- Read NAIA_SPECS.md (3000+ lines) - use grep for specific contracts
- Read full test files - grep to find function, then targeted read
- Generate tests without checking similar existing tests first

## Key References

| Document | Purpose | When to Read |
|----------|---------|--------------|
| `DEV_PROCESS.md` | Full SDD process, tooling, patterns | Complex tasks |
| `PLAN.md` | Current phase, next actions, blockers | Every session |
| `specs/generated/CONTRACT_REGISTRY.md` | All 185 contract IDs indexed | Finding contracts |
| `specs/generated/TRACEABILITY.md` | Contract↔test mapping | Checking coverage |
| `specs/generated/GAP_ANALYSIS.md` | Prioritized uncovered contracts | Planning work |

## Known Gaps

No known harness gaps. If a spec requires APIs not in the harness, implement them.

## Initiative Guidelines

**Act immediately:**
- Obvious bugs revealed by failing tests
- Missing contract annotations
- Spec typos or inconsistencies

**Ask first:**
- Changing spec semantics
- Adding new contracts
- Architectural harness changes
- Changes outside `test/` or `specs/`

## Workflow Quick Reference

**Phase A: Complete test coverage (CURRENT PRIORITY)**
1. Run `./specs/spec_tool.sh coverage`
2. Pick uncovered contract from output
3. Grep for similar tests: `grep -l "entity-authority" test/tests/*.rs`
4. Read similar test for pattern
5. Write COMPILING test with contract annotation (no `todo!()`)
6. Verify test compiles: `cargo test --package naia-test --test <file> --no-run`
7. Run coverage again to verify annotation
8. Test is allowed to FAIL - that's fine for Phase A

**Phase A: Eliminate todo!() macros**
1. Run `grep -r "todo!" test/tests/*.rs`
2. For each todo!(), write actual test assertions
3. Test must COMPILE (failures are OK)
4. The test failure message documents the implementation gap

**Phase B: Fix implementation (AFTER Phase A complete)**
1. Run `cargo test --package naia-test` to see all failures
2. Pick a failing test
3. Understand what the test expects (read the spec)
4. Fix implementation to match spec
5. Verify test passes
6. Run 3x for flakiness

## Debugging Tests

### e2e_debug Feature Flag

Enable detailed tracing of network events during test execution:

```bash
cargo test --package naia-test --features e2e_debug <test_name> -- --nocapture
```

**What it traces:**
- `[SERVER_SEND]` - Authority grants, delegation enable/disable commands
- `[CLIENT_RECV]` - Received delegation and authority state changes
- Entity IDs, authority status transitions, and caller locations

**Example output:**
```
[SERVER_SEND] EnableDelegation entity=GlobalEntity(42) callsite=send_enable_delegation(host)
[CLIENT_RECV] EnableDelegation entity=GlobalEntity(42)
[SERVER_SEND] SetAuthority entity=GlobalEntity(42) status=Granted
[CLIENT_RECV] SetAuthority entity=GlobalEntity(42) from_status=Available to_status=Granted
```

**When to use:**
- Test failures involving authority or delegation state
- Understanding message flow between server and clients
- Debugging entity visibility/scope issues

**Additional debug APIs (feature-gated):**
- `scenario.debug_dump_identity_state()` - Dump entity state across server and clients
- `expect_ctx.scenario()` - Access scenario from expect context

## Constraints

- Demos excluded from workspace (feature conflicts)
- Wasm: use `wbindgen` OR `mquad`, not both
- All tick math must be wrap-safe (u16 wraps at 65535)
- Specs use RFC 2119: MUST/MUST NOT/MAY/SHOULD/SHALL
- No timing hacks in tests - use `expect()` polling pattern
