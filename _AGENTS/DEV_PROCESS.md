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

The specification documents in `specs/contracts/*.md` define the correct behavior of Naia. When a test fails, it means the implementation has a bug, not that the spec is wrong.

### 1.2 Two-Phase Development

**Phase A: Complete Test Coverage**
```
Contracts → Compiling Tests (no todo!())
```
- Every spec contract has a compiling E2E test
- Tests are allowed to FAIL - failures indicate implementation gaps
- Goal: 100% coverage (N/N - see PLAN.md for current count), zero `todo!()` macros

**Phase B: Fix Implementation**
```
Failing Tests → Implementation Fixes → Passing Tests
```
- Run all tests, observe failures
- Systematically fix implementation
- Failing tests are the bug tracker

**Key Insight:** A `todo!()` in a test is a **specification gap**, not an implementation bug. Write what you *expect* to happen, and let the test fail if the implementation is wrong. The failing test documents exactly what's broken.

### 1.3 Why This Order Matters

1. **Complete visibility:** You can't fix what you can't see. All spec behavior must be tested first.
2. **Prioritization:** Once all tests exist, you can prioritize fixes by importance, not by which test you wrote first.
3. **Regression prevention:** New tests don't get blocked by existing bugs.
4. **Clear separation:** Test writing is about understanding specs; implementation fixing is about code.

### 1.4 Minimal Diffs Win

- Prefer Edit over Write
- Prefer adding annotation over adding test (if test exists)
- Prefer simple assertion over complex scenario
- Only implement what the contract requires

---

## 2. The SDD Loop

### 2.1 Overview: Two Phases

Development proceeds in two distinct phases:

```
╔═══════════════════════════════════════════════════════════════════╗
║  PHASE A: Complete Test Coverage                                  ║
║                                                                   ║
║  ┌──────────┐     ┌──────────┐     ┌──────────┐                  ║
║  │  1.SPEC  │────▶│  2.TEST  │────▶│  3.VALID │                  ║
║  │   Find   │     │  Write   │     │ Coverage │                  ║
║  └──────────┘     └──────────┘     └──────────┘                  ║
║                                                                   ║
║  Goal: 100% coverage, zero todo!() (see PLAN.md for N/N count)   ║
║  Tests are ALLOWED TO FAIL - that documents implementation gaps   ║
╚═══════════════════════════════════════════════════════════════════╝

                              ↓ Phase A complete

╔═══════════════════════════════════════════════════════════════════╗
║  PHASE B: Fix Implementation                                      ║
║                                                                   ║
║  ┌──────────┐     ┌──────────┐     ┌──────────┐                  ║
║  │  1.RUN   │────▶│  2.FIX   │────▶│ 3.VERIFY │                  ║
║  │  Tests   │     │   Impl   │     │  Passes  │                  ║
║  └──────────┘     └──────────┘     └──────────┘                  ║
║                                                                   ║
║  Goal: All tests pass, implementation matches spec                ║
╚═══════════════════════════════════════════════════════════════════╝
```

### 2.2 Phase A, Step 1: SPEC (Find Contract)

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
4. Validate: `cargo run -p naia-specs -- lint`

### 2.3 Phase A, Step 2: TEST (Write Compiling Test)

**Goal:** Create a compiling test that asserts the contract's expected behavior.

