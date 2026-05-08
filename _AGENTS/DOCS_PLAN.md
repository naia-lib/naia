# Naia Documentation Overhaul Plan

**Created:** 2026-05-08  
**Status:** IN PROGRESS

This plan brings naia's documentation to production-grade quality. The goal is a
cohesive documentation suite that is accurate to the current codebase, idiomatic
Rust, and useful at every experience level — from a game developer evaluating
networking libraries to a contributor reading the codebase for the first time.

---

## Guiding principles

- **Accurate before comprehensive.** Every claim must match the current code.
  No aspirational feature lists, no stale demo instructions.
- **Succinct.** One sentence beats three. No padding.
- **Layered.** README → module docs → item docs → concepts guide forms a
  gradient from "what is this?" to "how does this internal invariant work?"
- **Cohesive.** Types cross-reference each other. The README links to the
  concepts guide. The concepts guide links back to API docs. Nothing is an island.

---

## Deliverables

### D-1 — README.md (complete rewrite)

The current README is stale (references `cargo-web`, duplicate headers, aspirational
"tutorials are on the way"). Replace it entirely.

**Structure:**
1. **Tagline** — one sentence, no buzzwords
2. **What naia is** — server-authoritative ECS replication + message passing; runs native
   and Wasm; Bevy and macroquad adapters; built on the Tribes 2 networking model
3. **Crate map** — table: crate name → role → when to add it as a dependency
4. **Quick concepts** — 6-bullet conceptual sketch (Protocol, Entity, Room, Channel,
   Tick, Authority delegation) — enough to orient, not enough to teach
5. **Getting started** — three paths: core (no ECS), Bevy adapter, macroquad adapter;
   each path: Cargo.toml snippet + link to the relevant demo
6. **Channel reference table** — the 6 built-in channels, their guarantees, and
   canonical use cases
7. **Platform support** — native (UDP), browser (WebRTC); wasm32-unknown-unknown targets
8. **Links** — docs.rs, Discord, CONCEPTS.md, CHANGELOG.md, demos/

### D-2 — Module-level `//!` docs (4 crate lib.rs files)

Each `//!` block is the crate's front page on docs.rs. Currently server and client have
2–3 line stubs; Bevy adapters have none.

**Each block must contain:**
- What the crate does in one sentence
- The canonical main loop (for server/client) or setup flow (for adapters)
- A minimal, compilable code example
- Links to the key types (`Server`, `Client`, `EntityMut`, etc.)

**Files:**
- `server/src/lib.rs`
- `client/src/lib.rs`
- `adapters/bevy/server/src/lib.rs`
- `adapters/bevy/client/src/lib.rs`

### D-3 — Item-level `///` rustdoc on all public API

The priority tiers below drive the implementation sequence.

**Tier A — Core types (must document first; everything else references these)**

| Item | File |
|------|------|
| `Publicity` enum | `shared/src/world/publicity.rs` |
| `ReplicationConfig` struct + `ScopeExit` | `server/src/world/replication_config.rs` |
| `NaiaServerError` enum | `server/src/error.rs` |
| `NaiaClientError` enum | `client/src/error.rs` |
| `EntityOwner` | `server/src/world/entity_owner.rs` |

**Tier B — The server API surface**

Document every `pub fn` on `Server<E>` in `server/src/server/server.rs`.
Group the docs by logical area with a leading `//` section comment:

- Connection lifecycle (`listen`, `accept_connection`, `reject_connection`)
- Event loop (`receive_all_packets`, `process_all_packets`, `take_world_events`,
  `take_tick_events`, `send_all_packets`) — with a note on the mandatory call order
- Messaging (`send_message`, `broadcast_message`, `send_request`, `send_response`)
- Entities (`spawn_entity`, `entity`, `entity_mut`, `entities`, `entity_owner`)
- Entity replication config (`configure_entity_replication`, `entity_replication_config`,
  `entity_is_static`, `entity_is_delegated`)
- Authority delegation (`entity_give_authority`, `entity_take_authority`,
  `entity_release_authority`, `enable_delegation`)
- Resources (`insert_resource`, `remove_resource`, `has_resource`, `configure_resource`,
  `resource_take_authority`, `resource_release_authority`)
