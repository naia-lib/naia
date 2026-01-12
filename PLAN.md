# Naia Development Plan

**Status:** Active
**Created:** 2026-01-11
**Goal:** Get the spec-driven agentic development flywheel running

---

## Executive Summary

We have **185 contracts** in specs and **149 tests** in the harness, but **0% traceability** between them. The highest-leverage work is establishing this connection, then systematically filling gaps.

**The Flywheel:**
```
Contracts → Annotated Tests → Coverage Metrics → Gap Identification → New Tests → Implementation
     ↑                                                                                    ↓
     └────────────────────── Spec Refinements ←──────────────────────────────────────────┘
```

---

## Phase 1: Establish Traceability (Highest Leverage)

**Goal:** Connect existing tests to contracts so coverage tracking works.

### 1.1 Annotate Existing Tests with Contract IDs

**Priority:** P0 - Unblocks all coverage tracking

Each of the 149 existing tests needs a `/// Contract: [contract-id]` annotation. This is mechanical but critical.

**Approach:**
1. Analyze each test file to determine which contracts it covers
2. Add contract annotations to test function doc comments
3. Some tests may cover multiple contracts (annotate all)
4. Some contracts may have no tests (note for Phase 3)

**Files to process (by domain):**

| File | Est. Tests | Primary Contracts |
|------|------------|-------------------|
| `connection_auth_identity.rs` | ~10 | `connection-XX` |
| `entities_lifetime_identity.rs` | ~12 | `entity-replication-XX`, `entity-scopes-XX` |
| `entity_authority_client_ops.rs` | ~8 | `entity-authority-XX` |
| `entity_authority_server_ops.rs` | ~8 | `entity-authority-XX` |
| `entity_client_owned.rs` | ~15 | `entity-ownership-XX`, `entity-publication-XX` |
| `entity_delegation_toggle.rs` | ~12 | `entity-delegation-XX` |
| `entity_migration_and_events.rs` | ~5 | `entity-delegation-XX`, `server-events-XX` |
| `entity_scope_coupling.rs` | ~4 | `entity-scopes-XX` |
| `events_world_integration.rs` | ~12 | `server-events-XX`, `client-events-XX`, `world-integration-XX` |
| `messaging_channels.rs` | ~25 | `messaging-XX` |
| `rooms_scope_snapshot.rs` | ~15 | `entity-scopes-XX`, `entity-replication-XX` |
| `time_ticks_transport.rs` | ~15 | `time-XX`, `commands-XX`, `transport-XX` |
| `protocol_schema_versioning.rs` | ~8 | `messaging-XX` (protocol compat) |
| `integration_transport_parity.rs` | ~5 | `transport-XX` |

**Validation:**
```bash
./spec_tool.sh coverage  # Target: >50% after Phase 1
```

### 1.2 Generate Initial Traceability Matrix

**Priority:** P0 - Provides visibility

After annotations are added:
```bash
./spec_tool.sh traceability
```

Review `specs/TRACEABILITY.md` to identify:
- Contracts with multiple tests (good)
- Contracts with no tests (Phase 3 targets)
- Tests covering no contracts (may need spec additions)

### 1.3 Update Spec Test Obligations Sections

**Priority:** P1 - Completes bidirectional traceability

For each spec file, fill the Test Obligations section:

```markdown
## Test Obligations

| Contract | Test | File |
|----------|------|------|
| [entity-scopes-01] | entities_only_replicate_when_room_scope_match | rooms_scope_snapshot.rs |
```

---

## Phase 2: Gap Analysis

**Goal:** Identify which contracts have no test coverage.

### 2.1 Run Coverage Analysis

```bash
./spec_tool.sh coverage > coverage_report.txt
```

Categorize uncovered contracts:

| Category | Action |
|----------|--------|
| Core behavior contracts | Must have tests (Phase 3) |
| Edge case contracts | Should have tests (Phase 3) |
| Documentation-only contracts | May not need tests |
| API contracts | Verify via existing integration tests |

### 2.2 Prioritize by Risk

High-risk contracts to test first:
1. State machine transitions (connection, authority, delegation)
2. Scope/visibility predicates
3. Event ordering guarantees
4. Error handling contracts

Lower-risk (can defer):
1. Observability/metrics contracts
2. Debug-mode-only behavior
3. Configuration defaults

---

## Phase 3: Fill Coverage Gaps

**Goal:** Write tests for uncovered high-priority contracts.

### 3.1 Generate Test Skeletons

For each uncovered contract:
```bash
./spec_tool.sh gen-test <contract-id> >> test/tests/<domain>.rs
```

### 3.2 Implement Tests by Domain

**Order by dependency (test foundations first):**

1. **Connection lifecycle** (`connection-XX`)
   - Foundation for all other tests
   - Auth, handshake, disconnect semantics

2. **Entity scopes** (`entity-scopes-XX`)
   - Visibility predicates
   - Room membership effects