**Critical:** NO `todo!()` macros. Write actual assertions. If the test fails, that's fine - it documents an implementation gap.

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
       // ... setup ...

       // Assert expected behavior (test may fail - that's OK!)
       scenario.expect(|ctx| {
           let result = ctx.client(client_key, |c| c.has_entity(&entity));
           result.then_some(())
       });
   }
   ```
4. Verify test COMPILES (failure is acceptable):
   ```bash
   cargo test --package naia-test --test <file> --no-run  # Must compile
   cargo test --package naia-test contract_name_scenario   # May fail
   ```

### 2.4 Phase A, Step 3: VALID (Verify Coverage)

**Goal:** Confirm the contract has a compiling test.

**Steps:**
1. Run coverage check:
   ```bash
   cargo run -p naia-specs -- coverage
   ```
2. Verify the contract ID appears in "covered" output
3. Check for remaining todos:
   ```bash
   grep -r "todo!" test/tests/*.rs
   ```

### 2.5 Phase B, Step 1: RUN (Identify Failures)

**Goal:** See which tests fail and understand the implementation gaps.

**Prerequisites:** Phase A complete (100% coverage, zero `todo!()`)

**Steps:**
1. Run all tests:
   ```bash
   cargo test --package naia-test
   ```
2. Collect list of failing tests
3. Prioritize by importance/risk

### 2.6 Phase B, Step 2: FIX (Implement Correct Behavior)

**Goal:** Make the test pass by fixing the implementation.

**Steps:**
1. Read the failing test to understand expected behavior
2. Read the spec contract for context
3. Make minimal changes to the relevant crate:
   - `shared/` for protocol-level changes
   - `client/` for client-side behavior
   - `server/` for server-side behavior
4. Run test until it passes
5. Run quality checks:
   ```bash
   cargo clippy --no-deps
   cargo fmt -- --check
   ```

### 2.7 Phase B, Step 3: VERIFY (Confirm Fix)

**Goal:** Confirm the fix is correct and doesn't regress other tests.

**Steps:**
1. Run test 3x for flakiness check:
   ```bash
   cargo test --package naia-test contract_name_scenario
   cargo test --package naia-test contract_name_scenario
   cargo test --package naia-test contract_name_scenario
   ```
2. Run full test suite to check for regressions:
   ```bash
   cargo test --package naia-test
   ```

### 2.8 Phase 3: Adequacy Review (Optional)

**Goal:** Verify tests adequately cover contract semantics before claiming implementation complete.

**When to use:** After implementation passes but before marking contract complete.

**Steps:**
1. Generate review packet:
   ```bash
   cargo run -p naia-specs -- packet <contract-id>              # Concise mode
   cargo run -p naia-specs -- packet <contract-id> --full-tests # Include full test code
   ```
2. Review packet contents:
   - Contract guarantee, preconditions, postconditions
   - Test assertion indices (expect_msg labels)
   - Full test code (if --full-tests used)
3. Map spec semantics to test assertions:
   - Does each postcondition have a corresponding assertion?
   - Are preconditions properly set up?
   - Is the guarantee actually tested?
4. Add `expect_msg` labels for deterministic review:
   ```rust
   scenario.expect_msg("Client receives entity", |ctx| {
       ctx.client(key, |c| c.has_entity(&entity)).then_some(())
   });
   ```
5. Re-verify after improvements:
   ```bash
   cargo run -p naia-specs -- verify --contract <id>
   ```

**Purpose:** Ensures tests are comprehensive and maintainable, not just passing.

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

### 3.5 Failing Test Pattern (Preferred)

When a test fails due to an implementation bug, **write the full test anyway**:

```rust
/// Contract: [entity-authority-11]
/// Contract: [entity-authority-12]
#[test]
fn out_of_scope_ends_authority_for_that_client() {
    let mut scenario = Scenario::new();
    // ... full setup ...

    // Remove client from scope
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.user_scope_mut(&client_key).unwrap().exclude(&entity);
        });
    });

    // Assert expected behavior (may fail - that's OK!)
    // The failure message documents the implementation gap
    scenario.expect(|ctx| {
        let status = ctx.client(client_key, |c| {
            c.entity(&entity).and_then(|e| e.authority())
        });
        // Client should no longer have the entity
        (status.is_none()).then_some(())
    });
}
```

**Why this is better than `todo!()`:**
- Test compiles and runs
- Failure message documents exactly what's broken
- Coverage tools see the test
- When implementation is fixed, test automatically passes

### 3.6 mutate() vs expect() - Critical Anti-Patterns

**NEVER VIOLATE THE ALTERNATION RULE**

The test harness enforces `mutate()` ↔ `expect()` alternation by design. Sequential `mutate()` calls will cause a panic. This is intentional - don't work around it!

**Purpose:**
- `mutate()` - **Change state only** (spawn, send messages, modify components)
- `expect()` - **Wait/poll** until a condition becomes true

**❌ ANTI-PATTERN 1: Empty expect() to "fix" sequential mutate()**
```rust
// WRONG - trying to get tick, then send message
let tick = scenario.mutate(|ctx| {
    ctx.client(key, |c| c.client_tick())  // Just reading state!
});
scenario.expect(|_| Some(()));  // ← WRONG! Empty wait does nothing!
scenario.mutate(|ctx| {
    ctx.client(key, |c| c.send_message(&tick, &msg));
});
```

**✅ CORRECT: Merge into one mutate() block**
```rust
let tick = scenario.mutate(|ctx| {
    ctx.client(key, |client| {
        let tick = client.client_tick();  // Read AND use in same block
        client.send_message(&tick, &msg);
        tick
    })
});
```

**❌ ANTI-PATTERN 2: Using mutate() for queries**
```rust
// WRONG - mutate() is for changing state, not reading it
let value = scenario.mutate(|ctx| {
    ctx.server(|s| s.get_some_value())  // This is a query!
});
```

**✅ CORRECT: Combine query with actual mutation**
```rust
scenario.mutate(|ctx| {
    ctx.server(|server| {
        let value = server.get_some_value();  // Read
        server.do_something_with(value);       // Mutate
    })
});
```

**❌ ANTI-PATTERN 3: Empty no-op mutations**
```rust
// WRONG - does nothing!
scenario.mutate(|_| {});
scenario.expect(|_| Some(()));
```

**✅ CORRECT: Just use expect()**
```rust
scenario.expect(|_| Some(()));
```

**When you see sequential mutate() calls:**
1. **Can they be merged?** → YES, merge them into one block
2. **Is one just reading state?** → Move the read into the next mutation
3. **Is one empty/no-op?** → Delete it
4. **Is there real waiting needed?** → Add expect() with actual condition

**The panic is your friend - it prevents bad test design. Fix the structure, don't hack around it.**

---

## 4. Tooling Reference

### 4.1 CLI Commands

| Command | Purpose | When to Use |
|---------|---------|-------------|
| `cargo run -p naia-specs -- verify` | Full verification pipeline (specs + tests + coverage) | Phase B: Check overall health |
| `cargo run -p naia-specs -- verify --contract <id>` | Run tests for one contract only | Phase B: Fast iteration loop |
| `cargo run -p naia-specs -- coverage` | Show covered/uncovered contracts | Phase A: Track coverage progress |
| `cargo run -p naia-specs -- packet <id>` | Generate adequacy review packet | Phase 3: Contract verification |
| `cargo run -p naia-specs -- traceability` | Generate contract↔test matrix | After adding tests |
| `cargo run -p naia-specs -- lint` | Validate spec format | After editing specs |
| `cargo run -p naia-specs -- gen-test <id>` | Generate test skeleton | Starting new test |
| `cargo run -p naia-specs -- bundle` | Generate NAIA_SPECS.md | After spec changes |
| `cargo run -p naia-specs -- registry` | Generate CONTRACT_REGISTRY.md | After adding contracts |
| `cargo run -p naia-specs -- check-orphans` | Find untracked MUSTs | Spec cleanup |

**Verify Command Options:**
- `--contract <id>`: Run tests only for specific contract (fast iteration)
- `--strict-coverage`: Fail if any contracts uncovered (CI enforcement)
- `--strict-orphans`: Fail if orphan MUST statements exist (CI enforcement)
- `--write-report <path>`: Write summary to markdown file

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

### 6.1 Phase A: Add Test Coverage for Contract

```
1. `cargo run -p naia-specs -- coverage`
2. Pick uncovered contract from output
3. grep -l "<domain>" test/tests/*.rs
4. Read similar test (targeted)
5. Write COMPILING test with annotation (no todo!())
6. cargo test --package naia-test --test <file> --no-run  # Must compile
7. cargo test --package naia-test <test_name>              # May fail - OK!
8. `cargo run -p naia-specs -- coverage` (verify annotation)
```

### 6.2 Phase A: Eliminate todo!() Macro

```
1. grep -r "todo!" test/tests/*.rs
2. Read the test with todo!()
3. Read the spec contract it references
4. Write actual assertions for expected behavior
5. cargo test --package naia-test --test <file> --no-run  # Must compile
6. cargo test --package naia-test <test_name>              # May fail - OK!
```

### 6.3 Phase B: Fix Failing Test

**Prerequisites:** Phase A complete (100% coverage, zero `todo!()`)

```
1. `cargo run -p naia-specs -- verify --contract <id>`  # Fast: see targeted failures
   OR cargo test --package naia-test            # Full: see all failures
2. Pick a failing test/contract
3. Read the test to understand expected behavior
4. Read the spec contract for context
5. Fix implementation (minimal changes)
6. `cargo run -p naia-specs -- verify --contract <id>`  # Fast: verify fix
7. Run 3x for flakiness
8. `cargo run -p naia-specs -- verify`                  # Full: check no regressions
```

**Tip:** Use `verify --contract <id>` for fast iteration (~5-10 sec vs ~5-10 min)

### 6.4 New Feature

```
1. Draft contracts in spec file
2. `cargo run -p naia-specs -- lint`
3. `cargo run -p naia-specs -- gen-test <contract-id>`
4. Write FULL test (no todo!()) - test will fail
5. `cargo run -p naia-specs -- coverage`  # Verify annotation
--- Phase A complete for this feature ---
6. Implement feature code
7. Verify test passes
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
- [ ] Pass `cargo run -p naia-specs -- lint`
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
2. Run `cargo run -p naia-specs -- coverage` (not cached)
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
| `connection-` | Connection lifecycle | 1_connection_lifecycle.md |
| `transport-` | Transport layer | 2_transport.md |
| `messaging-` | Message passing | 3_messaging.md |
| `time-` | Time/tick semantics | 5_time_ticks_commands.md |
| `commands-` | Command buffer | 5_time_ticks_commands.md |
| `observability-` | Metrics | 5_observability_metrics.md |
| `entity-scopes-` | Scope predicates | 6_entity_scopes.md |
| `entity-replication-` | Replication protocol | 7_entity_replication.md |
| `entity-ownership-` | Ownership rules | 8_entity_ownership.md |
| `entity-publication-` | Publication state | 9_entity_publication.md |
| `entity-delegation-` | Delegation mechanics | 10_entity_delegation.md |
| `entity-authority-` | Authority state machine | 11_entity_authority.md |
| `server-events-` | Server event API | 12_server_events_api.md |
| `client-events-` | Client event API | 13_client_events_api.md |
| `world-integration-` | ECS integration | 14_world_integration.md |

### Appendix B: Test File Map (1:1 Spec Mapping)

Test files now map 1:1 to spec files for instant traceability:

| Spec File | Test File |
|-----------|-----------|
| `1_connection_lifecycle.md` | `01_connection_lifecycle.rs` |
| `2_transport.md` | `02_transport.rs` |
| `3_messaging.md` | `03_messaging.rs` |
| `4_time_ticks_commands.md` | `04_time_ticks_commands.rs` |
| `5_observability_metrics.md` | `05_observability_metrics.rs` |
| `6_entity_scopes.md` | `06_entity_scopes.rs` |
| `7_entity_replication.md` | `07_entity_replication.rs` |
| `8_entity_ownership.md` | `08_entity_ownership.rs` |
| `9_entity_publication.md` | `09_entity_publication.rs` |
| `10_entity_delegation.md` | `10_entity_delegation.rs` |
| `11_entity_authority.md` | `11_entity_authority.rs` |
| `12_server_events_api.md` | `12_server_events_api.rs` |
| `13_client_events_api.md` | `13_client_events_api.rs` |
| `14_world_integration.md` | `14_world_integration.rs` |

**Finding tests:** To find tests for spec `N_domain.md`, open `NN_domain.rs`

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
[ ] Run `cargo run -p naia-specs -- verify` (optional health check)
[ ] Determine current phase (A or B)
[ ] Identify next action
```

**Phase A work (Complete Test Coverage):**
```
[ ] Pick uncovered contract OR test with todo!()
[ ] Write COMPILING test (no todo!())
[ ] Verify test compiles: cargo test --package naia-test --test <file> --no-run
[ ] Test may FAIL - that's acceptable
[ ] Verify coverage: `cargo run -p naia-specs -- coverage`
```

**Phase B work (Fix Implementation):**
```
[ ] Prerequisites: 236/236 coverage, zero todo!()
[ ] Run `cargo run -p naia-specs -- verify --contract <id>`  # Fast iteration
[ ] Pick failing test/contract
[ ] Fix implementation (minimal changes)
[ ] Run `cargo run -p naia-specs -- verify --contract <id>`  # Verify fix
[ ] Run 3x for flakiness
[ ] Run `cargo run -p naia-specs -- verify`                  # Check no regressions
```

**End of session:**
```
[ ] Run `cargo run -p naia-specs -- verify` (optional: final health check)
[ ] Update PLAN.md if state changed
[ ] Run `cargo run -p naia-specs -- traceability` if tests added
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

## Appendix F: Phase B Lessons Learned (2026-01-11)

### Critical: Quality Engineering Over Quick Hacks

**Lesson 1: Never Return Default Values - Implement Features Properly**

❌ **WRONG - Silently returning defaults:**
```rust
pub fn outgoing_bandwidth(&self) -> f32 {
    self.monitor.as_ref().map(|m| m.bandwidth()).unwrap_or(0.0)  // WRONG!
}
```

✅ **CORRECT - Enable the feature in configuration:**
```rust
// In test config:
config.connection.bandwidth_measure_duration = Some(Duration::from_secs(1));

// In implementation:
pub fn outgoing_bandwidth(&self) -> f32 {
    self.monitor.as_ref()
        .expect("Set bandwidth_measure_duration in config to enable monitoring")
        .bandwidth()
}
```

**Why this matters:** Tests should drive proper implementation, not paper over missing features. If a spec says metrics are queryable, implement the monitoring system properly.

**Lesson 2: Framework Violations Are Intentional - Fix Test Structure**

The test harness panics on `mutate()` → `mutate()` by design. This enforces good test patterns.

❌ **WRONG - Adding empty expect() as a spacer:**
```rust
let tick = scenario.mutate(|ctx| { ctx.client(k, |c| c.get_tick()) });
scenario.expect(|_| Some(()));  // WRONG! Does nothing!
scenario.mutate(|ctx| { /* use tick */ });
```

✅ **CORRECT - Merge read+write operations:**
```rust
let tick = scenario.mutate(|ctx| {
    ctx.client(k, |c| {
        let tick = c.get_tick();  // Read
        c.send_message(&tick, &msg);  // Write
        tick
    })
});
```

**Why this matters:** Empty `expect()` calls don't wait for anything. They're a code smell that indicates bad test structure.

**Lesson 3: Categorize Failures Before Fixing**

Not all test failures are implementation bugs:

| Failure Type | Indicator | Root Cause | Fix Strategy |
|--------------|-----------|------------|--------------|
| **Test Structure** | Panic at scenario.rs:155/213 | Sequential `mutate()` calls | Merge operations |
| **Timeout** | Panic at scenario.rs:253 | Missing impl or wrong assertion | Debug with e2e_debug |
| **Assertion** | Test fails at expect() | Logic bug | Fix implementation |
| **Compilation** | Rust error | Missing API or wrong types | Implement feature |

**Action:** Always categorize before fixing. Don't assume every failure is an implementation bug.

**Lesson 4: Fix Low-Hanging Fruit First**

In Phase B, we had:
- 13 test structure issues (easy fixes - just merge mutate calls)
- 8 timeout failures (medium difficulty - debug needed)
- 15 logic bugs (hard - state machine issues)

**Strategy:** Fix the 13 structure issues first for quick wins, then tackle harder problems. This builds momentum and reduces noise.

**Lesson 5: Use Saturating Arithmetic for Edge Cases**

❌ **WRONG - Assuming values are always ordered:**
```rust
let rtt_delay = round_trip_time - server_process_time;  // Can underflow!
```

✅ **CORRECT - Handle edge cases gracefully:**
```rust
// In fast tests or clock inconsistencies, server time might appear > RTT
let rtt_delay = round_trip_time.saturating_sub(server_process_time);
```

**Why this matters:** E2E tests run in microseconds. Clock measurements can have unexpected orderings. Use saturating arithmetic for time calculations.

---

*This document is the canonical reference for Naia's development process. Follow it exactly.*
