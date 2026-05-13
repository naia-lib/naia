# Why naia?

naia occupies a specific niche in the Rust multiplayer networking ecosystem:
**Bevy-friendly, ECS-agnostic entity replication for native and browser clients
from one shared protocol**, with serious primitives for authority, prediction,
lag compensation, and bandwidth control.

This page helps you decide whether naia is the right fit for your project. For
a technical deep-dive on how naia differs from each library, see
[Comparing naia to Alternatives](../reference/comparison.md).

---

## The short answer

Choose naia when you want:

- **A real browser story without a second protocol** — WebRTC supports native and
  Wasm clients from the same server, so your browser build is not a novelty
  build that lives in a side alley.
- **A full authority toolkit** — server-owned entities, opt-in client-owned
  entities, publication, authority delegation, and delegated resources are all
  modeled explicitly.
- **Replication that scales past the happy path** — rooms, per-user scope,
  per-field deltas, static entities, priority-weighted bandwidth, and
  per-connection budgets are built in.
- **Lag compensation as a first-class primitive** — the `Historian` gives the
  server a rewindable view for hit detection and other "what did that client
  see?" questions.
- **Bevy ergonomics with an escape hatch** — use the Bevy plugin when you are in
  Bevy; use the core API for macroquad or a custom world when you are not.

---

## Decision guide

### I need browser clients (WASM)

**Use naia if you want WebRTC and one shared protocol.** lightyear also supports
Wasm clients through WebTransport, and transport-agnostic libraries can be paired
with Web transports. naia's pitch is more specific: its WebRTC transport is part
of the naia stack and can serve native and Wasm clients simultaneously from the
same server.

### I want to build on Bevy and I don't need browser clients

**Consider lightyear first.** lightyear is Bevy-native and ships a
prediction/interpolation framework baked in. naia's Bevy adapter is solid, but
naia supplies prediction as primitives you assemble rather than a full framework.
If you want the interpolation path handled for you, lightyear is a better fit.

If you also need WebRTC, lag compensation, client-authoritative entities,
delegated resources, or per-entity bandwidth control, naia is still the stronger
choice even on Bevy.

### I only need message passing, not entity replication

**Consider a lower-level transport/message library.** naia's replication
machinery (diff tracking, scope management, priority sorting) is useful when
your game state maps to replicated entities/resources. If you already serialize
all state manually, a smaller layer may fit better.

### I'm building a fighting game or any P2P deterministic rollback game

**Use GGRS + matchbox.** GGRS implements GGPO-style rollback for fixed-roster
deterministic simulations. naia is server-authoritative — it is not designed for
P2P netcode. That said, a game can use naia for the lobby/server layer and GGRS
for the fast-path P2P match.

### I want the simplest possible Bevy replication and don't need advanced features

**Consider bevy_replicon.** bevy_replicon is simpler to set up and has less
surface area. It is also transport-agnostic, so browser support depends on the
transport you pair it with. If you do not need naia's authority model,
Historian, WebRTC transport, priority bandwidth, or compression features, it may
be easier to start with.

---

## What naia provides

- Entity replication with per-field delta compression (`Property<T>`)
- Static entities (write-once, zero per-tick cost after initial send)
- Replicated resources (singletons that can be server-owned or delegated)
- Two-level interest management: rooms (coarse) + `UserScope` (fine-grained)
- Opt-in client-authoritative entities
- Authority delegation (server grants/revokes client write authority per entity
  or resource)
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
- P2P / NAT hole-punching as a primary architecture
