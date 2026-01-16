# SYSTEM.md — Naia Repository Overview & Agent Instructions

## 1. Repository Purpose & Architecture
**Naia** is a cross-platform networking engine for methods multiplayer games in Rust. It follows a client-server authoritative model with:
- **Protocol**: Shared schema for Messages and Components.
- **Transports**: Abstracted network layers (UDP, WebRTC, etc via `naia-socket`).
- **Replication**: Entity scoping, diffing, and syncing (Server -> Client).
- **Events**: Event-based API for connection lifecycle and data updates.

### Key Workspaces
- `client/`, `server/`, `shared/`: The core engine crates.
- `adapters/`: engine-specific integrations (`bevy`, `hecs`).
- `sockets/`: low-level transport implementations.
- `specs/`: **Source of Truth** for behavior contracts.
- `test/`: Integration test suite (The "Test Harness").

## 2. Spec-Driven Development (SDD) Workflow
Naia uses a rigorous SDD process governed by the `naia_spec_tool`.

### The "Contract" is Law
Behavior is defined in **Markdown Contracts** located in `specs/contracts/`.
- Format: `[id] ... description ...`
- Example: `[connection-01] Clients transition through connect states`
- Files: `01_connection_lifecycle.spec.md`, etc.

### Verification Layer (`naia/test`)
The `naia-test` crate (`test/`) serves as the compliance suite.
- **Manual Mapping**: Tests in `test/tests/` map 1:1 to spec files.
- **Traceability**: Tests annotate coverage via comments: `Contract: [connection-01]`.
- **Harness**: Uses `Scenario` (in `test/src/harness`) to simulate Client/Server interactions in-memory without network IO (using `naia-shared`'s local transport features).

### Tooling (`naia_spec_tool`)
Located in `specs/src`, this tool manages the SDD lifecycle.
- `cargo run -p naia_spec_tool -- verify`: Runs the configured checks (lint, coverage, execution).
- `cargo run -p naia_spec_tool -- traceability`: Generates the matrix of Spec <-> Test coverage.

**Agent Instruction**: NEVER modify behavior without first checking the relevant `specs/contracts/*.spec.md`. If the spec changes, update the contract text and the corresponding test before implementation.

## 3. Namako Integration Status
*Context: The global workspace docs mention Namako (cucumber fork) and NPAP (adapter protocol).*

**Current State (2026-01-16):**
- **No active Namako code found**: There are no `.feature` files, `#[given]/#[when]` macros, or `npap` adapter binaries currently visible in the file tree.
- **Proto-Namako**: The `naia-test` `Scenario` harness acts as the logical precursor to a Namako adapter. It provides the "World" state and step-like manipulations, but is currently driven by standard Rust `#[test]` functions.
- **Future Direction**: Expect future tasks to involve wrapping the `Scenario` harness into a Namako Adapter to support Gherkin-based execution.

## 4. Agent Operating Instructions

### A. How to Implement a New Feature
1.  **Draft the Contract**: Create/Edit `specs/contracts/XX_feature.spec.md`. Define `[feature-XX]` obligations.
2.  **Scaffold the Test**: Create/Edit `test/tests/XX_feature.rs`. Add `Contract: [feature-XX]` comments.
3.  **Implement Logic**: Modify `client/`, `server/`, `shared/` to satisfy the test.
4.  **Verify**: Run `cargo run -p naia_spec_tool -- verify`.

### B. How to Fix a Bug
1.  **Reproduce in Test**: Find the relevant `test/tests/` file. Add a failing test case.
2.  **Check Spec**: Does the spec cover this edge case? If not, **Update the Spec** first.
3.  **Fix & Verify**: Fix the code, ensure the test passes, and `naia_spec_tool` is happy.

### C. Critical Files & Locations
- **Contracts**: `specs/contracts/*.spec.md` (READ FIRST)
- **Tests**: `test/tests/*.rs`
- **Harness**: `test/src/harness/` (Scenario logic)
- **Core Logic**:
    - `client/src/protocol/`: Message/Event handling
    - `server/src/room/`: Scope/Room logic
    - `shared/src/connection/`: Packet/Ack logic

## 5. Learnings & Best Practices
- **Do not guess behavior**: If code and spec disagree, **Spec wins** (but check if spec is outdated).
- **Traceability is mandatory**: Every test MUST cite a Contract ID.
- **Local Transport**: The test harness uses direct channel-based communication, bypassing sockets. Issues with real sockets (WebRTC/UDP) might not be caught here (check `socket/` tests if applicable).