3. **Entity replication** (`entity-replication-XX`)
   - Spawn/despawn semantics
   - Component sync

4. **Messaging** (`messaging-XX`)
   - Channel guarantees
   - Ordering semantics

5. **Authority/Delegation** (`entity-authority-XX`, `entity-delegation-XX`)
   - Complex state machines
   - Multi-client coordination

6. **Events API** (`server-events-XX`, `client-events-XX`)
   - Event ordering
   - Drain semantics

7. **Observability** (`observability-XX`)
   - Metrics convergence
   - May need tolerance-based assertions

### 3.3 Validate Coverage Improvement

After each domain:
```bash
cargo test --package naia-test
./spec_tool.sh coverage
```

**Target:** 80%+ contract coverage

---

## Phase 4: Spec Refinements

**Goal:** Improve specs based on test-writing insights.

### 4.1 Fix Discovered Ambiguities

During test writing, note any spec ambiguities:
- Unclear preconditions
- Missing edge cases
- Conflicting contracts

Update specs with clarifications.

### 4.2 Add Given/When/Then Scenarios

Enhance contracts with explicit test scenarios:

```markdown
### [entity-scopes-07] — Room removal causes despawn

**Scenario 1:** Single shared room
- Given: E in Room R, U in Room R, InScope(U, E)
- When: Server removes E from R
- Then: OutOfScope(U, E), despawn event on U's client
```

### 4.3 Resolve Remaining Orphan MUST Statements

Current: 24 orphan statements
Target: 0 orphans

Either:
- Associate with existing contracts
- Create new contracts
- Determine they're not normative

---

## Phase 5: Automation & CI

**Goal:** Prevent regression, enforce SDD process.

### 5.1 CI Pipeline for Specs

```yaml
# .github/workflows/specs.yml
- run: cd specs && ./spec_tool.sh lint
- run: cd specs && ./spec_tool.sh check-refs
- run: cd specs && ./spec_tool.sh coverage
  # Fail if coverage drops below threshold
```

### 5.2 Pre-commit Hooks

```bash
# Regenerate artifacts on spec changes
if git diff --cached --name-only | grep -q "specs/.*\.md"; then
  ./spec_tool.sh bundle
  ./spec_tool.sh registry
fi
```

### 5.3 PR Template

Require for all PRs:
- [ ] Contracts affected: `[contract-id]` or "None"
- [ ] Tests added/updated: `test_name` or "N/A"
- [ ] Spec changes: Yes/No
- [ ] `./spec_tool.sh lint` passes

---

## Phase 6: Implementation Work

**Goal:** Use the flywheel for actual feature development.

### 6.1 Process for New Features

1. Draft contracts in appropriate spec
2. Run `./spec_tool.sh lint`
3. Generate test skeletons with `gen-test`
4. Implement tests (should fail)
5. Implement feature code
6. Verify tests pass
7. Update spec Test Obligations
8. Run full validation

### 6.2 Process for Bug Fixes

1. Identify relevant contract
2. Write failing test that reproduces bug
3. Fix implementation
4. Verify test passes
5. Note any spec clarifications needed

---

## Metrics & Milestones

| Milestone | Metric | Target | Status |
|-----------|--------|--------|--------|
| Phase 1 complete | Test annotations | 154/154 | **Done** (all 14 test files, 154 tests annotated) |
| Phase 1 complete | Coverage tracking | >50% | **Done** (55% = 102/185 contracts) |
| Phase 2 complete | Gap analysis | Complete | **Done** (see specs/GAP_ANALYSIS.md) |
| Phase 3 progress | Contract coverage | >80% | **In Progress** (57% = 105/185) - Micro-batch #1 done |
| Phase 4 complete | Orphan statements | 0 | Pending |
| Phase 5 complete | CI pipeline | Active | Pending |

---

## Quick Reference: Daily Workflow

```bash
# Start of session
cd /home/ccarpenter/Personal/naia
./specs/spec_tool.sh coverage  # Check current state

# Working on tests
./specs/spec_tool.sh gen-test <contract-id>  # Generate skeleton
cargo test --package naia-test <test_name>   # Run test

# End of session
./specs/spec_tool.sh lint      # Validate specs
./specs/spec_tool.sh coverage  # Measure progress
```

---

## Files Modified in This Plan

| File | Purpose |
|------|---------|
| `PLAN.md` | This plan |
| `CLAUDE.md` | Session instructions |
| `NAIA_SPEC_DRIVEN_DEVELOPMENT.md` | Full SDD process documentation |
| `specs/spec_tool.sh` | CLI with coverage, gen-test, traceability |
| `specs/PLAN.md` | Previous spec-focused plan (superseded) |
| `specs/CONTRACT_REGISTRY.md` | Contract index |
| `specs/TRACEABILITY.md` | Contract↔test matrix (to be generated) |
| `test/tests/*.rs` | Test files to annotate |
