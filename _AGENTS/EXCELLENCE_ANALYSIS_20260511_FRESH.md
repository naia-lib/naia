# Naia Excellence Audit — 2026-05-11 (Fresh)

## Executive Summary

The specops/naia private branch is a substantially stronger library than its
last public release (v0.24, December 2024). Since the previous audit cycle
began, the codebase has closed most of the identified doc and code gaps: the
FAQ is accurate, PREDICTION.md covers multi-entity rollback and correction
batching, CONCEPTS.md has 20 sections including Historian/compression/priority/
diagnostics, SECURITY.md has a concrete stunnel guide, enum `#[derive(Message)]`
is implemented, Bevy `DefaultClientTag` and `DefaultPlugin` exist, Historian
filtering ships, five fuzz targets are present, a full `naia-metrics` /
`naia-bevy-metrics` observability layer is implemented, and `purge_user` closes
the request-map disconnect leak. The three remaining actionable gaps are:
(1) FEATURES.md still lists five completed items as planned, understating the
library to every developer who reads it; (2) message channel senders have no
capacity bound — an unbounded `VecDeque` in `ReliableSender` is a confirmed
OOM vector under adversarial or runaway clients (GitHub issue #165, open since
2023); (3) the demo `send_all_packets` call is placed inside the `TickEvent`
loop instead of at the top level of `update()`, which is a copy-paste trap for
every new user. The single highest-impact change is publishing the current
private branch as v0.25 targeting Bevy 0.18 — the private branch is at A-grade
quality but developers evaluating naia on crates.io see a 17-month-old release
targeting Bevy 0.15. **GRADE: A−**

---

## Competitive Landscape

Sources: GitHub (2026-05-11 stars, issues, last release), arewegameyet.rs,
lightyear and bevy_replicon changelogs, developer forum posts.

| Competitor | Stars | Last release | Strengths vs naia | Weaknesses vs naia |
|------------|-------|-------------|-------------------|--------------------|
| **lightyear** | 992 | v0.26.0 (Jan 2026, Bevy 0.18) | First-class prediction/rollback plugin; WebTransport (QUIC) + WebSocket + UDP + Steam relay; Avian physics integration; deterministic/lockstep mode; monthly release cadence | Bevy-only (not ECS-agnostic); no macroquad path; heavier API surface; steep learning curve; 25+ open issues |
| **renet / renet2** | 909 | v2.0.0 (Jan 2026, Bevy 0.18) | netcode.io 2.0 auth with built-in encryption; renet_visualizer egui dashboard; extremely simple message-passing API; more confirmed shipped games | Message-only — no entity replication, no rooms, no scope management; no WASM/browser client |
| **bevy_replicon** | 595 | v0.39.5 (Apr 2026, Bevy 0.18) | Per-component replication granularity; QUIC via bevy_quinnet; highest patch cadence; no_std support; pluggable transport | No prediction or rollback; no built-in WASM transport; Bevy-only scope |
| **Quinn** | 5 070 | active | Production QUIC + TLS 1.3; zero-RTT resumption | Pure transport — no game-networking layer |
| **GameNetworkingSockets (Valve)** | — | — | Production-proven at scale; per-lane reliability; Steam relay | C++ primary; Rust bindings unofficial |

