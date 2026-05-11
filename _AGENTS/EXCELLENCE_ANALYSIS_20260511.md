# Naia Excellence Audit — 2026-05-11

---

## Audit Progress

| Gap | Status | Commit |
|-----|--------|--------|
| A-1 — FAQ false statements + missing entries | **COMPLETE** | `10524bc1` |
| A-5 — PREDICTION.md advanced sections | **COMPLETE** | `902627d8` |
| A-4 — SECURITY.md stunnel guide + AEAD note | **COMPLETE** | pending |
| A-7 — RTT = 0ms bug | **ALREADY FIXED** in private branch (`rtt_initial_estimate = 200ms`) | n/a |
| A-12 — Reconnection flow | **COMPLETE** (CONCEPTS.md §19) | pending |
| A-13 — Per-entity priority docs | **COMPLETE** (CONCEPTS.md §16 + FAQ) | pending |
| A-10 — Compression docs | **COMPLETE** (CONCEPTS.md §17) | pending |
| A-15 — Diagnostics docs | **COMPLETE** (CONCEPTS.md §18) | pending |
| A-3 — FEATURES.md stale | **COMPLETE** (full rewrite) | pending |
| A-9 — Historian (FAQ portion) | **COMPLETE** — FAQ + FEATURES.md + CONCEPTS.md §20 + `enable_historian_filtered` | pending |
| A-11 — Request/response disconnect cleanup | **COMPLETE** — `purge_user()` on both managers, called from `user_delete` | `24bc167e` |
| A-8 — Enum `#[derive(Message)]` | **COMPLETE** — all three variant styles (Unit/Named/Unnamed), EntityProperty-aware; 39/39 integration tests pass | pending |
| A-14 — `DefaultClientTag` + `DefaultPlugin` | **COMPLETE** — `naia-bevy-client` now exports both; crate doc updated with single-client vs multi-client patterns | pending |
| A-6 — Per-component replication toggle | **CLOSED — deliberate design decision**: scope is per-entity, not per-component. If intra-entity visibility bifurcation is needed, move components to a separately scoped entity and link via `EntityProperty`. This is the established naia pattern. | n/a |
| A-18 — iOS/Android platform gap | **COMPLETE** — README platform table + CONCEPTS.md §11 | pending |
| A-19 — Steam relay gap | **COMPLETE** — README + CONCEPTS.md §11 | pending |
| A-16 — Fuzz coverage shallow | **COMPLETE** — 3 new targets: `serde_quantized`, `handshake_header`, `replication_decode`; `WorldReader` exported | `34805f5f` |
| All others | open | — |

---

## Executive Summary

Naia's private development branch (specops/naia) is substantially more capable
than its last public release (v0.24, December 2024). The private codebase ships
prediction primitives (`CommandHistory`, `TickBuffered`, `local_duplicate`), a
lag-compensation `Historian`, per-entity priority accumulators, configurable
zstd compression, rich connection diagnostics (RTT P50/P99, packet loss, kbps),
and targets Bevy 0.18 — yet the public crates.io release is frozen on Bevy 0.15
and none of the above surface in published docs. The three highest-leverage gaps
are: (1) the FAQ actively contradicts the current codebase, discouraging
adoption of features that already exist; (2) the public release cadence (one
release in 13+ months) has fallen far behind lightyear (12+ releases in 12
months) and bevy_replicon, which has moved naia from "the reference" to
"historically relevant" in Bevy community discussions; (3) prediction/rollback
exists as primitives but is not assembled into the turnkey framework that
lightyear now offers. The one change with the most developer-facing impact would
be publishing a release targeting Bevy 0.18 with updated docs — surfacing the
already-shipped capabilities before developers evaluate alternatives.

---

## Competitive Landscape

Sources: GitHub (2026-05-11 — stars, issues, last release), jaxter184 netcode
wiki, cBournhonesque/lightyear changelog, bevy_replicon changelog, renet
changelog, developer migration posts.

