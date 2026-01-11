# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Naia is a cross-platform (including Wasm) networking engine for multiplayer game development in Rust. It provides:
- **ECS replication**: Entities/components on the server automatically sync to clients when "in scope"
- **Message passing**: Reliable/unreliable, ordered/unordered channels
- **Efficient serialization**: Bit-packing and delta-compression

The architecture follows the [Tribes 2 Networking model](https://www.gamedevs.org/uploads/tribes-networking-model.pdf).

## Build and Test Commands

```bash
# Run all workspace tests
cargo test --workspace

# Run the main test package (integration/E2E tests)
cargo test --package naia-test

# Run tests with output visible
cargo test --package naia-test -- --nocapture

# Run a specific test
cargo test --package naia-test test_name

# Check formatting
cargo fmt -- --check

# Run clippy (note: demos excluded from default-members due to feature conflicts)
cargo clippy --no-deps

# Generate coverage report
./scripts/test_coverage.sh
```

## Architecture

### Crate Hierarchy

```
socket/           # Transport layer (WebRTC protocol implementation)
  ├── shared/     # Common socket types
  ├── client/     # Client-side socket
  └── server/     # Server-side socket

shared/           # Core networking abstractions
  ├── serde/      # Bit-packing serialization (naia-serde)
  └── derive/     # Proc macros for Message, Replicate, Channel

client/           # Game client networking
server/           # Game server networking

adapters/         # ECS framework integrations
  ├── bevy/       # Bevy ECS adapter (client, server, shared)
  └── hecs/       # Hecs ECS adapter (client, server, shared)

test/             # E2E testing harness
```

### Key Abstractions

**Protocol**: Defines the shared contract between client and server. Built using a builder pattern that registers:
- Components (via `#[derive(Replicate)]`)
- Messages (via `#[derive(Message)]`)
- Channels (via `#[derive(Channel)]`)

Example:
```rust
Protocol::builder()
    .add_component::<Position>()
    .add_message::<Auth>()
    .add_channel::<ReliableChannel>(
        ChannelDirection::Bidirectional,
        ChannelMode::OrderedReliable(ReliableSettings::default()),
    )
    .build()
```

**Property<T>**: Wrapper enabling change-detection for delta-compression. Required for component fields.

**Rooms**: Entities are "scoped" to clients via rooms. Entities in the same room as a user are synced to that user.

### Testing Infrastructure

The `test/` crate contains a deterministic testing harness using local transport:

- **Scenario**: Test fixture that simulates server + multiple clients
- **mutate/expect pattern**: Tests alternate between mutation phases and expectation phases
- **TestClock**: Deterministic time control via `test_time` feature

```rust
let mut scenario = Scenario::new();
scenario.server_start(ServerConfig::default(), protocol());
let client = scenario.client_start("player", Auth::new("user", "pass"), ...);

scenario.mutate(|ctx| {
    // Spawn entities, send messages
});

scenario.expect(|ctx| {
    // Assert on events, entity state
    Some(()) // Return Some when expectations met
});
```

### Feature Flags

Important features that affect compilation:
- `wbindgen` / `mquad`: Required for Wasm targets (mutually exclusive)
- `transport_udp` / `transport_local`: Transport implementations
- `bevy_support`: Enable Bevy ECS integration
- `test_time`: Enable deterministic time for testing
- `interior_visibility`: Expose `LocalEntity` type
- `e2e_debug`: Enable debug counters for E2E tests

## Specifications

The `specs/` directory contains normative specifications using RFC 2119 keywords (MUST, SHOULD, MAY). Specs are authoritative - if implementation differs from spec, the implementation is wrong.

## Development Notes

- Demos are excluded from default workspace members due to conflicting feature flags
- Miniquad clients require: `sudo apt-get install libxi-dev libgl1-mesa-dev`
- The codebase uses WebRTC for cross-platform networking (works on Web + Native)
- Naia does NOT provide client-prediction, lag compensation, or snapshot interpolation - these are game-specific