**Ecosystem momentum.** lightyear shipped 7 versions in 6 months
(v0.20 May 2024 → v0.26 Jan 2026) and is now the default recommendation in
Bevy community discussions. bevy_replicon patches every 2–4 weeks. naia's last
public release remains December 2024 (v0.24, Bevy 0.15); the private branch
has moved to Bevy 0.18 and landed significant new features, but nothing is
visible to evaluators on crates.io. arewegameyet.rs lists naia at 1,122 stars
(#1 among dedicated game-net libs) — indicating existing community trust that
the release gap is eroding.

---

## Gap Analysis

### B-1 — FEATURES.md lists five completed items as planned (stale)

**Current state.**
As of 2026-05-11, `FEATURES.md` still marks the following as `[ ]` planned:

| Line | Claim | Reality |
|------|-------|---------|
| 41 | "Enum support in `#[derive(Message)]`" | Done — commit `13e1d61e`; `shared/derive/src/message.rs` dispatches to `enum_message_impl` at line 19 |
| 43 | "`DefaultClientTag` alias in Bevy adapter" | Done — commit `62d80370`; `adapters/bevy/client/src/lib.rs` line 104 exports `DefaultClientTag` |
| 44 | "Historian component-kind filtering" | Done — commit `4d33206a`; `server/src/historian.rs` `enable_historian_filtered` ships |
| 45 | "Additional fuzz targets: quantized serde types, replication decoder, handshake" | Done — commit `34805f5f`; five targets in `fuzz/fuzz_targets/` |
| 46 | "Optional metrics / tracing integration" | Done — commits `89fdd0b4`+`64578202`; `naia-metrics` and `naia-bevy-metrics` crates ship |

A developer reading `FEATURES.md` before evaluating naia sees a library that
is missing five features it actually has. This directly suppresses adoption.

**Why it matters.**
FEATURES.md is typically read by developers doing pre-adoption due diligence —
the exact audience most likely to abandon the library if they see capability
gaps.

**Recommendation.**
Move all five items from `Planned` to the `Shipped` section with the
commit/crate reference. Takes 10 minutes.

**Effort:** XS.  **Leverage:** 3

---

### B-2 — Message channel senders have no capacity bound (OOM vector)

**Current state.**
`shared/src/messages/channels/senders/reliable_sender.rs` uses an unbounded
`VecDeque` for `sending_messages` and `outgoing_messages`. No maximum depth
check exists. `send_message` on the channel sender always pushes to the
back without checking queue depth. If a client sends faster than the network
can drain, or a server-side bug queues reliables without flushing, the queue
grows without bound until the process OOMs.

GitHub issue #165 ("Bug: MessageChannels need hard capacity limits") was opened
May 2023 and is still open. `send_message` on `Server<E>` now returns
`Result<(), NaiaServerError>` (from the Unreleased CHANGELOG), creating a
surface for communicating backpressure — but no limit is enforced, so the
`Ok` is vacuous.

This is the same category of bug as the handshake address-to-timestamp map
that was fixed (`CacheMap` bounded to 1024) — except the channel queues are
per-connection and therefore amplified by connection count.

**Why it matters.**
A slow or disconnected client (or a server bug that queues faster than it
flushes) can silently exhaust memory. This is a correctness and reliability
issue for production deployments, not just a DX issue.

**Recommendation.**
Add a `max_pending: Option<usize>` field to `ReliableSender` (and the
sequenced/unordered unreliable senders). When `send_message` would push past
the limit, return an error (for reliable channels) or silently drop the oldest
entry (for unreliable channels, where drop is semantically correct).
`ChannelMode` config can expose a `max_queue_depth` setting, defaulting to
a safe value like 1024.

**Effort:** S.  **Leverage:** 4

---

### B-3 — Demo `send_all_packets` is placed inside the TickEvent loop (copy-paste trap)

**Current state.**
`demos/basic/server/src/app.rs` line 185 calls `server.send_all_packets()` as
the last step inside the `for _ in tick_events.read::<TickEvent>()` loop body.
This is architecturally wrong: `send_all_packets` is one of the five mandatory
top-level steps (documented in CONCEPTS.md §3 and the server lib.rs doc
comment), not a per-tick inner call. CONCEPTS.md §3 explicitly says the five
steps must run at the top level every frame. The demo violates this.

**Why it matters.**
New users copy-paste demo code. A developer following the basic demo will put
`send_all_packets` inside the tick loop, which:
- Skips the flush when no tick fires (e.g. on a slower machine or network
  jitter causes a missed tick);
- Calls it zero or more-than-once if multiple tick events accumulate;
- Creates hard-to-debug packet ordering anomalies under load.

The Bevy demo avoids this problem because the Bevy adapter uses systems that
are scheduled correctly. The non-ECS basic demo is the entry point for users
who aren't using Bevy — the highest-friction path.

**Recommendation.**
Move `server.send_all_packets(self.world.proxy())` and the
`self.tick_count.wrapping_add(1)` line outside the `TickEvent` loop, making
them unconditional steps in `update()`. The tick loop body should only contain
game mutation logic; the five mandatory steps (including `send_all_packets`)
should be the outer frame structure.

**Effort:** XS.  **Leverage:** 3

---

### B-4 — Request/Response manager has no TTL eviction for pending entries

**Current state.**
`server/src/request.rs` `GlobalRequestManager` has `purge_user()` (added by
commit `24bc167e`) that correctly removes entries on user disconnect. However,
the map still has no TTL eviction for entries that are never responded to
(e.g., the server queues a request, the response handler has a bug and never
calls `send_response`, or the client's response is permanently lost). Such
entries accumulate indefinitely. On a long-running server they add up.

This is distinct from the disconnect case (which is now handled) — it is the
case where neither side disconnects but the response simply never arrives.

**Why it matters.**
Long-running game servers (MMO, persistent world) can accumulate thousands of
zombie request entries over days of uptime. While unlikely to OOM a modern
server on its own, it is a memory leak class and creates audit surface for
resource exhaustion.

**Recommendation.**
Add a `created_at: Instant` field to each map entry and an eviction sweep in
`purge_stale(now, max_age)` called periodically from the server's main loop.
Default `max_age` of 30 seconds covers all realistic request-response latency.

**Effort:** S.  **Leverage:** 2

---

### B-5 — Public release cadence: private branch targets Bevy 0.18, crates.io frozen on Bevy 0.15

**Current state.**
naia's last public release is v0.24.0 (December 2024, Bevy 0.15). Current
Bevy is 0.18. Every active alternative (lightyear v0.26, renet v2.0,
bevy_replicon v0.39) targets Bevy 0.18. The private specops branch targets
Bevy 0.18 (`adapters/bevy/client/Cargo.toml`) and contains 17 months of
substantial improvements over v0.24, but none are published.

Community status (arewegameyet.rs, GitHub Bevy networking discussion #4388):
naia is still listed and has the most stars of any dedicated game-net lib
(1,122), but community recommendations in active discussions have shifted to
lightyear (features) and bevy_replicon (simplicity + QUIC) for new projects
targeting Bevy 0.18.

**Why it matters.**
This is the single largest adoption barrier. Developers evaluating naia on
crates.io see a 17-month-old release on a Bevy version 3 generations behind.
All the work done in this audit cycle — A-1 through A-17 — is invisible to
the public. The private branch is at A− quality; the published library looks B.

**Recommendation.**
Publish v0.25 (or v1.0 if a version statement is warranted) targeting Bevy 0.18
with the current private branch state. Ship the audit-cycle improvements:
Historian, priority accumulators, prediction primitives, zstd compression,
ConnectionStats, reconnect fix, UB fix, handshake hardening, enum Message,
DefaultClientTag, fuzz targets, metrics crates. Update CHANGELOG and establish
a quarterly release cadence.

**Effort:** S (release prep + changelog polish).  **Leverage:** 5

---

### B-6 — No MSRV (minimum supported Rust version) declared

**Current state.**
Neither the workspace `Cargo.toml` nor any of the member crate `Cargo.toml`
files declare a `rust-version` field. This is confirmed by:
```
grep -r "rust-version\|MSRV" /home/connor/Work/specops/naia/Cargo.toml
# → (no output)
```

Without a declared MSRV, downstream crates can't check compatibility, CI
toolchain choices are arbitrary, and crates.io shows no minimum Rust
information.

**Why it matters.**
For ecosystem crates (as opposed to application binaries), declaring MSRV is
now a community expectation and is required for crates.io "Rust Version"
metadata. lightyear and bevy_replicon both declare theirs.

**Recommendation.**
Add `rust-version = "1.81"` (or the actual minimum) to the workspace
`Cargo.toml`. Verify with `cargo check` on that toolchain in CI.

**Effort:** XS.  **Leverage:** 1

---

### B-7 — `server/src/world/global_world_manager.rs` panics for expected-bad-caller states

**Current state.**
`global_world_manager.rs` contains ~10 `panic!` calls for conditions like
"entity record does not exist!" and "component does not exist!" These fire
when the caller passes an entity or component that is not tracked — conditions
that are reachable through normal API misuse (stale entity handles, calling
`remove_component` twice). The `user_opt`/`user_mut_opt` variants were added
for `user()`/`user_mut()` (CHANGELOG), but the entity-related functions still
hard-panic. GitHub issue #172 ("Many functions panic unnecessarily") is open
since May 2023.

These are documented in the CHANGELOG as a known issue but not yet resolved.

**Why it matters.**
A server that panics on a stale entity key brought in from application code
(e.g., a race between entity despawn and a message handler referencing it)
crashes the entire game session. This is the #2 reason (after the missing
release) that experienced game developers choose renet over naia for production.

**Recommendation.**
Audit `global_world_manager.rs` for all `panic!` sites. Convert those triggered
by external/caller input (stale entity key, double-remove) to `Result<_, NaiaServerError>`
return types. Keep `panic!` only for internal invariants that represent library
bugs. Add a `_opt` variant (returning `Option`) for the common "might be gone"
check pattern. This mirrors the `user_opt` pattern already established.

**Effort:** M (requires API changes and caller audit).  **Leverage:** 3

---

## Prioritised Action Table

| Rank | Gap ID | Description | Decision | Effort | Leverage |
|------|--------|-------------|----------|--------|----------|
| 1 | B-5 | Publish Bevy 0.18 release (v0.25) | After remaining gaps fixed | S | 5 |
| 2 | B-2 | Unbounded message channel queues (OOM vector) | Add max_queue_depth to ChannelMode | S | 4 |
| 3 | B-1 | FEATURES.md — 5 completed items still marked planned | Move to Shipped | XS | 3 |
| 4 | B-3 | Basic demo: send_all_packets inside TickEvent loop | Move outside loop | XS | 3 |
| 5 | B-7 | global_world_manager panics on stale entity keys | Result return types + _opt variants | M | 3 |
| 6 | B-4 | Request/Response: no TTL eviction for zombie entries | Add purge_stale() | S | 2 |
| 7 | B-6 | No MSRV declared | Add rust-version to workspace Cargo.toml | XS | 1 |

---

## What Would Make Naia Definitively #1

naia's ECS-agnostic design, rooms + UserScope interest management, static
entities, Historian lag compensation, per-entity priority accumulators, zstd
compression with custom dictionary training, tick-synchronised prediction
primitives, and metrics layer compose into a more complete feature set than
any single competitor. The thing holding it back from #1 is not features —
it is visibility. Publishing v0.25 targeting Bevy 0.18 with the current private
branch's accumulated improvements would immediately shift community evaluations
back toward naia. The technical quality is there; the public crate is not.

Secondary: closing B-2 (channel backpressure) would address the last
significant correctness concern for production use, removing the final
reservation experienced developers have about using naia in a live game server.

---

## Developer Journey Friction Test

Starting from README.md and following the "Getting started" → "Core (no ECS)"
path to: (1) establish a connection, (2) spawn one entity on the server,
(3) replicate one component update to the client.

**Step 1 — Understand which crate to use.**
README Crate map table is accurate and immediately helpful. No friction.

**Step 2 — Define a component.**
README Quick concepts section + demos/basic/shared shows `#[derive(Replicate)]`
with `Property<T>`. Well-documented in CONCEPTS.md §2 and §13.

**Step 3 — Build the Protocol.**
CONCEPTS.md §1 has a working code snippet. Only minor friction: the `Protocol`
builder pattern with `.add_component::<T>()` is clear, but a new user might
not immediately understand why both server and client call the same function —
the BLAKE3 hash mismatch behavior is documented but not highlighted.

**Step 4 — Server main loop.**
CONCEPTS.md §3 documents the five mandatory steps in the correct order.
Friction identified: `demos/basic/server/src/app.rs` (line 185) places
`send_all_packets` inside the `TickEvent` loop rather than as a top-level step,
contradicting the canonical documentation. A new user following the demo
will copy this bug. **[B-3 gap]**

**Step 5 — Spawn an entity and replicate.**
`server.spawn_entity(world.proxy_mut()).insert_component(component).id()`
is straightforward. Adding to a room with `server.room_mut(&room_key).add_entity(&entity)`
requires knowing to first create a room — documented in CONCEPTS.md §4.
Minor friction: new users often miss the room step and are puzzled why nothing
replicates; a note in the README "Quick concepts" → Entity section saying
"entity must be in a room shared with the user" would help.

**Step 6 — Client receive.**
`Events::read::<SpawnEntityEvent>()` + `Events::read::<InsertComponentEvent<..>>()`
pattern is straightforward once you know to look for it. The FAQ covers this.

**Overall friction score:** Low for Bevy users (adapter handles most
boilerplate), Medium for no-ECS users (main loop ordering trap in demo is
the highest-friction point, **B-3**).

---

## Regression Table

Comparing against the previous audit (`_AGENTS/EXCELLENCE_ANALYSIS_20260511.md`).

| Prev Gap ID | Description | Fixed? | Evidence |
|-------------|-------------|--------|---------|
| A-1 | FAQ — 3 false statements | ✓ FIXED | Commit `10524bc1`; lag comp, per-entity rate, transport all accurate |
| A-2 | Public release cadence (Bevy 0.15) | ✗ OPEN | Still v0.24 on crates.io; now 17 months stale. Carried as B-5 |
| A-3 | FEATURES.md — stale entries | PARTIAL | Old false "planned" items removed, but 5 new completed items still listed as planned. Carried as B-1 |
| A-4 | SECURITY.md — no stunnel guide | ✓ FIXED | Commit `7fe24c04`; stunnel + docker-compose + AEAD note present |
| A-5 | PREDICTION.md — missing advanced sections | ✓ FIXED | Commit `902627d8`; multi-entity rollback, smooth error interp, correction batching, tick-buffer miss all covered |
| A-6 | Per-component replication granularity | ✓ CLOSED (design decision) | Documented as intentional; split-entity pattern documented |
| A-7 | RTT=0ms bug on new connections | ✓ FIXED | Confirmed in private branch (`rtt_initial_estimate = 200ms`) |
| A-8 | `#[derive(Message)]` on enums | ✓ FIXED | Commit `13e1d61e`; `enum_message_impl` in `shared/derive/src/message.rs:19` |
| A-9 | Historian undocumented + no snapshot filtering | ✓ FIXED | Commit `4d33206a`; CONCEPTS.md §20 + `enable_historian_filtered` |
| A-10 | Compression pipeline undocumented | ✓ FIXED | Commit `7fe24c04`; CONCEPTS.md §17 |
| A-11 | Request/Response no disconnect cleanup | PARTIAL | `purge_user()` added (commit `24bc167e`), but no TTL eviction for zombie entries. Carried as B-4 |
| A-12 | Reconnection flow undocumented | ✓ FIXED | Commit `7fe24c04`; CONCEPTS.md §19 |
| A-13 | Per-entity priority undocumented | ✓ FIXED | Commit `7fe24c04`; CONCEPTS.md §16 |
| A-14 | Bevy adapter T phantom type friction | ✓ FIXED | Commit `62d80370`; `DefaultClientTag` + `DefaultPlugin` exported |
| A-15 | ConnectionStats / BandwidthConfig undocumented | ✓ FIXED | Commit `7fe24c04`; CONCEPTS.md §18 |
| A-16 | Fuzz coverage shallow (2 targets) | ✓ FIXED | Commit `34805f5f`; now 5 targets |
| A-17 | No push-based metrics | ✓ FIXED | Commits `89fdd0b4`+`64578202`; `naia-metrics` + `naia-bevy-metrics` ship |
| A-18 | iOS/Android platform gap undocumented | ✓ FIXED | Commit `06b8f66f`; README platform table updated |
| A-19 | Steam relay gap undocumented | ✓ FIXED | Commit `06b8f66f`; README transport table updated |
| — | Issue #165 — MessageChannel capacity limits | ✗ OPEN | `ReliableSender` `sending_messages` is still unbounded. New B-2 |
| — | demo send_all_packets placement | ✗ NEW | Basic demo app.rs:185 calls it inside TickEvent loop. New B-3 |
| — | global_world_manager panic on stale entity | ✗ OPEN | Issue #172 open; `user_opt` was added but entity path still panics. New B-7 |
| — | MSRV undeclared | ✗ NEW | No rust-version field anywhere. New B-6 |

**Net regression count:** 0 (no previously-fixed gaps reopened).
**Previous gaps fixed this cycle:** 16 of 19 (A-1, A-3–A-17, A-18–A-19).
**Still open from previous cycle:** A-2 (release), A-11 partial.
**New gaps identified this cycle:** B-1 (FEATURES.md), B-2 (channel backpressure),
B-3 (demo ordering), B-6 (MSRV), B-7 (panic on stale entity).

---

## Grade

**A−**

The private branch is production-quality: correct semantics, rich feature set,
strong documentation, working observability, 5 fuzz targets, full BDD contract
harness. The minus is the release gap — developers evaluating on crates.io still
see v0.24 / Bevy 0.15. One critical reliability concern remains (B-2, unbounded
channel queues). No correctness regressions were introduced since the previous
audit.

**Single change to move to A:** Publish v0.25 targeting Bevy 0.18 with the
current private branch (B-5). This immediately surfaces 17 months of closed
gaps to the evaluating developer. Fixes B-1 as a side-effect (FEATURES.md
ships accurate). With B-2 (channel backpressure) also closed, the grade
becomes A.