| Competitor | Stars | Last release | Strengths vs naia | Weaknesses vs naia |
|------------|-------|-------------|-------------------|--------------------|
| **lightyear** | 992 | v0.26.0 (Jan 2026, Bevy 0.18) | First-class prediction/rollback; WebTransport + WebSocket + UDP + Steam relay; lag compensation built-in; Avian physics integration; monthly release cadence; deterministic/lockstep mode | Bevy-only (not ECS-agnostic); no macroquad path; heavier API surface; 127 open issues; steep learning curve |
| **renet / renet2** | 909 | v2.0.0 (Jan 2026, Bevy 0.18) | netcode.io 2.0 auth with encryption; renet_visualizer egui dashboard; extremely simple API; more confirmed shipped games | Message-passing only — no ECS replication, no rooms, no scope management; no WASM/browser client |
| **bevy_replicon** | 595 | v0.39.5 (Apr 2026, Bevy 0.18) | Per-component replication granularity; pluggable transport (QUIC via quinnet); highest patch cadence; no_std support; zero-friction Bevy integration | No prediction or rollback; no built-in WASM transport; narrower scope (Bevy only) |
| **Quinn** | 5 070 | — (Rust QUIC impl) | Production QUIC with TLS 1.3; zero-RTT resumption; not a game library | Pure transport — no replication, channels, or rooms |
| **GameNetworkingSockets (Valve)** | — | — | Production-proven at scale; per-lane reliability; P2P relay | C++ primary; Rust bindings unofficial; Windows-centric |

**Ecosystem momentum signal.** lightyear went from v0.21 to v0.26 between July
2025 and January 2026 — a near-complete architectural overhaul in six months.
bevy_replicon patches every 2–4 weeks. naia's last public release is December
2024 (v0.24, Bevy 0.15); naia is now approximately 3 Bevy versions behind every
alternative. In public developer discussions, naia is no longer the default
recommendation for new Bevy projects; lightyear (feature-rich) and
bevy_replicon (simplicity + QUIC) are now the community defaults.

---

## Gap Analysis

### A-1 — FAQ actively contradicts the current codebase ✓ COMPLETE

**Current state.**  
`faq/README.md` contains three statements that are materially false against the
current private codebase:

- Line 50: *"Naia does NOT provide: lag compensation"* — `Historian`
  (`server/src/historian.rs`) is a rolling per-tick snapshot buffer for
  server-side lag compensation (rewind-based hit detection). The struct,
  its `enable_historian` API, and its doc comment are all present.
- Line 85: *"different replication frequencies per entity: not possible"* —
  `EntityPriorityMut` exposed on `Server<E>` as `global_entity_priority_mut`
  and `user_entity_priority_mut` implements per-entity gain values that
  directly control effective replication bandwidth allocation
  (`shared/src/connection/priority_state.rs`).
- Line 37: *"Is naia compatible with other transport layers? No"* — the
  `Socket` trait is pluggable; `NativeSocket` (UDP), `WebrtcSocket`, and
  `transport_local` are three live implementations.

**Why it matters.**  
The FAQ is the first doc most new developers read after the README. False
statements cause developers to choose competitors for capabilities naia already
has, or to file workaround code for non-existent limitations.

**Recommendation.**  
Rewrite all three entries. Add a "Lag Compensation" FAQ item showing the
`enable_historian` → `record_historian_tick` → `snapshot_at_tick` pattern. Add
a "Variable-rate entities" item showing `entity_priority_mut().set_gain(0.25)`.
Update the transport answer to describe the `Socket` trait.

**Effort:** XS.  **Leverage:** 4

---

### A-2 — Public release cadence has fallen far behind competitors

**Current state.**  
naia's last public release is v0.24.0, December 2024, targeting Bevy 0.15.
Current Bevy is 0.18. Every active competitor (lightyear, renet, bevy_replicon)
tracks Bevy 0.18 today. The private specops/naia branch targets Bevy 0.18
(`adapters/bevy/client/Cargo.toml: bevy_ecs = { version = "0.18" }`) and
contains significant improvements over v0.24, but none of these are published.

