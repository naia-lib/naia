# CLAUDE.md

Naia is a cross-platform Rust networking engine for multiplayer games. Architecture follows the [Tribes 2 Networking model](https://www.gamedevs.org/uploads/tribes-networking-model.pdf).

---

## Authority

**This document is authoritative for:**
- Agent/executor contract: read PLAN.md, write OUTPUT.md every session
- Session startup protocol and health check commands
- Token optimization strategies and tool usage patterns
- Quick reference for commands, test structure, and debugging

**Defer to:**
- `DEV_PROCESS.md` for complete SDD methodology and human workflow
- `specs/README.md` for authoritative tool commands and spec authoring rules
- `PLAN.md` (read this first every session) for current goals and constraints
- `SPEC_CERTIFICATION_PLAN.md` for one-time certification process details

---

## Current Status (NOT CANONICAL — READ PLAN/OUTPUT)

**Do not trust this file for "what phase we're in."** Status lives in:
- `_AGENTS/PLAN.md` (current marching orders + DoD)
- `_AGENTS/OUTPUT.md` (what actually happened + evidence)

As of 2026-01-13, the top priority is **certification-readiness (Phase A2 mechanics)**:
- Keep `cargo run -p naia_spec_tool -- validate` GREEN
- Drive `cargo run -p naia_spec_tool -- adequacy --strict` to GREEN
- Dry-run packets for sanity
- Only then do semantic fan-out

Runtime pass rate is secondary until certification gates are complete.

## Test File Organization (1:1 Mapping)

Test files map directly to spec files for instant traceability:

```
specs/contracts/0N_domain.spec.md  →  test/tests/0N_domain.rs
```

| Spec | Test File |
|------|-----------|
| `01_connection_lifecycle.spec.md` | `01_connection_lifecycle.rs` |
| `02_transport.spec.md` | `02_transport.rs` |
| `03_messaging.spec.md` | `03_messaging.rs` |
| ... | ... |
| `14_world_integration.spec.md` | `14_world_integration.rs` |

**To find tests for a contract:** Open the matching numbered test file

## Session Startup Protocol (PLAN/OUTPUT Convention)

**CRITICAL: Every session follows the PLAN → EXECUTE → OUTPUT cycle.**

**At session start:**
1. **Read `_AGENTS/PLAN.md` first** (REQUIRED - contains current goal, constraints, exact commands)
2. Optional health check: `cargo run -p naia_spec_tool -- verify --contract <id>` or `coverage`
3. Begin work following the plan

**At session end (or when stopping for user review):**
1. **Write `_AGENTS/OUTPUT.md`** (REQUIRED - commands run, results, diffs, next steps)
2. Include: git diff --stat, file changes, key excerpts, open questions
3. The OUTPUT becomes the handoff artifact for next session or human review

**Structure:**
- `PLAN.md` = Input (what to do, how to do it, when to stop)
- `OUTPUT.md` = Output (what was done, what changed, what's next)

**Phase B tip:** Use `verify --contract <id>` for fast feedback on the contract you're fixing.

## Spec-Driven Development (CRITICAL)

**Specs are the source of truth.** If implementation differs from spec, the implementation is wrong.

```
specs/contracts/*.md (contracts) → test/tests/*.rs (E2E tests) → Implementation
```

### Development Phases (certification-first)

**Phase A (Certification):**
- A1: Policy B obligations exist for every contract (at least `t1`)
- A2: Mechanical adequacy: `validate` GREEN and `adequacy --strict` GREEN
- A3: Semantic certification via packets (expensive; do last)

**Phase B (Implementation hardening):**
- After certification is complete, fix implementation until tests pass.

**Key insight:** A `todo!()` in a test is a **specification gap**, not an implementation bug. Write what you *expect* to happen, and let the test fail if the implementation is wrong.

**The SDD Loop (Phase B - Current):**
1. **RUN**: Run tests, identify failures by type (panic location, timeout, assertion)
2. **DIAGNOSE**:
   - Panic at scenario.rs:155/213 → Test structure issue (mutate/expect violation)
   - Timeout → Implementation gap or wrong assertion
   - Assertion failure → Logic bug
3. **FIX**: Fix root cause (never hack around framework violations)
4. **VERIFY**: Test passes, no regressions

## Essential Commands

```bash
# Spec operations (run from project root)
cargo run -p naia_spec_tool -- verify                      # Full verification pipeline (specs + tests + coverage)
cargo run -p naia_spec_tool -- verify --contract <id>      # Fast: test only one contract
cargo run -p naia_spec_tool -- coverage                    # Check contract test coverage
cargo run -p naia_spec_tool -- packet <id>                 # Generate adequacy review packet
cargo run -p naia_spec_tool -- packet <id> --full-tests    # Generate packet with full test code
cargo run -p naia_spec_tool -- lint                        # Validate specs only

# Testing
cargo test --package naia-test                        # All E2E tests
cargo test --package naia-test --test <file>          # Single test file
cargo test --package naia-test <test_name>            # Single test
cargo test --package naia-test <test_name> -- --nocapture  # With output

# Debugging (detailed network event tracing)
cargo test --package naia-test --features e2e_debug <test_name> -- --nocapture

# Quality gates
cargo clippy --no-deps && cargo fmt -- --check

# Tool development
cargo test -p naia_spec_tool                 # Run naia_spec_tool self-tests
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

### Critical: mutate() vs expect() (NEVER VIOLATE THIS)

**Purpose:**
- `mutate()` - Change state (spawn entities, send messages, modify components)
- `expect()` - Wait/poll until a condition is true (replication, connection, state changes)

**ANTI-PATTERNS (NEVER DO THIS):**

❌ **Adding empty expect() between mutate() calls:**
```rust
scenario.mutate(|ctx| { /* get tick */ });
scenario.expect(|_| Some(()));  // ← WRONG! Empty wait does nothing!
scenario.mutate(|ctx| { /* send message */ });
```

❌ **Using mutate() to read state (not mutate):**
```rust
let tick = scenario.mutate(|ctx| {
    ctx.client(key, |c| c.client_tick())  // ← WRONG! This is a query, not mutation!
});
scenario.mutate(|ctx| { /* use tick */ });
```

❌ **Sequential empty mutations:**
```rust
scenario.mutate(|_| {});  // ← WRONG! Does nothing!
scenario.expect(|_| Some(()));
```

✅ **CORRECT: Merge sequential mutate() calls:**
```rust
let tick = scenario.mutate(|ctx| {
    ctx.client(key, |client| {
        let tick = client.client_tick();  // Read and use in same block
        client.send_message(&tick, &msg);
        tick  // Return what you need
    })
});
```

**If you see sequential `mutate()` calls, ask:**
1. Can these be merged into one mutate() block? (Usually YES)
2. Is one of them just reading state? (Merge it with the next mutation)
3. Is one of them empty/no-op? (Delete it)

**The framework will panic on `mutate()` → `mutate()` violations. This is intentional. Don't work around it with empty expect() calls - fix the test structure.**

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
| `PLAN.md` | **Current goal, constraints, exact commands** | **Every session (READ FIRST)** |
| `OUTPUT.md` | **Session results, diffs, next steps** | **Every session (WRITE AT END)** |
| `DEV_PROCESS.md` | Full SDD process, tooling, patterns | Complex tasks |
| `specs/README.md` | Tool commands, spec authoring rules | When using naia_spec_tool |
| `specs/generated/CONTRACT_REGISTRY.md` | All contract IDs indexed | Finding contracts |
| `specs/generated/TRACEABILITY.md` | Contract↔test mapping | Checking coverage |
| `specs/generated/GAP_ANALYSIS.md` | Prioritized uncovered contracts | Planning work |

## Historical Notes (may be stale — defer to PLAN/OUTPUT)

**Example past issues (do not assume still true):**
- File: `08_entity_ownership.rs`
- Problem: Sequential `mutate()` calls without `expect()` between them
- Fix: Merge operations or add proper `expect()` with real condition

**Example past timeouts:**
- Files: `03_messaging.rs` (4), `06_entity_scopes.rs` (4)
- Debug with: `cargo test --features e2e_debug <test> -- --nocapture`

**Example past delegation/authority bugs:**
- Files: `10_entity_delegation.rs` (10), `11_entity_authority.rs` (5)
- Likely real state machine bugs - investigate after structure fixes

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

**Phase B: Fix implementation (CURRENT)**
1. Run `cargo test --package naia-test` to see all failures
2. Pick a failing test
3. Understand what the test expects (read the spec)
4. Fix implementation to match spec
5. Verify test passes
6. Run 3x for flakiness

**Phase 3: Adequacy review (optional)**
1. Run `cargo run -p naia_spec_tool -- packet <contract-id>`
2. Paste packet into LLM for adequacy review
3. Map spec guarantees/preconditions/postconditions to test assertions
4. Add `expect_msg` labels for deterministic review
5. Verify with `cargo run -p naia_spec_tool -- verify --contract <id>`

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

**Session protocol (MUST follow):**
- MUST read `PLAN.md` at session start
- MUST write `OUTPUT.md` at session end (or when stopping)
- MUST NOT commit, branch, rebase, or push to git (human does this)
- Stop on uncertainty and report in OUTPUT.md

**Technical constraints:**
- Demos excluded from workspace (feature conflicts)
- Wasm: use `wbindgen` OR `mquad`, not both
- All tick math must be wrap-safe (u16 wraps at 65535)
- Specs use RFC 2119: MUST/MUST NOT/MAY/SHOULD/SHALL
- No timing hacks in tests - use `expect()` polling pattern
