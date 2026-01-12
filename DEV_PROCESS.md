# Naia Development Process

**Version:** 2.0
**Updated:** 2026-01-11
**Purpose:** Complete guide for spec-driven development of Naia using Claude Code

---

## Table of Contents

1. [Philosophy](#1-philosophy)
2. [The SDD Loop](#2-the-sdd-loop)
3. [Test Harness Patterns](#3-test-harness-patterns)
4. [Tooling Reference](#4-tooling-reference)
5. [Token Optimization](#5-token-optimization)
6. [Workflow Templates](#6-workflow-templates)
7. [Quality Gates](#7-quality-gates)
8. [Troubleshooting](#8-troubleshooting)
9. [Appendices](#appendices)

---

## 1. Philosophy

### 1.1 Specs Are Truth

**If implementation differs from spec, the implementation is wrong.**

The specification documents in `specs/contracts/*.md` define the correct behavior of Naia. When a test fails, it usually means the implementation has a bug, not that the spec is wrong.

### 1.2 The Flywheel

```
Contracts → Tests → Implementation → Validation
    ↑                                      ↓
    └──────── Spec Refinements ←──────────┘
```

Each cycle:
1. Improves both specs and implementation
2. Catches bugs before they ship
3. Creates documentation as a side effect

### 1.3 Minimal Diffs Win

- Prefer Edit over Write
- Prefer adding annotation over adding test (if test exists)
- Prefer simple assertion over complex scenario
- Only implement what the contract requires

---

## 2. The SDD Loop

### 2.1 Overview

```
┌──────────┐     ┌──────────┐     ┌──────────┐     ┌──────────┐
│  1.SPEC  │────▶│  2.TEST  │────▶│  3.IMPL  │────▶│  4.VALID │
│  Define  │     │  Write   │     │  Build   │     │  Verify  │
└──────────┘     └──────────┘     └──────────┘     └──────────┘
      ▲                                                  │
      └──────────────────────────────────────────────────┘
```

### 2.2 Phase 1: SPEC (Find or Define Contract)

**Goal:** Identify the contract that defines the behavior.

**Steps:**
1. Search for existing contract:
   ```bash
   grep -n "entity-authority" specs/contracts/*.md
   ```
2. If contract exists, read its definition
3. If new behavior needed, draft contract:
   ```markdown
   ### [domain-NN] — Short Title

   **Guarantee:** Single sentence with RFC 2119 keywords.

   **Preconditions:**
   - Condition A MUST be true

   **Postconditions:**
   - Effect X MUST occur
   ```
4. Validate: `./specs/spec_tool.sh lint`

### 2.3 Phase 2: TEST (Write E2E Test)

**Goal:** Create a test that verifies the contract.

**Steps:**
1. Find similar existing tests:
   ```bash
   grep -l "entity-authority" test/tests/*.rs
   ```
2. Read similar test for pattern (targeted read, not full file)
3. Write test with contract annotation:
   ```rust
   /// Contract: [entity-authority-NN]
   #[test]
   fn contract_name_scenario() {
       let mut scenario = Scenario::new();
       // ...
   }
   ```
4. Run test (should fail or pass depending on impl state):
   ```bash
   cargo test --package naia-test contract_name_scenario
   ```

### 2.4 Phase 3: IMPL (Make Test Pass)

**Goal:** Implement or fix code to satisfy the contract.

**Steps:**
1. If test fails, identify the implementation gap
2. Make minimal changes to the relevant crate:
   - `shared/` for protocol-level changes
   - `client/` for client-side behavior
   - `server/` for server-side behavior
3. Run test until it passes
4. Run quality checks:
   ```bash
   cargo clippy --no-deps
   cargo fmt -- --check
   ```

### 2.5 Phase 4: VALID (Verify Coverage)

**Goal:** Confirm the contract is now covered.

**Steps:**
1. Run coverage check:
   ```bash
   ./specs/spec_tool.sh coverage
   ```
2. Verify the contract ID appears in "covered" output
3. Run test 3x for flakiness check:
   ```bash
   cargo test --package naia-test contract_name_scenario
   cargo test --package naia-test contract_name_scenario
   cargo test --package naia-test contract_name_scenario
   ```

---

## 3. Test Harness Patterns

### 3.1 Basic Test Structure

```rust
/// Contract: [domain-NN]
#[test]
fn contract_name_scenario() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    // Start server
    scenario.server_start(ServerConfig::default(), test_protocol.clone());

    // Create room
    let room_key = scenario.mutate(|ctx| {
        ctx.server(|server| server.make_room().key())
    });

    // Connect client
    let client_key = client_connect(
        &mut scenario,
        &room_key,
        "Client A",
        Auth::new("user", "pass"),
        test_client_config(),
        test_protocol.clone(),
    );

    // Action
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            // Do something
        });
    });

    // Verification
    scenario.expect(|ctx| {
        let result = ctx.client(client_key, |client| {
            // Check state
        });
        result.then_some(())
    });
}
```

### 3.2 Multi-Client Pattern

```rust
let client_a_key = client_connect(&mut scenario, &room_key, "A", auth_a, config(), proto.clone());
let client_b_key = client_connect(&mut scenario, &room_key, "B", auth_b, config(), proto.clone());

// Wait for both to see entity
scenario.expect(|ctx| {
    let a_sees = ctx.client(client_a_key, |c| c.has_entity(&entity));
    let b_sees = ctx.client(client_b_key, |c| c.has_entity(&entity));
    (a_sees && b_sees).then_some(())
});
```

### 3.3 Authority Setup Pattern

```rust
// 1. Spawn entity
let entity = scenario.mutate(|ctx| {
    ctx.server(|server| {
        let (entity, _) = server.spawn(|mut e| {
            e.insert_component(Position::new(0.0, 0.0));
            e.enter_room(&room_key);
        });
        server.user_scope_mut(&client_key).unwrap().include(&entity);
        entity
    })
});

// 2. Wait for replication
scenario.expect(|ctx| {
    ctx.client(client_key, |c| c.has_entity(&entity)).then_some(())
});

// 3. Enable delegation
scenario.mutate(|ctx| {
    ctx.server(|server| {
        if let Some(mut e) = server.entity_mut(&entity) {
            e.configure_replication(ReplicationConfig::Delegated);
        }
    });
});

// 4. Wait for Available status
scenario.expect(|ctx| {
    use naia_shared::EntityAuthStatus;
    let status = ctx.client(client_key, |c| {
        c.entity(&entity).and_then(|e| e.authority())
    });
    (status == Some(EntityAuthStatus::Available)).then_some(())
});

// 5. Give/request authority
scenario.mutate(|ctx| {
    ctx.server(|server| {
        if let Some(mut e) = server.entity_mut(&entity) {
            e.give_authority(&client_key).unwrap();
        }
    });
});
```

### 3.4 Multi-Contract Annotation

When a single test covers multiple contracts:

```rust
/// Contract: [entity-delegation-01]
/// Contract: [entity-delegation-02]
/// Contract: [entity-authority-13]
#[test]
fn delegation_enables_authority_semantics() {
    // ...
}
```

### 3.5 Blocked Test Pattern

When a test is blocked by an implementation bug:

```rust
/// Contract: [entity-authority-11]
/// Contract: [entity-authority-12]
///
/// TODO: Blocked by implementation bug in remote_world_manager.rs
/// Error: EntityDoesNotExistError when authority holder goes out of scope
#[test]
fn out_of_scope_ends_authority_for_that_client() {
    todo!("Blocked by implementation bug: authority holder going out of scope panics")
}
```

---

## 4. Tooling Reference

### 4.1 spec_tool.sh Commands

| Command | Purpose | When to Use |
|---------|---------|-------------|
| `./specs/spec_tool.sh coverage` | Show covered/uncovered contracts | Every session start |
| `./specs/spec_tool.sh traceability` | Generate contract↔test matrix | After adding tests |
| `./specs/spec_tool.sh lint` | Validate spec format | After editing specs |
| `./specs/spec_tool.sh gen-test <id>` | Generate test skeleton | Starting new test |
| `./specs/spec_tool.sh bundle` | Generate NAIA_SPECS.md | After spec changes |
| `./specs/spec_tool.sh registry` | Generate CONTRACT_REGISTRY.md | After adding contracts |
| `./specs/spec_tool.sh check-orphans` | Find untracked MUSTs | Spec cleanup |

### 4.2 Cargo Test Commands

```bash
# Run all E2E tests
cargo test --package naia-test

# Run single test file
cargo test --package naia-test --test entity_authority_server_ops

# Run single test
cargo test --package naia-test out_of_scope_ends_authority

# Run with output
cargo test --package naia-test out_of_scope -- --nocapture

# Run workspace tests
cargo test --workspace
```

### 4.3 Quality Check Commands

```bash
# Lint
cargo clippy --no-deps

# Format check
cargo fmt -- --check

# Full quality gate
cargo clippy --no-deps && cargo fmt -- --check
```

### 4.4 Debugging with e2e_debug

The `e2e_debug` feature enables detailed tracing of network events during test execution.

**Enable:**
```bash
cargo test --package naia-test --features e2e_debug <test_name> -- --nocapture
```

**What it traces:**
- `[SERVER_SEND]` - Authority grants, delegation enable/disable commands with entity IDs
- `[CLIENT_RECV]` - Received delegation and authority state changes with state transitions
- Caller locations for debugging which code path triggered events

**Example output:**
```
[SERVER_SEND] EnableDelegation entity=GlobalEntity(42) callsite=send_enable_delegation(host)
[CLIENT_RECV] EnableDelegation entity=GlobalEntity(42)
[SERVER_SEND] SetAuthority entity=GlobalEntity(42) status=Granted
[CLIENT_RECV] SetAuthority entity=GlobalEntity(42) from_status=Available to_status=Granted
[SERVER_SEND] DisableDelegation entity=GlobalEntity(42) caller=shared/src/world/local/local_world_manager.rs:826
```

**When to use:**
- Test failures involving authority or delegation state machines
- Understanding message flow and ordering between server and clients
- Debugging entity visibility/scope issues
- Investigating state transition problems

**Feature-gated debug APIs:**
When `e2e_debug` is enabled, additional APIs are available in the test harness:

```rust
// Dump entity state across server and all clients
#[cfg(feature = "e2e_debug")]
scenario.debug_dump_identity_state("After authority grant", &entity_key, &[client_a, client_b]);

// Access scenario from expect context for debugging
#[cfg(feature = "e2e_debug")]
let scenario = expect_ctx.scenario();
```

---

## 5. Token Optimization

### 5.1 Efficient File Access

**DO:**
```
# Grep first to find location
grep -n "out_of_scope_ends_authority" test/tests/*.rs

# Then targeted read
Read file with offset=220, limit=50
```

**DON'T:**
```
# Read entire 1000-line test file
Read entire file

# Read 3000-line NAIA_SPECS.md
Read specs/generated/NAIA_SPECS.md
```

### 5.2 Parallel Operations

**DO:**
```
# Multiple independent reads in one message
Read file A, Read file B, Read file C (parallel)
```

**DON'T:**
```
# Sequential reads when parallel works
Read file A
[wait]
Read file B
[wait]
Read file C
```

### 5.3 Batch Test Runs

**DO:**
```bash
# Run entire test file
cargo test --package naia-test --test entity_authority_server_ops
```

**DON'T:**
```bash
# Run tests one by one
cargo test --package naia-test test_1
cargo test --package naia-test test_2
cargo test --package naia-test test_3
```

### 5.4 Token Budget Reference

| Operation | Approximate Tokens |
|-----------|-------------------|
| Read 100-line file | ~500 |
| Read 1000-line file | ~5000 |
| Grep search | ~200 |
| Write file | ~300 + content |
| Edit file | ~200 + changes |

**Target:** 80% of reads under 200 lines.

---

## 6. Workflow Templates

### 6.1 Coverage Improvement

```
1. ./specs/spec_tool.sh coverage
2. Pick uncovered contract from output
3. grep -l "<domain>" test/tests/*.rs
4. Read similar test (targeted)
5. Write test with annotation
6. cargo test --package naia-test <test_name>
7. Fix if needed, run 3x for flakiness
8. ./specs/spec_tool.sh coverage (verify)
```

### 6.2 Bug Fix (Implementation)

```
1. Identify contract from spec
2. grep for contract ID in tests
3. If test exists and passes: spec might be wrong (investigate)
4. If test exists and fails: fix implementation
5. If no test: write one that reproduces bug
6. Fix implementation
7. Verify test passes
8. Run 3x for flakiness
```

### 6.3 New Feature

```
1. Draft contracts in spec file
2. ./specs/spec_tool.sh lint
3. ./specs/spec_tool.sh gen-test <contract-id>
4. Implement test (should fail)
5. Implement feature code
6. Verify test passes
7. ./specs/spec_tool.sh coverage
```

### 6.4 Unblock todo!() Test

```
1. Read the todo!() message for context
2. Read the implementation location mentioned
3. Understand the bug
4. Implement fix (minimal)
5. Remove todo!() from test
6. Implement actual test body
7. Run test
8. Run 3x for flakiness
```

---

## 7. Quality Gates

### 7.1 Test Quality

Every test MUST:
- [ ] Have `/// Contract: [id]` annotation
- [ ] Use Scenario harness
- [ ] Alternate mutate() ↔ expect()
- [ ] Use test_client_config() for clients
- [ ] Pass 3x without flakiness
- [ ] Not use timing hacks (sleep, delay)

### 7.2 Spec Quality

Every spec change MUST:
- [ ] Pass `./specs/spec_tool.sh lint`
- [ ] Have valid cross-references
- [ ] Use RFC 2119 keywords correctly

### 7.3 Implementation Quality

Every code change MUST:
- [ ] Pass `cargo clippy --no-deps`
- [ ] Pass `cargo fmt -- --check`
- [ ] Not regress existing tests
- [ ] Be minimal (no over-engineering)

---

## 8. Troubleshooting

### 8.1 Test Fails Unexpectedly

**Check order:**
1. Is the spec correct? (Usually yes)
2. Is the test correct? (Check against spec)
3. Is the implementation wrong? (Usually yes)

**Common causes:**
- Message ordering assumptions
- Missing wait for replication
- Wrong client/entity reference

### 8.2 Test Passes But Shouldn't Cover Contract

**Check:**
1. Does test have correct annotation? `/// Contract: [id]`
2. Does test actually verify the contract's guarantee?
3. Is assertion strong enough?

### 8.3 Coverage Not Updating

**Check:**
1. Did you add the annotation correctly?
2. Run `./specs/spec_tool.sh coverage` (not cached)
3. Is the contract ID spelled correctly?

### 8.4 Implementation Panic

**Example:** `EntityDoesNotExistError`

**Debug steps:**
1. Read the panic location
2. Understand the code path
3. Check for missing existence checks
4. Check for incorrect cleanup order
5. Implement fix

### 8.5 Debugging with e2e_debug Tracing

When a test fails and the cause is unclear, enable detailed tracing:

```bash
cargo test --package naia-test --features e2e_debug <test_name> -- --nocapture
```

This shows network events like `[SERVER_SEND]` and `[CLIENT_RECV]` with entity IDs, authority states, and caller locations. See section 4.4 for full details.

---

## Appendices

### Appendix A: Contract ID Prefixes

| Prefix | Domain | Spec File |
|--------|--------|-----------|
| `connection-` | Connection lifecycle | 2_connection_lifecycle.md |
| `transport-` | Transport layer | 3_transport.md |
| `messaging-` | Message passing | 4_messaging.md |
| `time-` | Time/tick semantics | 5_time_ticks_commands.md |
| `commands-` | Command buffer | 5_time_ticks_commands.md |
| `observability-` | Metrics | 6_observability_metrics.md |
| `entity-scopes-` | Scope predicates | 7_entity_scopes.md |
| `entity-replication-` | Replication protocol | 8_entity_replication.md |
| `entity-ownership-` | Ownership rules | 9_entity_ownership.md |
| `entity-publication-` | Publication state | 10_entity_publication.md |
| `entity-delegation-` | Delegation mechanics | 11_entity_delegation.md |
| `entity-authority-` | Authority state machine | 12_entity_authority.md |
| `server-events-` | Server event API | 13_server_events_api.md |
| `client-events-` | Client event API | 14_client_events_api.md |
| `world-integration-` | ECS integration | 15_world_integration.md |

### Appendix B: Test File Map

| Domain | Test File |
|--------|-----------|
| connection-* | `connection_auth_identity.rs` |
| entity-authority-* | `entity_authority_server_ops.rs`, `entity_authority_client_ops.rs` |
| entity-delegation-* | `entity_delegation_toggle.rs`, `entity_migration_and_events.rs` |
| entity-ownership-* | `entity_client_owned.rs` |
| entity-publication-* | `entity_client_owned.rs` |
| entity-replication-* | `entities_lifetime_identity.rs` |
| entity-scopes-* | `rooms_scope_snapshot.rs`, `entity_scope_coupling.rs` |
| messaging-* | `messaging_channels.rs`, `protocol_schema_versioning.rs` |
| server-events-*, client-events-*, world-integration-* | `events_world_integration.rs` |
| time-*, commands-*, transport-* | `time_ticks_transport.rs`, `integration_transport_parity.rs` |

### Appendix C: Imports Reference

```rust
use naia_client::ClientConfig;
use naia_server::{ServerConfig, ReplicationConfig};
use naia_shared::{Protocol, EntityAuthStatus};
use naia_test::{
    protocol, Auth, Position, Scenario,
    ClientEntityAuthGrantedEvent, ClientEntityAuthDeniedEvent, ClientEntityAuthResetEvent,
    ClientKey,
};
use test_helpers::{test_client_config, client_connect};
```

### Appendix D: Session Checklist

**Start of session:**
```
[ ] Read CLAUDE.md (if unfamiliar)
[ ] Read PLAN.md (always)
[ ] Run ./specs/spec_tool.sh coverage
[ ] Check grep -r "todo!" test/tests/*.rs
[ ] Identify next action
```

**During work:**
```
[ ] Use grep before read
[ ] Use parallel tool calls
[ ] Run test file, not individual tests
[ ] Run 3x for flakiness
```

**End of session:**
```
[ ] Run ./specs/spec_tool.sh coverage
[ ] Update PLAN.md if state changed
[ ] Run ./specs/spec_tool.sh traceability if tests added
```

### Appendix E: Known Issues

**entity-authority-11/12 Bug:**
- Location: `shared/src/world/remote/remote_world_manager.rs:146`
- Error: `EntityDoesNotExistError` on unwrap
- Trigger: Authority holder removed from scope
- Fix: Check entity existence before cleanup

**observability-01 through 09:**
- Challenge: Metrics APIs not exposed in test harness
- Options: Check existing APIs, or add feature-gated test hooks
- Status: Needs assessment

---

*This document is the canonical reference for Naia's development process. Follow it exactly.*