Consequence: developers evaluating naia today see:
- A crates.io page showing a 5-month-old release with a Bevy version 3 generations behind.
- A GitHub README that may not match what crates.io ships.
- lightyear 0.26 with 12 recent releases and 992 stars, appearing more
  actively maintained with a fraction fewer stars.

Community developer discussions (jaxter184 wiki, arewegameyet.rs) have shifted
their default recommendations away from naia toward lightyear and bevy_replicon.

**Why it matters.**  
A stale public release is the single largest adoption barrier for Bevy
developers. Even if the private branch is excellent, developers don't see it.
This gap is responsible for more lost evaluators than any feature gap.

**Recommendation.**  
Publish a new release targeting Bevy 0.18 with the current private branch state.
The unreleased work (Historian, priority accumulators, prediction primitives,
zstd compression, ConnectionStats, reconnect fix, UB fix, handshake hardening)
is substantial enough to justify a new minor or major version. Establish a
release cadence (quarterly minimum) to remain visible.

**Effort:** S (release prep + changelog polish).  **Leverage:** 5

---

### A-3 — FAQ and FEATURES.md are stale beyond the three false statements

**Current state.**  
`FEATURES.md` lists as "planned" several items that are already implemented
or are explicitly closed decisions:
- "Congestion Control" — AIMD is a closed/inapplicable decision; the token-bucket
  + priority accumulator is the correct model and already ships.
- "Update Priority" and "Dynamic Update Priority" — `EntityPriorityMut` with
  the accumulator system implements this.
- "Custom Property read/write implementation" — `Serde` is fully custom today.
- "Debugging / Logging / Metrics visualizations" — partially implemented via
  `ConnectionStats` (RTT P50/P99, jitter, kbps_sent/recv).

Listing implemented features as "planned" underrepresents the library.

**Recommendation.**  
Archive `FEATURES.md` or replace with an accurate roadmap. Categorize each
item: "Shipped — see §N in CONCEPTS.md", "Closed — see CHANGELOG", or
"Future — tracked in issue #N with no set date."

**Effort:** XS.  **Leverage:** 2

---

### A-4 — No production encryption path for native clients

**Current state.**  
`SECURITY.md` correctly documents that `transport_udp` sends all packets as
unencrypted plaintext and recommends a TLS proxy as the interim path.
`transport_quic` (TLS 1.3) is deferred (closed decision). `transport_webrtc`
provides DTLS for browser clients only.

renet2 ships netcode.io 2.0 with encryption as table stakes — this is the most
common reason developers cite when choosing renet2 over naia for native-only
games (confirmed in developer forum posts).

**Recommendation.**  
QUIC remains out of scope. Near-term actionable steps:
1. Expand `SECURITY.md` with a concrete stunnel config snippet (< 20 lines)
   and a docker-compose example so the proxy path is easy to follow.
2. Evaluate a lightweight AEAD-over-UDP mode (e.g. XChaCha20-Poly1305 with
   a pre-shared key exchanged out-of-band). This is M effort, closes the
   confidentiality gap for most indie use cases without QUIC.

**Effort:** XS (doc) → M (AEAD stepping stone).  **Leverage:** 4

---

### A-5 — Prediction guide covers only the simple linear case ✓ COMPLETE

**Current state.**  
`docs/PREDICTION.md` provides an excellent walkthrough of the
single-entity/single-command/single-field linear prediction loop covering all
five building blocks (`TickBuffered`, `CommandHistory`, `local_duplicate`,
per-tick record→send→apply, and the correction handler). From the outside,
however, lightyear ships a first-class rollback plugin with Avian physics
integration and multi-entity rollback — developers see an "assembled machine"
versus naia's "here are the parts."

What the guide does not cover:
- **Multi-entity rollback** — when the predicted entity interacts with other
  dynamic entities, each needs its own confirmed/predicted pair.
