# Comparing naia to Alternatives

An in-depth technical comparison of naia against the major Rust multiplayer
networking libraries. For a high-level decision guide, see
[Why naia?](../getting-started/why-naia.md).

Updated 2026-05. Check each library's changelog for the latest.

---

## Feature matrix

| | **naia** | **lightyear** | **renet** | **bevy_replicon** | **GGRS** |
|-|----------|--------------|-----------|------------------|---------|
| **Entity replication** | ✅ delta-compressed | ✅ | ❌ messages only | ✅ | ❌ |
| **Browser / WASM client** | ✅ WebRTC | ❌ | ❌ | ❌ | ❌ |
| **ECS-agnostic** | ✅ | ❌ Bevy-only | ✅ | ❌ Bevy-only | ✅ |
| **Lag compensation (Historian)** | ✅ built-in | ❌ | ❌ | ❌ | ❌ |
| **Priority-weighted bandwidth** | ✅ per-entity + per-user | ❌ | ❌ | ❌ | ❌ |
| **Client-side prediction** | primitives | built-in framework | ❌ | ❌ | ✅ GGPO-style |
| **Interest management** | rooms + UserScope | rooms | ❌ | visibility filter | ❌ |
| **Authority delegation** | ✅ | ✅ | ❌ | ❌ | ❌ |
| **P2P / NAT traversal** | ❌ | ❌ | ❌ | ❌ | ✅ (via matchbox) |
| **zstd compression** | ✅ + dict training | ❌ | ❌ | ❌ | ❌ |
| **smol / async-std** | ✅ | ❌ (tokio) | ❌ (tokio) | ❌ (tokio) | n/a |
| **BDD test harness** | ✅ 215 contracts | ❌ | ❌ | ❌ | ❌ |

---

## naia vs lightyear — in depth

### Similarities

Both naia and lightyear provide entity replication with delta compression,
authority delegation, interest management, and client-side prediction primitives
built on a tick-synchronized model.

### Key differences

**Browser support.** naia ships `transport_webrtc` — a production WebRTC transport
that runs in `wasm32-unknown-unknown`. lightyear has no browser transport.

**Prediction model.** lightyear ships a complete prediction/interpolation framework:
`Predicted` and `Interpolated` entity markers, automatic rollback, and a hook
for registering custom rollback systems. naia supplies the building blocks
(`TickBuffered` channels, `CommandHistory`, `local_duplicate()`) and you write
the prediction loop. naia's approach gives you more control; lightyear's gives
you a faster start.

**ECS coupling.** naia's core crates are ECS-agnostic — the same `Server<E>` and
`Client<E>` types work with Bevy, macroquad, or a custom engine. lightyear is
tightly coupled to Bevy; its API is a set of Bevy plugins and uses Bevy resources
throughout.

**Async runtime.** naia uses smol / async-std internally. lightyear uses tokio.
This matters if your project already has a runtime — mixing tokio and async-std
requires a compatibility shim.

**Lag compensation.** naia ships the `Historian` — a rolling per-tick world
snapshot buffer you use to rewind the server to the client's perceived tick for
hit detection. lightyear has no equivalent built-in primitive.

**Bandwidth control.** naia's priority accumulator lets you set gain per entity
per user; entities with gain `0.0` are never sent. lightyear does not ship a
per-entity priority system.

**Compression.** naia supports optional zstd packet compression with default,
custom-dictionary, and dictionary-training modes. A game-specific dictionary
typically achieves 40–60% better compression than the default on real packet data.
lightyear does not ship zstd support.

---

## naia vs renet — in depth

renet is a **message-passing library**, not an entity replication library. It
provides reliable and unreliable channels over UDP, connection management, and
typed message serialization. It does not replicate ECS state — you must serialize
and deserialize your entire game state manually.

**naia automates the diff-and-send loop.** With naia, mark a component field as
`Property<T>` and naia automatically:
- Detects which fields changed each tick.
- Sends only changed fields to each in-scope user.
- Handles spawn, despawn, and scope entry/exit events.
- Manages per-user interest (rooms, UserScope).

With renet you write all of that manually. This gives you full control over
the wire format, but it is significantly more code.

**When renet is the right choice:**
- Your game state does not map cleanly to ECS components.
- You want the smallest possible dependency footprint.
- You need full control over serialization for an unusual wire format.

---

## naia vs bevy_replicon — in depth

bevy_replicon is a simpler Bevy-only replication library. Its API surface is
smaller and it has less to configure.

**Gaps vs naia:**
- No browser / WASM transport.
- No per-entity bandwidth control or priority accumulator.
- No lag compensation (no Historian).
- No zstd compression.
- Interest management via a single `VisibilityFilter` — less granular than
  naia's two-level rooms + UserScope model.

**When bevy_replicon is the right choice:**
- You are building a simple Bevy game and none of the above features are required.
- You want to minimize the amount of configuration to get replication working.

---

## naia vs GGRS — in depth

GGRS is a rollback-netcode library for **peer-to-peer deterministic simulations**.
It is not an entity replication library. GGRS assumes a fixed roster of peers
all running the exact same deterministic simulation, and uses rollback to reconcile
divergences when inputs arrive late.

naia is server-authoritative. A server holds all canonical state; clients receive
a replicated view. These are fundamentally different architectures for different
genres:

| Use case | Architecture | Library |
|----------|-------------|---------|
| Fighting game (2 players, P2P) | Deterministic rollback | GGRS + matchbox |
| MMO / open world (N players, server-auth) | Server replication | naia |
| MOBA / shooter with server-side hit detection | Server-auth + lag comp | naia + Historian |

**GGRS and naia are complementary.** A game can use naia for the lobby, world
state, and matchmaking layer, and GGRS for the fast-path P2P match simulation —
the two libraries operate on independent connections.

---

## Updating this page

This page reflects the state of these libraries as of 2026-05. If you notice
an inaccuracy, please
[open a PR](https://github.com/naia-lib/naia/edit/main/book/src/reference/comparison.md).
