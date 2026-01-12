# Naia Development Plan

**Status:** Active - Phase A (Complete Test Coverage)
**Updated:** 2026-01-11
**Goal:** All spec contracts have compiling E2E tests

---

## Two-Phase Development Process

**Phase A: Complete Test Coverage (CURRENT)**
- Every spec contract has a compiling E2E test
- Tests MUST compile with NO `todo!()` macros
- Tests are allowed to FAIL - that indicates implementation gaps
- Goal: 236/236 contracts covered, zero `todo!()`

**Phase B: Fix Implementation (BLOCKED until Phase A complete)**
- Run all tests, observe failures
- Systematically fix implementation
- Failing tests are the bug tracker

**Key insight:** A `todo!()` in a test is a **specification gap**, not an implementation bug. Write what you *expect* to happen, and let the test fail.

---

## Current State

| Metric | Value | Target |
|--------|-------|--------|
| Contracts with compiling tests | **185/236 (78%)** | 236/236 (100%) |
| Tests with `todo!()` | **0** | 0 |
| Uncovered contracts | **51** | 0 |
| Phase A | **IN PROGRESS** | - |

---

## Immediate Next Actions (Phase A)

### Priority 1: Write Tests for New Spec Contracts (51 uncovered)

The spec suite was expanded on 2026-01-11 with stronger contracts. These need E2E tests:

**Command Sequence (4 contracts) → `04_time_ticks_commands.rs`**
- `commands-03a`: Command sequence is required (varint encoded)
- `commands-03b`: Server applies commands in sequence order
- `commands-03c`: Command cap per tick (`MAX_COMMANDS_PER_TICK_PER_CONNECTION = 64`)
- `commands-03d`: Duplicate `(tick, sequence)` commands are dropped

**Protocol Identity (6 contracts) → `01_connection_lifecycle.rs`**
- `connection-14a`: protocol_id check during handshake
- `connection-28`: Reconnect is fresh session
- `connection-29`: protocol_id definition
- `connection-30`: protocol_id wire encoding (u128 little-endian)
- `connection-31`: protocol_id handshake gate (`ProtocolMismatch` error)
- `connection-32`: What affects protocol_id
- `connection-33`: No partial compatibility

**Common/Cross-cutting (15 contracts) → `00_common.rs` (NEW FILE)**
- `common-01`: User-initiated misuse returns `Result::Err`
- `common-02`: Remote/untrusted input MUST NOT panic
- `common-02a`: Protocol mismatch is deployment error
- `common-03`: Framework invariant violations MUST panic
- `common-04`: Warnings are debug-only and non-normative
- `common-05`: Determinism under deterministic inputs
- `common-06`: Per-tick determinism rule
- `common-07`: Tests MUST NOT assert on logs
- `common-08`: Test obligation template
- `common-09`: Observable signals subsection
- `common-10`: Fixed invariants are locked
- `common-11`: Configurable defaults
- `common-11a`: New constants start as invariants
- `common-12`: Internal measurements vs exposed metrics
- `common-12a`: Test tolerance constants
- `common-13`: Metrics are non-normative for gameplay
- `common-14`: Reconnect is fresh session

**Entity Ownership (14 contracts) → `08_entity_ownership.rs`**
- `entity-ownership-01` through `entity-ownership-14`
- These were reformatted with proper headers; need dedicated tests

**Observability (1 contract) → `05_observability_metrics.rs`**
- `observability-01a`: Internal measurements vs exposed metrics

### Priority 2: Test File Creation Order

1. **`00_common.rs`** - Create new test file for cross-cutting common contracts
2. **`04_time_ticks_commands.rs`** - Add command sequence tests
3. **`01_connection_lifecycle.rs`** - Add protocol_id tests
4. **`08_entity_ownership.rs`** - Add entity ownership tests
5. **`05_observability_metrics.rs`** - Add observability-01a test

### Priority 3: For Each Contract

1. Read the contract in `specs/contracts/`
2. Write a compiling test with `/// Contract: [contract-id]` annotation
3. Test should assert what the spec requires
4. Test is allowed to FAIL (implementation gap)
5. Run `cargo test --package naia-test --test <file> --no-run` to verify compilation
6. Run `./specs/spec_tool.sh coverage` to verify annotation

---

## Recent Spec Changes (2026-01-11)

### 1. Protocol Identity Hardening
- Defined `protocol_id` as deterministic 128-bit identifier
- Wire encoding: u128 little-endian (16 bytes)
- Handshake gate: `protocol_id` comparison before any other checks
- New `ProtocolMismatch` error for deployment configuration errors

### 2. Command Sequence Locking
- `sequence` required on every command (varint encoded)
- Server applies in sequence order regardless of arrival
- `MAX_COMMANDS_PER_TICK_PER_CONNECTION = 64` invariant
- Duplicate `(tick, sequence)` dropped

### 3. Channel Compatibility Simplification
- All compatibility enforced via `protocol_id` gate
- No runtime channel compatibility checks
- messaging-04 updated to reference protocol_id

### 4. Metrics Testability
- Tests MAY assert on metrics
- Tests MUST NOT assert on logs
- RTT/jitter: inequality-style assertions only

### 5. Error Taxonomy
- Added `common-02a`: Protocol mismatch classification
- Updated Error/Failure Mode Summary table
- Clarified: panic reserved for internal invariants only

### 6. Tool Update
- `spec_tool.sh` now supports alphanumeric contract suffixes (e.g., `-03a`)

---

## Phase A Progress

