# SYSTEM.md — Naia Repository Overview & Agent Instructions

## 1. Repository Purpose & Architecture
**Naia** is a cross-platform networking engine for multiplayer games in Rust. It follows a client-server authoritative model with:
- **Protocol**: Shared schema for Messages and Components.
- **Transports**: Abstracted network layers (UDP, WebRTC, etc via `naia-socket`).
- **Replication**: Entity scoping, diffing, and syncing (Server -> Client).
- **Events**: Event-based API for connection lifecycle and data updates.

### Key Workspaces
- `client/`, `server/`, `shared/`: The core engine crates.
- `adapters/`: engine-specific integrations (`bevy`, `hecs`).
- `sockets/`: low-level transport implementations.
- `test/specs/`: **Source of Truth** - Gherkin feature specifications.
- `test/tests/`: Step bindings implementing the specs.
- `test/npa/`: Namako Protocol Adapter (test runner).
- `test/harness/`: Test harness with Scenario abstraction.

## 2. Spec-Driven Development (SDD) with Namako + Tesaki

### Quick Start (for new agents)

```bash
# Check current status
cd naia
namako lint --adapter-cmd "cargo run --manifest-path test/npa/Cargo.toml --" --specs-dir test/specs

# Run autonomous development loop
tesaki
> loop 10   # Runs up to 10 missions autonomously
```

### The SDD Stack
1. **Namako** - Measures truth (parses specs, runs tests, produces packets)
2. **Tesaki** - Drives development (selects missions, invokes runners, validates)
3. **Runner** - Executes missions (Copilot CLI, Claude Code, Codex)

### Configuration
```toml
# naia/.tesaki/config.toml
specs_dir = "test/specs"
adapter_cmd = "cargo run --manifest-path test/npa/Cargo.toml --"
runner = "copilot"
planner = "copilot"
max_retries = 0
```

## 3. Current State (2026-02-02)

| Metric | Value |
|--------|-------|
| Feature files | 17 |
| Executable scenarios | 46 |
| Total steps | 210 |
| Missing bindings | **0** ✅ |
| Lint status | PASS |

### Key Files
- **Specs**: `test/specs/features/*.feature`
- **Bindings**: `test/tests/src/steps/*.rs`
- **Harness**: `test/harness/src/`
- **Adapter**: `test/npa/`

## 4. Agent Operating Instructions

### A. Adding New Behavior
1. **Write the spec first**: Add/edit `test/specs/features/*.feature`
2. **Run lint**: `namako lint ...` to see missing bindings
3. **Add bindings**: Create step functions in `test/tests/src/steps/`
4. **Verify**: `namako gate ...` should pass

### B. Using Tesaki Autonomous Mode
```bash
tesaki
> loop 5   # Run 5 missions
> exit
```

Or interactively:
```bash
tesaki
> propose a mission
> run it
```

### C. Manual Commands
```bash
# Lint (check bindings)
namako lint --adapter-cmd "cargo run --manifest-path test/npa/Cargo.toml --" --specs-dir test/specs

# Full gate (lint + run + verify)
namako gate --adapter-cmd "cargo run --manifest-path test/npa/Cargo.toml --" --specs-dir test/specs
```

## 5. Step Binding Pattern

```rust
// test/tests/src/steps/transport.rs
use cucumber::{given, when, then};
use crate::world::NaiaWorld;

#[given("a client connection request")]
async fn client_connection_request(world: &mut NaiaWorld) {
    world.client.connect();
}

#[when("the server accepts the connection")]
async fn server_accepts(world: &mut NaiaWorld) {
    world.server.accept_connection();
}

#[then("the client should be connected")]
async fn client_connected(world: &mut NaiaWorld) {
    assert!(world.client.is_connected());
}
```

## 6. Best Practices

1. **Spec is truth** - If code and spec disagree, spec wins
2. **Small missions** - One focused task > large batch
3. **`max_retries = 0`** - Fresh context beats stale retries
4. **Check lint first** - Always know your binding status before coding

## 7. Troubleshooting

| Issue | Solution |
|-------|----------|
| "Missing bindings" | Add step functions matching the Gherkin text |
| "Dirty workspace" | Use `tesaki` REPL (auto-allows dirty) |
| "Gate failed" | Check `namako gate --json` for details |
| "Copilot not found" | Install GitHub Copilot CLI |

---

*See also: `namako/_WORKSPACE/` for full tooling documentation.*
