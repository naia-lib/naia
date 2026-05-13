# Introduction

naia is an entity replication and typed message-passing library for multiplayer
games in Rust. Its default architecture is server-authoritative, but it also
supports opt-in client-authoritative entities and delegated authority when your
game needs a more flexible ownership model.

The same naia server can accept native and browser clients over WebRTC from a
single shared protocol and game-code path. UDP is still available for native
development and trusted networks, but WebRTC is the transport most users should
reach for first.

---

## What naia is

naia lets you define a shared `Protocol` — a compile-time list of replicated
component types, message types, and channel configurations — that both the server
and the client agree on. Given that protocol:

- The **server** usually spawns entities, attaches replicated components, assigns
  users to rooms, and lets the adapter flush packets every tick. naia diffs
  changed fields and delivers them to every in-scope client automatically.
- The **client** receives entity spawn/update/despawn events and the current
  server-side field values with no extra bookkeeping.
- Either side can send typed messages over ordered-reliable, unordered-reliable,
  or unreliable channels.
- Clients can create their own replicated entities when the protocol explicitly
  enables client-authoritative entities.
- The server can **delegate authority** over a specific entity to a client,
  allowing client mutations to flow back to the server while the server retains
  final ownership.

naia is ECS-agnostic at its core. The Bevy adapter is the most polished path,
macroquad works through the core client, and custom engines can integrate by
implementing naia's world access traits.

The internal networking model follows the
[Tribes 2 Networking Model](https://www.gamedevs.org/uploads/tribes-networking-model.pdf).

---

## Why naia stands out

Among Rust game networking libraries, naia stands out for:

- **One server, native and Wasm clients** — `transport_webrtc` works for native
  and `wasm32-unknown-unknown` clients at the same time, using the same protocol
  and gameplay code. The browser is not a second-class citizen; it gets a chair
  at the adult table.
- **A complete replication model** — server-owned, client-owned, delegated,
  public/private publication, static entities, replicated resources, rooms, and
  per-user scope are all part of the same system.
- **Built-in lag compensation** — the `Historian` snapshots the world each tick
  so you can rewind to the tick the client was seeing for server-side hit detection.
- **Per-entity bandwidth control** — set priority gain per entity per user; the
  token-bucket send loop allocates bandwidth proportionally.
- **Bevy-first ergonomics without Bevy lock-in** — Bevy users get plugins,
  commands, and replicated resources; non-Bevy users still get the same protocol,
  transport, replication, and message machinery.
- **Typed everything** — messages, requests/responses, channels, replicated
  components, resources, and auth payloads all go through the shared protocol.

---

## Crate map

| Crate | Role | Use when… |
|-------|------|-----------|
| `naia-shared` | Protocol definition, derives, channel types | Writing the shared protocol crate |
| `naia-server` | Core server | Writing a server without Bevy |
| `naia-client` | Core client | Writing a client without Bevy or macroquad |
| `naia-bevy-shared` | Bevy protocol/resource/component helpers | Writing a Bevy shared crate |
| `naia-bevy-server` | Bevy server adapter | Using Bevy on the server |
| `naia-bevy-client` | Bevy client adapter | Using Bevy on the client |
| `naia-metrics` / `naia-bevy-metrics` | Optional diagnostics integration | Exporting runtime metrics |

---

## Quick concepts

- **Protocol** — the shared type registry. Both server and client build from the same
  `Protocol` value; a hash mismatch during the handshake causes rejection.
- **Entity** — a world object that naia can replicate after it has been
  registered with the replication layer. In Bevy, that means calling
  `enable_replication()`.
- **Component** — replicated state attached to an entity. Fields wrapped in
  `Property<T>` are delta-tracked.
- **Resource** — a replicated singleton value, represented internally as a
  hidden one-component entity.
- **Message** — a typed payload sent over a channel. Messages are not
  delta-tracked; they are serialized each time they are sent.
- **Room** — a coarse membership group. A user and an entity must share a room before
  replication is possible. Think: match, zone, lobby.
- **Scope** — a per-user fine-grained visibility decision applied after rooms.
- **Channel** — a named transport lane with configurable ordering and reliability.
  Messages and entity actions travel through channels.
- **Tick** — the server's heartbeat. `take_tick_events` advances the tick counter.
- **Client-authoritative entity** — an opt-in entity created and owned by a
  client, replicated to the server, and optionally published to other clients.
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

> **Not using Bevy?** See [Without Bevy](adapters/overview.md) for macroquad
> and custom engine integration.
