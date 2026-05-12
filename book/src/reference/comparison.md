# Comparing naia to Alternatives

An honest comparison of naia against the other major Rust multiplayer networking
libraries. Updated for 2026; check each library's changelog for the latest.

---

## Feature matrix

| | **naia** | **lightyear** | **renet** | **bevy_replicon** | **GGRS** |
|-|----------|--------------|-----------|------------------|---------|
| **Entity replication** | ✅ delta-compressed | ✅ | ❌ messages only | ✅ | ❌ |
| **Browser / WASM client** | ✅ WebRTC | ❌ | ❌ | ❌ | ❌ |
| **ECS-agnostic** | ✅ | ❌ Bevy-only | ✅ | ❌ Bevy-only | ✅ |
| **Lag compensation (Historian)** | ✅ built-in | ❌ | ❌ | ❌ | ❌ |
| **Priority-weighted bandwidth** | ✅ built-in | ❌ | ❌ | ❌ | ❌ |
| **Client-side prediction** | primitives | built-in | ❌ | ❌ | ✅ GGPO-style |
| **Interest management** | rooms + UserScope | rooms | ❌ | visibility filter | ❌ |
| **Authority delegation** | ✅ | ✅ | ❌ | ❌ | ❌ |
| **P2P / NAT traversal** | ❌ | ❌ | ❌ | ❌ | ✅ (via matchbox) |
| **zstd compression** | ✅ + dict training | ❌ | ❌ | ❌ | ❌ |
| **smol / async-std** | ✅ | ❌ (tokio) | ❌ (tokio) | ❌ (tokio) | n/a |
| **BDD test harness** | ✅ 215 contracts | ❌ | ❌ | ❌ | ❌ |

---

## naia vs lightyear

**Choose naia if:**
- You need browser clients (WebRTC WASM).
- Your runtime is smol / async-std (naia has no tokio dependency).
- You want built-in lag compensation without rolling your own snapshot buffer.
- You want ECS-agnostic core (custom engine, macroquad, etc.).
- You want explicit bandwidth control per entity.

**Choose lightyear if:**
- You are all-in on Bevy and want prediction/interpolation built into the
  framework rather than as primitives you assemble.
- You prefer tokio.
- You don't need browser clients.

---

## naia vs renet

renet is a **message-passing** library, not an entity replication library. It
does not replicate ECS state — you manage serialization and deserialization of
game state yourself. naia automates the diff-and-send loop.

**Choose renet if:**
- You want full control over serialization and don't want the replication
  overhead.
- Your game state doesn't map cleanly to ECS components.

**Choose naia if:**
- You want automatic per-field delta replication with no serialization
  boilerplate.

---

## naia vs bevy_replicon

bevy_replicon is a simpler Bevy-only replication library. It lacks browser
clients, per-entity bandwidth control, lag compensation, and zstd compression.

**Choose bevy_replicon if:**
- Simplicity is the top priority and you don't need any of naia's advanced
  features.

---

## naia vs GGRS

GGRS is a rollback-netcode library, not an entity replication library. It is
designed for **peer-to-peer deterministic simulations** (fighting games,
turn-based games) where every peer runs the same simulation and rollback is
used to reconcile divergences.

naia and GGRS are **complementary**: use naia for server→client replication
(lobby, world state, matchmaking) and GGRS for the fast-path P2P match
simulation.

---

## Updating this comparison

This page reflects the state of these libraries as of 2026-05. If you notice
an inaccuracy, please [open a PR](https://github.com/naia-lib/naia/edit/main/book/src/reference/comparison.md).