- Rooms (`create_room`, `room_mut`, `rooms_count`)
- Users (`user`, `user_mut`, `user_keys`, `users_count`, `user_scope`, `user_scope_mut`)
- Scope management (`scope_checks_all`, `scope_checks_pending`)
- Ticks (`receive_tick_buffer_messages`)
- Adapter-only methods — mark clearly with `# Adapter use only` warning

**Tier B — The client API surface**

Same approach for `Client<E>` in `client/src/client.rs`:

- Connection (`auth`, `connect`, `connection_status`, `disconnect`, `server_address`)
- Event loop (`receive_all_packets`, `process_all_packets`, `take_world_events`,
  `take_tick_events`, `send_all_packets`)
- Messaging (`send_message`, `send_request`, `send_response`)
- Entities (`spawn_entity`, `entity`, `entity_mut`, `entities`)
- Entity replication config (`configure_entity_replication`, `entity_replication_config`)
- Authority (`entity_request_authority`, `entity_release_authority`)
- Resources (`has_resource`, `resource_entity`, `resource_entities`)
- Tick info (`client_tick`, `server_tick`, `tick_duration`, `client_interpolation`)
- Diagnostics (`rtt`, `jitter`, `incoming_bandwidth`, `outgoing_bandwidth`)

**Tier C — Builder types**

- `server::EntityMut` — all methods; note `as_static()` must be called before
  `insert_component`; explain the static-entity contract
- `client::EntityMut` — all methods
- `EntityRef` (both sides) — read-only accessors
- `RoomMut` / `RoomRef`
- `UserMut` / `UserRef`
- `UserScopeMut` / `UserScopeRef`

**Tier D — Bevy adapter public API**

- `CommandsExt` trait — `enable_replication`, `enable_static_replication`,
  `disable_replication`, `configure_replication`, `replicate_resource`
- `ServerCommandsExt` / `ClientCommandsExt`
- `Plugin` (both adapters) — what systems it schedules and in what order
- `Server` / `Client` Bevy resources — how they differ from the core structs

### D-4 — `docs/CONCEPTS.md`

The mental-model guide. Audience: a Rust game developer who has read the README and
is about to write their first multiplayer game with naia. Sections:

1. **The shared Protocol**  
   What `Protocol` is, why both server and client build from the same definition,
   how to structure the shared crate. Include the `Protocol::builder()` idiom.

2. **Entities and Components**  
   naia tracks entities as generic `E: Copy + Eq + Hash` values — it is ECS-agnostic.
   The server owns a subset of the world; components must `#[derive(Replicate)]`.
   Explain `Property<T>` as the change-detection wrapper.

3. **The Replication Loop**  
   Walk through a single server tick: `receive_all_packets` → `process_all_packets`
   → event dispatch → mutation → `send_all_packets`. Explain why the order is
   mandatory and what breaks if you skip steps.

4. **Rooms and Scope**  
   Two-level scoping model:
   - Room membership (coarse): user + entity must share a room for replication
     to be possible
   - `UserScope` (fine): explicit include/exclude within a room; `scope_checks_all`
     and `scope_checks_pending` for the custom scope callback pattern
   Include the `x ∈ [5, 15]` scope example from the basic demo, with explanation.

5. **Channels**  
   The 6 built-in `ChannelMode`s and their guarantees. When to use each.
   Custom channels: `#[derive(Channel)]`, `ChannelDirection`, add via `Protocol`.
   `TickBuffered`: why it exists (client→server input with tick-accurate delivery).

6. **Static vs Dynamic Entities**  
   Dynamic (default): delta-tracked, per-field diffs sent on change.  
   Static: no diff-tracking; full component snapshot sent once on scope entry;
   created via `.as_static()` builder call. Use cases: map tiles, level geometry,
   any entity that never mutates after spawn.

7. **Replicated Resources**  
   Server-side singletons automatically in scope for all connected users.
   `insert_resource(world, value, is_static)`. Compare with entities: no room
   membership needed; no scope management; one per type.

