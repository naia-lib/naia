# Why naia?

naia occupies a specific niche in the Rust multiplayer networking ecosystem:
**server-authoritative entity replication that targets both native and browser
from a single codebase**, with built-in primitives for prediction, lag
compensation, and bandwidth control.

This page helps you decide whether naia is the right fit for your project. For
a technical deep-dive on how naia differs from each library, see
[Comparing naia to Alternatives](../reference/comparison.md).

---

## The short answer

Choose naia when you need **any** of the following:

- **Browser clients** — naia is the only Rust game networking library with a
  production WebRTC transport. Your game runs in `wasm32-unknown-unknown` with
  the same code as the native client.
- **Built-in lag compensation** — naia's `Historian` snapshots the world each
  tick so you can rewind to the tick the client was seeing and run server-side
  hit detection. No other Rust library ships this.
- **Per-entity bandwidth control** — set gain per entity per user; the send loop
  allocates bandwidth proportionally. Invisible entities can be paused entirely.
- **smol / async-std runtime** — naia has zero tokio dependencies. If your stack
  uses smol or async-std, naia fits without a runtime conflict.
- **ECS-agnostic core** — naia's core works with any entity type that is
  `Copy + Eq + Hash`. You can use it with Bevy, macroquad, or a custom engine.

---

## Decision guide

### I need browser clients (WASM)

**Use naia.** No other Rust game networking library ships a browser-compatible
WebRTC transport. The client code is identical for native and WASM targets —
only the socket type changes.

### I want to build on Bevy and I don't need browser clients

**Consider lightyear first.** lightyear is Bevy-native and ships a
prediction/interpolation framework baked in. naia's Bevy adapter is solid, but
naia supplies prediction as primitives you assemble rather than a full framework.
If you want the interpolation path handled for you, lightyear is a better fit.

If you also need browser clients, lag compensation, or per-entity bandwidth
control, naia is still the right choice even on Bevy.

### I only need message passing, not entity replication

**Consider renet.** renet is a lean message-passing library with no replication
overhead. naia's replication machinery (diff tracking, scope management, priority
sorting) adds overhead you won't benefit from if your game serializes its own
state manually.

### I'm building a fighting game or any P2P deterministic rollback game

**Use GGRS + matchbox.** GGRS implements GGPO-style rollback for fixed-roster
deterministic simulations. naia is server-authoritative — it is not designed for
P2P netcode. That said, a game can use naia for the lobby/server layer and GGRS
for the fast-path P2P match.

### I want the simplest possible Bevy replication and don't need advanced features

**Consider bevy_replicon.** bevy_replicon is simpler to set up and has less
surface area. It lacks browser clients, per-entity bandwidth control, lag
compensation, and zstd compression — if you don't need those, it may be easier
to start with.

---

## What naia provides

- Entity replication with per-field delta compression (`Property<T>`)
- Static entities (write-once, zero per-tick cost after initial send)
- Replicated resources (server-wide singletons, no room/scope setup required)
- Two-level interest management: rooms (coarse) + `UserScope` (fine-grained)
- Authority delegation (server grants/revokes client write authority per entity)
- Tick synchronization with client tick leading by ~RTT/2
- Client-side prediction primitives: `TickBuffered` channels, `CommandHistory`,
  `local_duplicate()`
- Lag compensation via the `Historian` snapshot buffer
- Priority-weighted bandwidth allocation with token-bucket send loop
- Optional zstd packet compression with custom dictionary training
- Connection diagnostics: RTT (EWMA + P50/P99), jitter, packet loss, kbps

## What naia does not provide

- A built-in snapshot interpolation framework (the demos show the pattern; you
  write the `Interp` component logic)
- Spatial / automatic interest management (you write the scope predicate;
  naia calls it via `scope_checks_pending()`)
- P2P / NAT hole-punching (naia is server-authoritative by design)

---

## Relationship to Tribes 2

naia's internal networking model follows the
[Tribes 2 Networking Model](https://www.gamedevs.org/uploads/tribes-networking-model.pdf)
(GDC 2000). If you have read that paper, the concepts of ghosts, scoping, and
packet send queues map directly to naia's entities, rooms + UserScope, and
`send_all_packets`.