| Task | Status |
|------|--------|
| Annotate all existing tests | **DONE** |
| Gap analysis | **DONE** |
| Write tests for original 185 contracts | **DONE** |
| Eliminate `todo!()` macros | **DONE** |
| Spec hardening (2026-01-11) | **DONE** |
| Write tests for 51 new contracts | **TODO** |
| **Phase A Complete** | **BLOCKED** |

---

## Test Files (1:1 Spec Mapping)

| Spec File | Test File | Contracts | Status |
|-----------|-----------|-----------|--------|
| `0_common.md` | `00_common.rs` | 15 | **NEW FILE NEEDED** |
| `1_connection_lifecycle.md` | `01_connection_lifecycle.rs` | 33 | +6 needed |
| `2_transport.md` | `02_transport.rs` | 5 | Covered |
| `3_messaging.md` | `03_messaging.rs` | 27 | Covered |
| `4_time_ticks_commands.md` | `04_time_ticks_commands.rs` | 17 | +4 needed |
| `5_observability_metrics.md` | `05_observability_metrics.rs` | 11 | +1 needed |
| `6_entity_scopes.md` | `06_entity_scopes.rs` | 15 | Covered |
| `7_entity_replication.md` | `07_entity_replication.rs` | 12 | Covered |
| `8_entity_ownership.md` | `08_entity_ownership.rs` | 14 | +14 needed |
| `9_entity_publication.md` | `09_entity_publication.rs` | 11 | Covered |
| `10_entity_delegation.md` | `10_entity_delegation.rs` | 17 | Covered |
| `11_entity_authority.md` | `11_entity_authority.rs` | 16 | Covered |
| `12_server_events_api.md` | `12_server_events_api.rs` | 14 | Covered |
| `13_client_events_api.md` | `13_client_events_api.rs` | 13 | Covered |
| `14_world_integration.md` | `14_world_integration.rs` | 9 | Covered |

**To find tests for a contract:** Open the matching numbered test file

---

## Completion Checklist

```
[x] Annotate all existing tests with contract IDs
[x] Generate gap analysis
[x] Write tests for original 185 contracts
[x] Eliminate ALL todo!() macros

--- PHASE A: Complete Test Coverage (CURRENT) ---
[x] Harden specs with protocol_id, command sequence, etc.
[ ] Create 00_common.rs test file
[ ] Write tests for 4 command sequence contracts
[ ] Write tests for 6 protocol_id contracts
[ ] Write tests for 14 entity ownership contracts
[ ] Write tests for 1 observability contract
[ ] Write tests for 15 common contracts
[ ] Verify: spec_tool.sh coverage shows 236/236
[ ] Verify: grep -r "todo!" returns nothing
[ ] Verify: cargo test --package naia-test --no-run succeeds

--- PHASE B: Fix Implementation (BLOCKED until above complete) ---
[ ] Run all tests, collect failures
[ ] Prioritize fixes by importance
[ ] Fix implementation systematically
[ ] All tests pass
[ ] Run 3x for flakiness
```

---

## Session Workflow

```bash
# Start of session
./specs/spec_tool.sh coverage          # Check contracts with tests (target: 236/236)
grep -r "todo!" test/tests/*.rs        # Find incomplete tests (target: 0)

# Phase A work: Write compiling tests for uncovered contracts
# Example: writing tests for commands-03a through commands-03d
vim test/tests/04_time_ticks_commands.rs
cargo test --package naia-test --test 04_time_ticks_commands --no-run  # Must compile

# Verify coverage improved
./specs/spec_tool.sh coverage

# Debugging (if needed)
cargo test --package naia-test --features e2e_debug <test_name> -- --nocapture

# End of session
./specs/spec_tool.sh coverage          # Verify annotation coverage
grep -r "todo!" test/tests/*.rs        # Verify no incomplete tests
```

---

## Key Files Reference

| File | Purpose |
|------|---------|
| `CLAUDE.md` | Session instructions, quick reference |
| `PLAN.md` | This file - current state, next actions |
| `DEV_PROCESS.md` | Full SDD methodology |
| `specs/generated/CONTRACT_REGISTRY.md` | All 236 contract IDs |
| `specs/generated/TRACEABILITY.md` | Contract↔test mapping |

---

## Quality Rules for Tests

**Phase A requirements (test coverage):**
1. **Must compile** - No `todo!()` macros, actual assertions
2. **Contract annotations required** - Every test needs `/// Contract: [id]`
3. **Minimal assertions** - Test exactly what the contract specifies
4. **Failures are OK** - Test documents implementation gap

**Phase B requirements (implementation fixes):**
1. **No timing hacks** - Use `expect()` polling, not `sleep()`
2. **Reordering-tolerant** - Tests must pass regardless of async message order
3. **Run 3x** - Verify no flakiness before marking complete
4. **No regressions** - Full test suite must still pass

---

## Session History

### 2026-01-11 (Current)
- **Spec hardening:** Added 51 new contracts
  - Protocol identity (`protocol_id`) with u128 little-endian encoding
  - Command sequence with `MAX_COMMANDS_PER_TICK_PER_CONNECTION = 64`
  - Error taxonomy with `ProtocolMismatch` error type
  - Metrics testability with inequality-style assertions
- **Tool fix:** Updated `spec_tool.sh` to support alphanumeric contract suffixes
- **Status:** Phase A incomplete (185/236 contracts covered)

### Previous Sessions
- Completed tests for original 185 contracts
- Fixed entity-authority-11/12 implementation bug
- Added e2e_debug documentation
- Adopted two-phase development process