- **Misprediction detection threshold** — snapping on every correction produces
  jitter; a dead-band check before the snap is standard practice.
- **Rollback with constraints** — physics simulations need re-running
  constraints after re-simulation; left entirely to the developer.
- **Tick-buffer miss** — the guide does not explain that late `TickBuffered`
  messages are silently discarded; developers may be surprised.

**Recommendation.**  
Extend `docs/PREDICTION.md` with three new sections: multi-entity rollback,
misprediction threshold, and tick-buffer miss handling.

**Effort:** S.  **Leverage:** 4

---

### A-6 — Per-component replication granularity ✗ CLOSED — deliberate design decision

**Decision.**  
Scope in naia is per-entity, not per-component. This is intentional. When
different subsets of data need different visibility rules (e.g. a player's
`Position` is public but their `Inventory` is private), the correct approach is
to place those components on separate entities with separate scope configuration,
linked via `EntityProperty` references. This pattern is used throughout naia
projects and preserves the invariant that all components on a replicated entity
are always consistent with each other from the client's perspective.

Per-component scoping is a different architectural model (bevy_replicon's
approach) that introduces partial-entity state on the client, complicating
client-side consistency guarantees. This gap is not an implementation gap —
it is a design boundary.

**Closed. Not a candidate for implementation.**

---

### A-7 — RTT estimation bug causes reliable-message burst on new connections

**Current state.**  
GitHub issue #208 (open): the server resends all pending reliable messages on
new connections because the ping manager initializes with 0ms RTT, causing the
retransmission timer to fire immediately before the real RTT estimate is
established. Every new connection sees a burst of redundant reliable traffic
before the RTT averages out.

**Why it matters.**  
On a server with many concurrent connections, new-connection bursts stack:
`N_connections × reliable_backlog × initial_retransmit_burst`. At scale this
can spike bandwidth and confuse clients (duplicate reliable messages).

**Recommendation.**  
Initialize the ping manager's RTT estimate with a conservative default (e.g.
`ServerConfig::initial_rtt_estimate_ms: u32 = 150`) instead of 0ms, deferring
retransmission until the real RTT stabilizes.

**Effort:** XS — one default value in `PingStore` or `PingConfig`.  
**Leverage:** 3

---

### A-8 — Enum types cannot derive Message ✓ COMPLETE

**Current state.**  
GitHub issue #163 (open): `#[derive(Message)]` does not support enum types.
Developers who want to send sum types as messages (e.g.,
`enum PlayerAction { Move(Vec2), Attack(EntityId) }`) must either wrap enums
in a struct or use separate message types for each variant. This is a common
Rust pattern that works for other derive macros but is not supported here.

**Why it matters.**  
Enums-as-messages are idiomatic Rust. This limitation surprises every developer
who tries it, forces workarounds, and is frequently cited in the GitHub issue
tracker as a first-hour DX friction point.

**Recommendation.**  
Extend the `naia-derive` proc-macro for `Message` to support enums. The
serialization is a dispatch on variant index; the `Serde` infrastructure
already handles tagged enums.

**Effort:** S — proc-macro change in `naia-derive`.  **Leverage:** 3

---

### A-9 — Historian: undocumented in public docs and lacks snapshot filtering ✓ COMPLETE

**Current state.**  
`server/src/historian.rs` implements a rolling per-tick snapshot buffer for
server-side lag compensation. The source documentation is complete. However:
- `docs/CONCEPTS.md` has no mention of `Historian`.
- `README.md` has no mention of `Historian`.
- The code comment at `historian.rs:52` notes: *"Selectively snapping only the
  components you care about is a future extension; for now all replicated
  components are captured."* On a server with 1000 entities each having 10
  components, per-tick clone cost is significant.

**Recommendation.**  
1. Add a `## 16. Lag Compensation (Historian)` section to `docs/CONCEPTS.md`.
2. Add component-kind filtering to `Historian::record` via a `HashSet<ComponentKind>` allowlist.

