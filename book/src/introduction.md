# Introduction

naia is a server-authoritative entity replication and typed message-passing library
for multiplayer games in Rust. It runs on native platforms (UDP) and in the browser
(WebRTC / WASM) from a **single codebase**.

---

## What naia is

naia lets you define a shared `Protocol` — a compile-time list of replicated
component types, message types, and channel configurations — that both the server
and the client agree on. Given that protocol:

- The **server** spawns entities, attaches replicated components, assigns users
  to rooms, and calls `send_all_packets` every tick. naia diffs changed fields
  and delivers them to every in-scope client automatically.
- The **client** receives entity spawn/update/despawn events and the current
  server-side field values with no extra bookkeeping.
- Either side can send typed messages over ordered-reliable, unordered-reliable,
  or unreliable channels.
- The server can **delegate authority** over a specific entity to a client,
  allowing client mutations to flow back to the server while the server retains
  final ownership.

naia is ECS-agnostic. Bevy and macroquad adapters are included; the core crate
works with any entity type that is `Copy + Eq + Hash + Send + Sync`.

The internal networking model follows the
[Tribes 2 Networking Model](https://www.gamedevs.org/uploads/tribes-networking-model.pdf).

---

## Why naia stands out

Among Rust game networking libraries, naia is unique in:

- **Browser clients** — the only Rust library with a production WebRTC transport
  for `wasm32-unknown-unknown`, sharing all protocol and game logic with the
  native client.
- **Built-in lag compensation** — the `Historian` snapshots the world each tick
  so you can rewind to the tick the client was seeing for server-side hit detection.
- **Per-entity bandwidth control** — set priority gain per entity per user; the
  token-bucket send loop allocates bandwidth proportionally.
- **No tokio dependency** — naia uses smol / async-std internally, fitting cleanly
  into stacks that already use those runtimes.

---

## Crate map

| Crate | Role | Use when… |
|-------|------|-----------|
| `naia-shared` | Protocol definition, derives, channel types | Writing the shared protocol crate |
| `naia-server` | Core server | Writing a server without Bevy |
| `naia-client` | Core client | Writing a client without Bevy or macroquad |
| `naia-bevy-server` | Bevy server adapter | Using Bevy on the server |
| `naia-bevy-client` | Bevy client adapter | Using Bevy on the client |
| `naia-macroquad-client` | macroquad adapter | Using macroquad on the client |

---

## Quick concepts

- **Protocol** — the shared type registry. Both server and client build from the same
  `Protocol` value; a hash mismatch during the handshake causes rejection.
- **Entity** — any `Copy + Eq + Hash` value your world allocates. naia tracks which
  entities are replicated and to whom, but never allocates them itself.
- **Room** — a coarse membership group. A user and an entity must share a room before
  replication is possible. Think: match, zone, lobby.
- **Channel** — a named transport lane with configurable ordering and reliability.
  Messages and entity actions travel through channels.
- **Tick** — the server's heartbeat. `take_tick_events` advances the tick counter.
  `TickBuffered` channels deliver client input at the correct server tick for
  prediction and rollback.
- **Authority delegation** — a server entity can be marked `Delegated`, allowing a
  client to request write authority. The server grants or denies and can revoke at
  any time.

---

## How to read this book

- **New to naia?** Start with [Bevy Quick Start](getting-started/bevy-quickstart.md)
  — copy-paste a working server + client in under five minutes.
- **Want a step-by-step walkthrough?** Read
  [Your First Server](getting-started/first-server.md) then
  [Your First Client](getting-started/first-client.md).
- **Looking for a specific concept?** Jump to [Core Concepts](concepts/protocol.md).
- **Building a prediction loop?** Read [Client-Side Prediction & Rollback](advanced/prediction.md).
- **Optimising bandwidth?** Read [Priority-Weighted Bandwidth](advanced/bandwidth.md)
  and [Delta Compression](advanced/delta-compression.md).
- **Comparing naia to other libraries?** See [Why naia?](getting-started/why-naia.md)
  for the decision guide and [Comparing naia to Alternatives](reference/comparison.md)
  for the technical deep-dive.
- **Migrating from an older API?** See the [Migration Guide](reference/migration.md).

> **Not using Bevy?** See [Without Bevy](adapters/overview.md) for macroquad
> and custom engine integration.