8. **Authority Delegation**  
   Full state-machine walkthrough:
   - Server marks entity `ReplicationConfig::delegated()`
   - Client calls `entity_request_authority` → `EntityAuthDeniedEvent` or grant
   - While client holds authority: client mutations replicate to server
   - Client calls `entity_release_authority` → server resumes ownership
   Explain the trust model: the server can revoke at any time; the client never
   holds unrevocable ownership.

9. **Tick Synchronisation**  
   `tick_interval` in Protocol. Server ticks drive `send_all_packets`.
   `client_tick` vs `server_tick`. `TickBuffered` channel and prediction.
   `CommandHistory` for rollback.

10. **Transport and Wasm**  
    Native: UDP socket via naia-socket-native.  
    Browser: WebRTC data channel via naia-socket-webrtc.  
    Same API; different `Socket` type passed to `listen`/`connect`.
    `wasm32-unknown-unknown` with `wbindgen` feature.

### D-5 — `CHANGELOG.md`

Standard [Keep a Changelog](https://keepachangelog.com/) format.
Create `[Unreleased]` section documenting all breaking changes from the API
cleanup session (2026-05-08):

- `spawn_static_entity` removed → `spawn_entity(world).as_static()`
- `insert_static_resource` removed → `insert_resource(world, value, true)`
- `insert_resource` signature changed → `insert_resource(world, value, is_static: bool)`
- `WorldEvents<E>` (client) renamed to `Events<E>`
- `make_room` renamed to `create_room`
- `resource_count` / `room_count` renamed to `resources_count` / `rooms_count`
- Client `ReplicationConfig` enum removed → `Publicity` (from `naia_shared`)
- `server.send_message` now returns `Result<(), NaiaServerError>`
- `EntityMut::insert_components` (batch) removed → use `insert_component` per field
- `entity_is_delegated` predicate added to server public API

### D-6 — `docs/MIGRATION.md`

Before/after code snippets for every breaking change in D-5.
Concise: one code block per change, no prose padding. Audience is someone with
an existing naia project upgrading to the post-cleanup API.

### D-7 — `SECURITY.md`

**Trust model:**
- The server is authoritative. All entity state originates on the server.
- Authority delegation grants a client the right to mutate *specific* entities.
  The server still receives those mutations and can reject or clamp them.
- naia does NOT provide: packet authentication (DTLS/TLS), anti-cheat,
  rate-limiting at the application layer, or input validation. These are the
  application's responsibility.
- `AuthEvent` credentials are transmitted in plaintext by default.
  Applications that need secrecy MUST wrap the transport in TLS/DTLS.
- Client-authoritative mutations should be validated server-side before
  applying to authoritative game state. naia replicates what the client
  sends; it does not validate it.

---

## Implementation sequence

The sequence follows dependency order: later items use vocabulary and cross-links
established by earlier items.

```
Phase 1 — Foundations (item docs on core types)
  D-3/A  Publicity, ReplicationConfig, ScopeExit, error types, EntityOwner

Phase 2 — API surface (item docs on main types)
  D-3/B  Server<E> all pub fn
  D-3/B  Client<E> all pub fn
  D-3/C  EntityMut (both sides), EntityRef, RoomMut/Ref, UserMut/Ref, UserScopeMut/Ref

Phase 3 — Module entry points
  D-2    //! blocks for all four lib.rs files

Phase 4 — Architecture narrative
  D-4    docs/CONCEPTS.md

Phase 5 — History and migration
  D-5    CHANGELOG.md
  D-6    docs/MIGRATION.md

Phase 6 — Bevy adapter surface
  D-3/D  CommandsExt, Plugin, Server/Client Bevy resources

Phase 7 — README and security
  D-1    README.md complete rewrite
  D-7    SECURITY.md
```

---

## Style conventions

- Item docs: one-sentence summary, then detail paragraphs. Summary line must
  be a complete sentence ending with a period. No "This method…" opening.
- Panics: document with `# Panics` section whenever a method can panic.
- Errors: document with `# Errors` section whenever a method returns `Result`.
- Cross-links: use `` [`TypeName`] `` and `` [`method_name`] `` everywhere.
- No aspirational text: do not document planned but unimplemented features.
- Code examples in `///` docs: use `# use naia_server::*;` preamble to keep
  examples compilable without requiring full demo setup.
- Bevy adapter docs: note which items require the `bevy_support` feature.