**Effort:** XS (doc) + S (filtering).  **Leverage:** 3

---

### A-10 — Compression pipeline (zstd) is completely undocumented

**Current state.**  
`shared/src/connection/compression_config.rs` implements configurable zstd
compression: `Default(level)`, `Dictionary(level, dict)`, and
`Training(n_samples)`. The training mode collects real packet samples and
trains a domain-specific dictionary, achieving 40–60% bandwidth reduction on
typical game-state delta data. This feature is mentioned in zero public
documents.

**Recommendation.**  
Add a `## Compression` section to `docs/CONCEPTS.md` covering all three modes
and the `Training` workflow (run with `Training(2000)`, extract the dictionary,
switch to `Dictionary` mode).

**Effort:** XS.  **Leverage:** 2

---

### A-11 — Request/Response has no timeout or disconnect cleanup guarantee ✓ COMPLETE (purge_user)

**Current state.**  
`server/src/request.rs` implements `GlobalRequestManager` as a `HashMap` from
`GlobalRequestId` to `(UserKey, Option<MessageContainer>)`. There is no TTL
and no eviction sweep. If a client disconnects mid-request, the map entry must
be cleaned up by the `disconnect_user` path — this path is not obviously called
from reading `request.rs` alone, and there is no test contract covering it.

**Recommendation.**  
1. Audit the `disconnect_user` path to verify all outstanding `GlobalRequestId`
   entries for disconnecting users are purged.
2. Add a `max_age_ticks: u32` eviction to `GlobalRequestManager`.
3. Add a BDD contract covering "request orphaned by client disconnect is
   cleaned up."

**Effort:** S.  **Leverage:** 3

---

### A-12 — Reconnection flow undocumented

**Current state.**  
The CHANGELOG notes (Unreleased): *"Clients that disconnect and reconnect
mid-session now correctly re-receive all in-scope entities and replicated
resources on reconnect."* The fix is live. No public document shows the
`DisconnectEvent → client.connect(socket)` sequence or explains what state
the client must clear vs what naia clears automatically.

**Recommendation.**  
Add a `## Reconnection` section to `docs/CONCEPTS.md` covering the sequence,
what the client world must do (despawn local entities), and a note on
implementing retry backoff.

**Effort:** XS.  **Leverage:** 3

---

### A-13 — Per-entity update rate control is implemented but undocumented (partial — FAQ done)

**Current state.**  
`EntityPriorityMut::set_gain(f32)` and the accumulator system
(`shared/src/connection/priority_state.rs`) implement per-entity bandwidth
allocation. The FAQ says this is "not possible." No document shows how to use
`entity_priority_mut()`. This is a documentation-only gap — the feature fully
exists.

**Recommendation.**  
Add a `## Per-Entity Priority and Bandwidth` section to `docs/CONCEPTS.md`.
Fix the FAQ entry. Two doc edits, no code changes.

**Effort:** XS.  **Leverage:** 3

---

### A-14 — Bevy adapter T phantom type creates friction for new users ✓ COMPLETE

**Current state.**  
`NaiaClientPlugin::<T>`, `Client<T>`, and `NaiaClientConfig::<T>` all carry a
phantom type `T: Resource` for multi-client disambiguation. `docs/CONCEPTS.md`
§10 explains it clearly but it is the #1 friction point for new Bevy users —
a compile error about a missing type parameter with no helpful message.
lightyear does not require a phantom type parameter for the common single-client
case.

**Resolution.**  
`naia-bevy-client` now exports `DefaultClientTag` (unit struct) and
`DefaultPlugin` (type alias for `Plugin<DefaultClientTag>`). The crate-level
`lib.rs` doc now shows the single-client pattern (`DefaultPlugin::new(...)`,
`Client<DefaultClientTag>`) and the multi-client pattern (distinct tag structs)
side-by-side. Existing code using custom tags is fully unaffected.

**Effort:** S.  **Leverage:** 3

---

