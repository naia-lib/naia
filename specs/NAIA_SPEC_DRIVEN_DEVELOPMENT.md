# Naia Spec-Driven Development Process

**Version:** 1.0
**Created:** 2026-01-11
**Purpose:** A systematic, iron-clad process for using Claude Code (Opus 4.5) to drive development from specifications through E2E tests to implementation.

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [Current State Analysis](#2-current-state-analysis)
3. [The SDD Process](#3-the-sdd-process)
4. [Tooling Architecture](#4-tooling-architecture)
5. [Contract-to-Test Mapping](#5-contract-to-test-mapping)
6. [Claude Code Efficiency Guidelines](#6-claude-code-efficiency-guidelines)
7. [Quality Gates](#7-quality-gates)
8. [Workflow Templates](#8-workflow-templates)
9. [Gap Analysis & Remediation](#9-gap-analysis--remediation)
10. [Appendices](#appendices)

---

## 1. Executive Summary

### What is Spec-Driven Development (SDD)?

SDD is a methodology where **specifications are the source of truth**. The flow is:

```
specs/*.md (normative contracts)
    ↓ [GENERATE]
test/tests/*.rs (E2E tests that verify contracts)
    ↓ [DRIVE]
Implementation (code that passes tests)
    ↓ [VALIDATE]
specs/*.md (confirm implementation matches spec)
```

### Current Assets

| Asset | Count | Location |
|-------|-------|----------|
| Specifications | 16 files | `specs/*.md` |
| Contracts | 185 | `specs/CONTRACT_REGISTRY.md` |
| E2E Tests | 149 | `test/tests/*.rs` |
| Test Harness | 24 modules | `test/src/harness/` |
| Spec Tooling | 1 CLI | `specs/spec_tool.sh` |

### The Gap

- **185 contracts** defined in specs
- **149 tests** exist
- **Unknown**: How many contracts have test coverage?
- **Missing**: Contract-to-test traceability matrix

---

## 2. Current State Analysis

### 2.1 Specification Quality Assessment

**Strengths:**
- RFC 2119 normative keywords (MUST, MUST NOT, MAY, SHOULD)
- Master glossary with 30+ defined terms
- State transition tables for key state machines
- Temporal semantics defined (Immediately, Eventually, Same tick, etc.)
- Contract ID format standardized: `[spec-name-NN]`

**Weaknesses:**
- 24 orphan MUST statements (not associated with contract IDs)
- Test obligations sections are TODO placeholders in most specs
- No explicit Given/When/Then scenarios for contracts
- No machine-readable contract extraction format (JSON/YAML)

### 2.2 Test Infrastructure Assessment

**Strengths:**
- Sophisticated `Scenario` harness with deterministic time (`TestClock`)
- `mutate()`/`expect()` pattern enforces proper test structure
- Entity registry tracks entities across client/server boundaries
- Helper functions (`client_connect()`, `test_client_config()`)
- Link conditioner for network simulation

**Weaknesses:**
- No contract ID annotations in tests
- No coverage tracking by contract
- No automated test generation from specs
- Test names don't consistently reference contract IDs

### 2.3 Test Coverage by Domain

| Domain | Spec File | Contracts | Estimated Test Coverage |
|--------|-----------|-----------|------------------------|
| Connection | `2_connection_lifecycle.md` | 27 | ~10 tests (37%) |
| Transport | `3_transport.md` | 5 | ~8 tests (160%) |
| Messaging | `4_messaging.md` | 20 | ~25 tests (125%) |
| Time/Ticks | `5_time_ticks_commands.md` | 17 | ~15 tests (88%) |
| Observability | `6_observability_metrics.md` | 9 | ~4 tests (44%) |
| Entity Scopes | `7_entity_scopes.md` | 15 | ~12 tests (80%) |
| Entity Replication | `8_entity_replication.md` | 12 | ~8 tests (67%) |
| Entity Ownership | `9_entity_ownership.md` | 33 | ~15 tests (45%) |
| Entity Publication | `10_entity_publication.md` | 11 | ~8 tests (73%) |
| Entity Delegation | `11_entity_delegation.md` | 17 | ~12 tests (71%) |
| Entity Authority | `12_entity_authority.md` | 16 | ~14 tests (88%) |
| Server Events | `13_server_events_api.md` | 14 | ~10 tests (71%) |
| Client Events | `14_client_events_api.md` | 13 | ~10 tests (77%) |
| World Integration | `15_world_integration.md` | 9 | ~8 tests (89%) |

**Note:** These are estimates. Actual traceability requires the tooling defined in Section 4.

---

## 3. The SDD Process

### 3.1 The Canonical Loop

```
┌─────────────────────────────────────────────────────────────────┐
│                    SPEC-DRIVEN DEVELOPMENT LOOP                 │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   ┌──────────┐     ┌──────────┐     ┌──────────┐              │
│   │  1. SPEC │────▶│ 2. TEST  │────▶│ 3. IMPL  │              │
│   │  Define  │     │  Write   │     │  Build   │              │
│   └──────────┘     └──────────┘     └──────────┘              │
│        ▲                                  │                    │
│        │           ┌──────────┐          │                    │
│        └───────────│ 4. VALID │◀─────────┘                    │
│                    │  Verify  │                                │
│                    └──────────┘                                │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### 3.2 Phase 1: SPEC (Define the Contract)

**Input:** Feature requirement or bug report
**Output:** Contract(s) in spec file with unique IDs

**Steps:**
1. Identify which spec file owns this behavior
2. Draft the contract using the template:
   ```markdown
   ### [spec-name-NN] — Short descriptive title

   **Guarantee:** One sentence describing the invariant.

   **Preconditions:**
   - List of conditions that must be true

   **Postconditions:**
   - List of conditions that must be true after

   **Test Scenarios:**
   - Scenario 1: Given X, When Y, Then Z
   ```
3. Run `./spec_tool.sh lint` to validate format
4. Run `./spec_tool.sh check-orphans` to ensure no orphan MUSTs
5. Regenerate `./spec_tool.sh bundle` and `./spec_tool.sh registry`

**Claude Code Command:**
```
Read the spec for [domain]. Add contract [spec-name-NN] that guarantees [behavior].
Follow the contract format in specs/1_template.md. Run spec_tool.sh lint after.
```

### 3.3 Phase 2: TEST (Write E2E Test)

**Input:** Contract ID from Phase 1
**Output:** Test function in `test/tests/*.rs`

**Steps:**
1. Create test function with contract annotation:
   ```rust
   /// Contract: [entity-scopes-07]
   /// Guarantee: When E becomes OutOfScope for user U, E MUST be despawned on U's client.
   ///
   /// Scenario: Room removal causes despawn
   /// - Given: Entity E in Room R, User U in Room R, InScope(U, E)
   /// - When: Server removes E from Room R (no other shared rooms)
   /// - Then: OutOfScope(U, E), E despawned on U's client
   #[test]
   fn entity_scopes_07_room_removal_causes_despawn() {
       // Implementation
   }
   ```
2. Use the `Scenario` harness pattern:
   - Setup with `Scenario::new()`, `server_start()`, `client_connect()`
   - Preconditions in `mutate()`
   - Trigger in `mutate()`
   - Verification in `expect()`
3. Run `cargo test --package naia-test <test_name>`
4. Test MUST fail initially (Red phase)

**Claude Code Command:**
```
Write an E2E test for contract [spec-name-NN] in test/tests/[file].rs.
Follow the Given/When/Then from the spec. Use the Scenario harness.
Test should fail initially since implementation doesn't exist yet.
```

### 3.4 Phase 3: IMPL (Build Implementation)

**Input:** Failing test from Phase 2
**Output:** Code changes that make the test pass

**Steps:**
1. Identify which crate(s) need modification:
   - `shared/` for protocol-level changes
   - `client/` for client-side behavior
   - `server/` for server-side behavior
   - `adapters/` for ECS integration
2. Implement the minimal code to pass the test
3. Run `cargo test --package naia-test` to verify
4. Run `cargo clippy --no-deps` for lint
5. Run `cargo fmt -- --check` for formatting

**Claude Code Command:**
```
The test [test_name] is failing. Implement the behavior specified in contract
[spec-name-NN] to make it pass. Make minimal changes. Run the test after.
```

### 3.5 Phase 4: VALID (Verify Against Spec)

**Input:** Passing test from Phase 3
**Output:** Updated spec with test coverage annotation

**Steps:**
1. Update spec's Test Obligations section:
   ```markdown
   ## Test Obligations

   | Contract | Test | Status |
   |----------|------|--------|
   | [entity-scopes-07] | entity_scopes_07_room_removal_causes_despawn | PASS |
   ```
2. Run full test suite: `cargo test --workspace`
3. Run spec validation: `./spec_tool.sh validate`
4. Regenerate bundle: `./spec_tool.sh bundle`

**Claude Code Command:**
```
Test [test_name] passes. Update specs/[file].md Test Obligations section to
mark contract [spec-name-NN] as covered. Run spec_tool.sh validate.
```

---

## 4. Tooling Architecture

### 4.1 Current Tools

| Tool | Command | Purpose |
|------|---------|---------|
| `spec_tool.sh bundle` | Generate `NAIA_SPECS.md` | Concatenate all specs |
| `spec_tool.sh lint` | Validate spec format | Check contract IDs, cross-refs |
| `spec_tool.sh validate` | Full validation | lint + check-refs + check-orphans |
| `spec_tool.sh registry` | Generate `CONTRACT_REGISTRY.md` | Index all contracts |
| `spec_tool.sh check-orphans` | Find untracked MUSTs | Identify missing contract IDs |
| `spec_tool.sh check-refs` | Validate cross-references | Ensure spec links resolve |
| `spec_tool.sh stats` | Show statistics | Lines, words, contracts per spec |

### 4.2 Required New Tools

#### Tool 1: `spec_tool.sh coverage` - Contract Coverage Tracker

**Purpose:** Extract contract annotations from tests and compute coverage.

**Implementation:**
```bash
# Add to spec_tool.sh
cmd_coverage() {
    print_header "Contract Coverage Analysis"

    # Extract contract annotations from tests
    # Pattern: /// Contract: [contract-id]
    local test_contracts=$(grep -rh '/// Contract: \[' test/tests/*.rs \
        | grep -oE '\[[a-z-]+-[0-9]+\]' | tr -d '[]' | sort -u)

    # Compare with registry
    local all_contracts=$(grep -oE '`[a-z-]+-[0-9]+`' CONTRACT_REGISTRY.md \
        | tr -d '`' | sort -u)

    local covered=$(echo "$test_contracts" | wc -l)
    local total=$(echo "$all_contracts" | wc -l)

    echo "Coverage: $covered / $total contracts ($(( covered * 100 / total ))%)"

    echo ""
    echo "Uncovered contracts:"
    comm -23 <(echo "$all_contracts") <(echo "$test_contracts")
}
```

#### Tool 2: `spec_tool.sh gen-test` - Test Skeleton Generator

**Purpose:** Generate test skeleton from contract ID.

**Implementation:**
```bash
cmd_gen_test() {
    local contract_id="$1"
    local spec_file=$(grep -l "\[$contract_id\]" specs/*.md | head -1)

    if [[ -z "$spec_file" ]]; then
        print_error "Contract $contract_id not found"
        return 1
    fi

    # Extract contract details
    local title=$(grep -A1 "\[$contract_id\]" "$spec_file" | head -1)
    local guarantee=$(grep -A5 "\[$contract_id\]" "$spec_file" | grep "Guarantee:" | head -1)

    cat << EOF
/// Contract: [$contract_id]
/// $guarantee
///
/// Scenario: TODO - describe Given/When/Then
#[test]
fn ${contract_id//-/_}() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());

    // TODO: Setup (Given)

    // TODO: Action (When)
    scenario.mutate(|ctx| {
        // Trigger the behavior
    });

    // TODO: Verification (Then)
    scenario.expect(|ctx| {
        // Assert the postcondition
        todo!("Implement assertion")
    });
}
EOF
}
```

#### Tool 3: `spec_tool.sh traceability` - Full Traceability Matrix

**Purpose:** Generate bidirectional contract-to-test mapping.

**Output Format:**
```markdown
# Traceability Matrix

## Contracts → Tests

| Contract | Tests | Status |
|----------|-------|--------|
| [connection-01] | basic_connect_disconnect_lifecycle | COVERED |
| [connection-02] | basic_connect_disconnect_lifecycle | COVERED |
| [entity-scopes-07] | - | UNCOVERED |

## Tests → Contracts

| Test | Contracts Verified |
|------|-------------------|
| basic_connect_disconnect_lifecycle | connection-01, connection-02, connection-03 |
```

#### Tool 4: `spec_tool.sh annotate-tests` - Batch Test Annotation

**Purpose:** Add contract annotations to existing tests based on naming patterns.

**Implementation:** Analyzes test names and content to suggest contract mappings.

### 4.3 Enhanced Test Infrastructure

#### Addition 1: Contract Annotation Macro

```rust
// In test/src/lib.rs
/// Macro to annotate tests with contract IDs for traceability
#[macro_export]
macro_rules! contract_test {
    ($contract:literal, $name:ident, $body:block) => {
        #[doc = concat!("Contract: [", $contract, "]")]
        #[test]
        fn $name() $body
    };
}

// Usage:
contract_test!("entity-scopes-07", room_removal_causes_despawn, {
    let mut scenario = Scenario::new();
    // ...
});
```

#### Addition 2: Test Registry

```rust
// In test/src/contract_registry.rs
use std::collections::HashMap;
use lazy_static::lazy_static;

lazy_static! {
    pub static ref CONTRACT_TESTS: HashMap<&'static str, Vec<&'static str>> = {
        let mut m = HashMap::new();
        m.insert("entity-scopes-07", vec!["room_removal_causes_despawn"]);
        m.insert("connection-01", vec![
            "basic_connect_disconnect_lifecycle",
            "connect_event_ordering_stable"
        ]);
        m
    };
}
```

---

## 5. Contract-to-Test Mapping

### 5.1 Naming Convention

**Test Function Names:**
```
{contract_id}_{scenario_slug}
```

Examples:
- `entity_scopes_07_room_removal_causes_despawn`
- `connection_01_basic_lifecycle`
- `messaging_05_ordered_delivery`

### 5.2 Test File Organization

```
test/tests/
├── connection_*.rs       # connection-XX contracts
├── transport_*.rs        # transport-XX contracts
├── messaging_*.rs        # messaging-XX contracts
├── time_ticks_*.rs       # time-XX, commands-XX contracts
├── observability_*.rs    # observability-XX contracts
├── entity_scopes_*.rs    # entity-scopes-XX contracts
├── entity_replication_*.rs # entity-replication-XX contracts
├── entity_ownership_*.rs # entity-ownership-XX contracts
├── entity_publication_*.rs # entity-publication-XX contracts
├── entity_delegation_*.rs # entity-delegation-XX contracts
├── entity_authority_*.rs # entity-authority-XX contracts
├── server_events_*.rs    # server-events-XX contracts
├── client_events_*.rs    # client-events-XX contracts
└── world_integration_*.rs # world-integration-XX contracts
```

### 5.3 Contract Annotation Format

Every test MUST include a contract annotation block:

```rust
/// Contract: [entity-scopes-07]
///
/// Guarantee: When E becomes OutOfScope for user U, E MUST be despawned
/// on U's client.
///
/// Scenario: Room removal causes despawn
/// Given:
///   - Entity E exists and is in Room R
///   - User U is in Room R
///   - InScope(U, E) is true
/// When:
///   - Server removes E from Room R (no other shared rooms)
/// Then:
///   - OutOfScope(U, E) becomes true
///   - E is despawned on U's client within same tick
#[test]
fn entity_scopes_07_room_removal_causes_despawn() {
    // ...
}
```

---

## 6. Claude Code Efficiency Guidelines

### 6.1 Token Optimization Strategies

| Strategy | Savings | Implementation |
|----------|---------|----------------|
| **Batch Operations** | 40-60% | Use sed/grep for multi-file changes |
| **Parallel Tool Calls** | 30-50% | Combine independent reads/writes |
| **Targeted Reads** | 50-70% | Read specific line ranges, not full files |
| **Template Reuse** | 20-30% | Reference existing patterns, don't repeat |
| **Incremental Changes** | 40-50% | Small edits vs. full file rewrites |

### 6.2 Efficient Prompts

**Bad (verbose):**
```
I need you to read the entire NAIA_SPECS.md file, understand all the contracts,
then go through each test file one by one, and for each test determine which
contract it covers, and then create a comprehensive report...
```

**Good (targeted):**
```
Run: grep -l "entity-scopes" test/tests/*.rs
Then read lines 1-50 of matching files to find contract annotations.
Output: table of test names → contract IDs.
```

### 6.3 Session Structure

**Optimal Session Length:** 3-5 major tasks

**Session Template:**
```
1. State goal clearly
2. Provide contract ID(s) to work on
3. Specify deliverable format
4. Allow autonomous execution
5. Review outputs at end
```

### 6.4 Cost-Effective Patterns

**Pattern 1: Explore Agent for Research**
```
Use Task tool with subagent_type=Explore for:
- Finding all tests related to a domain
- Understanding codebase patterns
- Locating specific implementations
```

**Pattern 2: Haiku for Simple Tasks**
```
Use model=haiku for:
- Running grep/sed commands
- Simple file reads
- Format validation
```

**Pattern 3: Opus for Complex Reasoning**
```
Reserve Opus for:
- Writing new tests
- Implementing contract behavior
- Analyzing spec ambiguities
```

---

## 7. Quality Gates

### 7.1 Spec Quality Gate

Before a spec change is accepted:

```bash
./spec_tool.sh lint          # 0 errors, ≤1 warning
./spec_tool.sh check-refs    # All references valid
./spec_tool.sh check-orphans # Manual review of any new orphans
```

### 7.2 Test Quality Gate

Before a test is accepted:

```bash
# Test compiles
cargo test --package naia-test --no-run

# Test has contract annotation
grep -q "/// Contract: \[" test/tests/<file>.rs

# Test follows naming convention
# Function name starts with contract_id pattern

# Test uses Scenario harness
grep -q "Scenario::new()" test/tests/<file>.rs
```

### 7.3 Implementation Quality Gate

Before implementation is accepted:

```bash
# All tests pass
cargo test --workspace

# No new warnings
cargo clippy --no-deps -- -D warnings

# Formatting clean
cargo fmt -- --check

# Coverage check (when tooling available)
./spec_tool.sh coverage  # No regression
```

### 7.4 Release Quality Gate

Before a release:

```bash
# Full spec validation
./spec_tool.sh validate

# Generate fresh artifacts
./spec_tool.sh bundle
./spec_tool.sh registry
./spec_tool.sh coverage  # ≥80% contract coverage

# Full test suite
cargo test --workspace

# Build all targets
cargo build --release
cargo build --target wasm32-unknown-unknown
```

---

## 8. Workflow Templates

### 8.1 New Feature Workflow

```
USER: Add support for [feature]

CLAUDE:
1. Identify spec domain
2. Draft contracts in spec file
3. Run spec_tool.sh lint
4. Generate test skeletons
5. Implement tests (should fail)
6. Implement feature code
7. Verify tests pass
8. Update spec test obligations
9. Run full validation
```

### 8.2 Bug Fix Workflow

```
USER: Fix bug where [behavior] doesn't match spec [contract-id]

CLAUDE:
1. Read contract from spec
2. Find existing test (or note absence)
3. Write/update test to reproduce bug
4. Verify test fails
5. Fix implementation
6. Verify test passes
7. Run regression suite
```

### 8.3 Spec Clarification Workflow

```
USER: The spec for [contract-id] is ambiguous about [case]

CLAUDE:
1. Read full contract context
2. Check related contracts
3. Review existing tests for implied behavior
4. Propose spec clarification
5. Update spec with unambiguous language
6. Add test for clarified case
7. Regenerate bundle
```

### 8.4 Coverage Improvement Workflow

```
USER: Improve test coverage for [spec-domain]

CLAUDE:
1. Run spec_tool.sh coverage (once tooling exists)
2. List uncovered contracts
3. Prioritize by risk/complexity
4. Generate test skeletons for each
5. Implement tests
6. Update spec test obligations
7. Report coverage improvement
```

---

## 9. Gap Analysis & Remediation

### 9.1 Immediate Actions (P0)

| Action | Owner | Deliverable |
|--------|-------|-------------|
| Implement `spec_tool.sh coverage` | Claude | New command in spec_tool.sh |
| Implement `spec_tool.sh gen-test` | Claude | New command in spec_tool.sh |
| Add contract annotations to existing tests | Claude | Updated test files |
| Create traceability matrix | Claude | `TRACEABILITY.md` |

### 9.2 Short-Term Actions (P1)

| Action | Owner | Deliverable |
|--------|-------|-------------|
| Fill Test Obligations in all specs | Claude | Updated spec files |
| Standardize test naming to include contract IDs | Claude | Renamed test functions |
| Add contract_test! macro | Claude | New macro in test/src/lib.rs |
| Create per-domain test coverage reports | Claude | Coverage by spec domain |

### 9.3 Medium-Term Actions (P2)

| Action | Owner | Deliverable |
|--------|-------|-------------|
| Add Given/When/Then to all contracts | Claude | Updated spec files |
| Create CI pipeline for spec validation | Claude | GitHub Actions workflow |
| Implement automated test generation | Claude | Enhanced gen-test command |
| Create spec diff tool for PRs | Claude | New spec_tool.sh command |

### 9.4 Spec Improvement Priorities

Based on analysis, these specs need the most work:

1. **`9_entity_ownership.md`** (33 contracts, ~45% coverage)
   - Uses non-standard bullet format for contracts
   - Many orphan MUST statements
   - Needs restructuring to heading format

2. **`2_connection_lifecycle.md`** (27 contracts, ~37% coverage)
   - Recently migrated format
   - Test obligations section empty
   - Critical path - affects all other domains

3. **`6_observability_metrics.md`** (9 contracts, ~44% coverage)
   - Metrics are hard to test deterministically
   - Needs tolerance constants (now added)
   - May need mock time for convergence tests

---

## Appendices

### Appendix A: Contract ID Reference

Full registry at `specs/CONTRACT_REGISTRY.md`

Quick reference by prefix:
- `connection-XX` - Connection lifecycle (27)
- `transport-XX` - Transport layer (5)
- `messaging-XX` - Message passing (20)
- `time-XX` - Time/tick semantics (12)
- `commands-XX` - Command buffer (5)
- `observability-XX` - Metrics (9)
- `entity-scopes-XX` - Scope predicates (15)
- `entity-replication-XX` - Replication protocol (12)
- `entity-ownership-XX` - Ownership rules (33)
- `entity-publication-XX` - Publication state (11)
- `entity-delegation-XX` - Delegation mechanics (17)
- `entity-authority-XX` - Authority state machine (16)
- `server-events-XX` - Server event API (14)
- `client-events-XX` - Client event API (13)
- `world-integration-XX` - ECS integration (9)

### Appendix B: Test Harness Quick Reference

```rust
// Setup
let mut scenario = Scenario::new();
scenario.server_start(ServerConfig::default(), protocol());
let room = scenario.mutate(|ctx| ctx.server(|s| s.make_room().key()));
let client = client_connect(&mut scenario, &room, "name", auth, config, protocol);

// Mutation phase
scenario.mutate(|ctx| {
    ctx.server(|server| { /* server operations */ });
    ctx.client(client_key, |client| { /* client operations */ });
});

// Expectation phase
scenario.expect(|ctx| {
    let condition = ctx.server(|s| /* check */);
    condition.then_some(())
});

// Extended timeout
scenario.until(200.ticks()).expect(|ctx| { /* ... */ });
```

### Appendix C: Spec Template

```markdown
### [spec-name-NN] — Short Title

**Guarantee:** One sentence invariant statement using RFC 2119 keywords.

**Preconditions:**
- Condition A MUST be true
- Condition B MUST be true

**Postconditions:**
- Effect X MUST occur
- Effect Y MUST be observable

**Test Scenarios:**

**Scenario spec-name-NN.t1: Descriptive name**
- **Given:** Initial state
- **When:** Trigger action
- **Then:** Expected outcome

**Covered by tests:**
- `test/tests/file.rs::test_function_name`
```

### Appendix D: Command Cheatsheet

```bash
# Spec operations
./spec_tool.sh lint           # Validate spec format
./spec_tool.sh validate       # Full validation
./spec_tool.sh bundle         # Generate NAIA_SPECS.md
./spec_tool.sh registry       # Generate CONTRACT_REGISTRY.md
./spec_tool.sh stats          # Show statistics
./spec_tool.sh check-orphans  # Find untracked MUSTs
./spec_tool.sh check-refs     # Validate cross-references

# Test operations
cargo test --package naia-test                    # Run all E2E tests
cargo test --package naia-test <test_name>        # Run specific test
cargo test --package naia-test -- --nocapture     # Show output
cargo test --workspace                            # Run all tests

# Quality checks
cargo clippy --no-deps                            # Lint
cargo fmt -- --check                              # Format check
```

---

## Changelog

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-01-11 | Initial comprehensive plan |

---

*This document is the canonical reference for Naia's spec-driven development process. All contributors MUST follow this process. If the process is found deficient, update this document first, then change the process.*