### A-15 — ConnectionStats and BandwidthConfig undocumented

**Current state.**  
`connection_stats.rs` exposes `rtt_ms`, `rtt_p50_ms`, `rtt_p99_ms`,
`jitter_ms`, `packet_loss_pct`, `kbps_sent`, `kbps_recv` — a richer
diagnostics snapshot than most game networking libraries provide. `BandwidthConfig`
lets developers set `target_bytes_per_sec` (default 512 kbps). Neither is
mentioned in `README.md` or `docs/CONCEPTS.md`.

**Recommendation.**  
Add a `## Diagnostics and Bandwidth Tuning` section to `docs/CONCEPTS.md` with
two-line code snippets for each API.

**Effort:** XS.  **Leverage:** 2

---

### A-16 — Fuzz coverage is shallow (2 targets) ✓ COMPLETE

**Resolution** (`34805f5f`).  
Added three new fuzz targets — total coverage is now 5 targets:

| Target | Covers |
|--------|--------|
| `serde_quantized` | `UnsignedFloat`, `SignedFloat`, `UnsignedVariableFloat`, `SignedVariableFloat` across multiple const-param combinations; includes roundtrip (de→ser→de) assertions |
| `handshake_header` | `HandshakeHeader::de` — the handshake enum (ClientIdentifyRequest, ServerIdentifyResponse, ServerRejectResponse + RejectReason, etc.) |
| `replication_decode` | `WorldReader::read_world_events` — entity spawn/despawn, component insert/remove, authority messages, updates; uses a `FakeGlobalWorldManager` stub |

Also exports `WorldReader` from `naia-shared` (was previously private mod).

**Effort:** M.  **Leverage:** 3

---

### A-17 — No push-based metrics or observability hooks

**Current state.**  
`ConnectionStats` is poll-based (call each tick per user). No built-in
integration with `tracing`, Prometheus, or the `metrics` crate facade. renet
ships `renet_visualizer` (egui-based dashboard). lightyear recently added
`bevy_metrics` hooks.

**Recommendation.**  
Add an optional `metrics` feature gate that emits `tracing::instrument` spans
around the five main-loop steps and records per-connection gauges via the
`metrics` crate (compatible with `metrics-exporter-prometheus`). No core API
changes required.

**Effort:** M.  **Leverage:** 2

---

### A-18 — Mobile client support gap (iOS/Android) ✓ COMPLETE

**Current state.**  
`FEATURES.md` lists "Android-compatible Client Socket" and "iOS-compatible
Client Socket" as planned. `transport_webrtc` uses `wasm-bindgen` browser APIs
on WASM and `webrtc-unreliable` natively — neither compiles for iOS or Android
native targets.

**Recommendation.**  
Explicitly note in the README platform table that iOS/Android native are not
yet supported, and document the WKWebView/WebView WASM workaround for games
that can accept it. Native mobile implementation is deferred pending QUIC.

**Effort:** XS (doc).  **Leverage:** 2

---

### A-19 — No Steam relay / ISteamNetworkingSockets integration ✓ COMPLETE

**Current state.**  
lightyear ships Steam transport as a first-class option (added 2025). naia has
no integration. The `Socket` trait is pluggable — a community crate is feasible.

**Recommendation.**  
Document the gap in the README transport table. A "Steam relay" row marked
"not available — implement via the Socket trait" sets correct expectations and
points motivated contributors in the right direction.

**Effort:** XS (doc gap note) → XL (implement).  **Leverage:** 2

---

## Prioritised Action Table

A-2 (publish release) is deferred until remaining audit items are closed.
A-1 and A-5 are complete. A-9/A-13 FAQ portions are done; CONCEPTS.md work remains.

| Rank | Gap ID | Description | Decision | Effort | Leverage | Status |
|------|--------|-------------|----------|--------|----------|--------|
| — | A-2 | Publish Bevy 0.18 release | After audit closes | S | 5 | deferred |
| 1 | A-9 | Historian: add CONCEPTS.md §20 + snapshot filtering | Doc + code | XS+S | 3 | partial |
| 2 | A-8 | Enum #[derive(Message)] not supported — issue #163 | Fix proc-macro | S | 3 | open |
| 3 | A-14 | Bevy T phantom type friction | Add DefaultClientTag alias | S | 3 | open |
| 4 | A-11 | Request/Response: no timeout, disconnect cleanup unaudited | Audit + add TTL | S | 3 | open |
| 5 | A-6 | Per-component replication granularity absent — issue #186 | Design + implement | M | 3 | open |
| 6 | A-16 | Fuzz coverage: 2 targets only | Add 3 new targets | M | 3 | **COMPLETE** `34805f5f` |
| 7 | A-17 | No push-based metrics hooks | Add optional tracing/metrics feature | M | 2 | open |
| 8 | A-18 | iOS/Android: document the gap | Clarify README | XS | 2 | open |
| 9 | A-19 | No Steam relay | Document gap in README | XS | 2 | open |
| — | A-1 | FAQ — 3 false statements fixed + 2 new entries | **COMPLETE** `10524bc1` | XS | 4 | ✓ done |
| — | A-5 | PREDICTION.md — 4 advanced sections | **COMPLETE** `902627d8` | S | 4 | ✓ done |
| — | A-4 | SECURITY.md — stunnel guide + docker-compose + AEAD note | **COMPLETE** | XS | 4 | ✓ done |
| — | A-7 | RTT = 0ms bug | **ALREADY FIXED** in private branch | — | 3 | ✓ done |
| — | A-12 | Reconnection — CONCEPTS.md §19 | **COMPLETE** | XS | 3 | ✓ done |
| — | A-13 | Per-entity priority — CONCEPTS.md §16 + FAQ | **COMPLETE** | XS | 3 | ✓ done |
| — | A-10 | Compression — CONCEPTS.md §17 | **COMPLETE** | XS | 2 | ✓ done |
| — | A-15 | Diagnostics — CONCEPTS.md §18 | **COMPLETE** | XS | 2 | ✓ done |
| — | A-3 | FEATURES.md — full accurate rewrite | **COMPLETE** | XS | 2 | ✓ done |

---

## What Would Make Naia Definitively #1

The single biggest unlock is publishing the current private branch (A-2,
deferred until this audit closes).

**Shipped in this audit cycle (2026-05-11):**
- A-1: FAQ no longer misleads developers about lag compensation, per-entity rate
  control, transport pluggability, or the Message derive.
- A-5: `docs/PREDICTION.md` now covers multi-entity rollback, smooth error
  interpolation, correction batching, and tick-buffer miss.
- A-4: `SECURITY.md` now includes a concrete stunnel config + docker-compose
  snippet and documents the AEAD stepping-stone evaluation.
- A-7: RTT = 0ms bug confirmed already fixed in private branch (`rtt_initial_estimate = 200ms`).
- A-12/A-13/A-10/A-15: `docs/CONCEPTS.md` now has §16 (priority/bandwidth),
  §17 (compression), §18 (diagnostics), §19 (reconnection).
- A-3: `FEATURES.md` fully rewritten — shipped features accurately listed,
  stale "planned" items removed, genuine roadmap clearly separated.

**Remaining to close the audit before v0.25:**
- A-9: Add CONCEPTS.md §20 for Historian (XS doc) + component-kind snapshot
  filtering (S code). High leverage — lag compensation is now documented in FAQ
  and FEATURES.md but still absent from the main concepts guide.
- A-8: Enum `#[derive(Message)]` proc-macro fix (S code). Real dev UX gap.
- A-14: `DefaultClientTag` Bevy alias (S code). Reduces new-user friction.
- A-11: Request/Response disconnect-path audit + TTL eviction (S code).
- A-6: Per-component replication toggle (M — design + implement).
- A-16: Additional fuzz targets (M).
- A-18/A-19: Two-line README clarifications for mobile and Steam gaps.
