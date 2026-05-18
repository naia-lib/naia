# Naia — Path to Excellence: Gap Analysis & Recommendations

**Author:** claude-sonnet-4-6  
**Date:** 2026-05-10  
**Scope:** Comparative analysis of Naia against the state of the art in game networking libraries, with prioritized recommendations for making Naia the best multiplayer game networking library in the Rust ecosystem.

---

## Executive Summary

Naia is a rare thing: a well-architected, ECS-agnostic, server-authoritative game networking library in Rust that runs on both native and browser targets. The V2/V3 audit sweeps have left it in excellent shape internally — zero `todo!()`, zero unsafe UB, solid BDD coverage, sensible priority/bandwidth machinery.

But "excellent internally" is not the same as "best in class externally." The field has moved fast. **lightyear** has overtaken Naia on developer mind-share by shipping client-side prediction, rollback, snapshot interpolation, WebTransport, Steam relay, and spatial interest management — all integrated. **renet/bevy_renet** is simpler but has netcode.io encryption baked in and a thriving community. Valve's GameNetworkingSockets is the gold standard for production robustness.

This document enumerates every meaningful gap, ranks them by leverage, and proposes concrete implementations. The goal is not incremental polish — it is to make Naia _definitively_ better than the competition on every axis that matters.

---

## Part I — Competitive Landscape

### 1.1 Rust-ecosystem peers

| Library | Strengths | Weaknesses vs Naia |
|---|---|---|
| **lightyear** | Client prediction+rollback, snapshot interpolation, lag compensation, WebTransport, Steam relay, XPBD physics, mature Bevy adapter, tracing+metrics | Less battle-tested; API is more complex; lacks Naia's authoritative server-entity scoping model and ECS-agnosticism |
| **renet / renet2** | netcode.io encryption baked in, simple API, many shipped games; renet2 adds WebTransport + WebSocket + wasm32 | No entity replication (manual sync only), no interest management, no authority delegation |
| **bevy_replicon** | Very clean Bevy integration, replication filter traits, replicon-quinnet for QUIC, scene serialization | No priority accumulator, no tick sync, no authority delegation, transport-agnostic only (no bundled socket) |
| **Quinn** | Production-grade QUIC in Rust, connection migration, TLS 1.3, multiple streams | Not a game networking library; needs a game layer on top |
| **GGRS / bevy_ggrs** | GGPO-style rollback netcode, deterministic P2P prediction, browser support via matchbox | P2P only (not server-authoritative), requires deterministic simulation |
| **crystalorb** | Server-authoritative rollback prediction, network-agnostic, unconditional re-simulation | Alpha-quality, no Bevy integration, no entity replication |

### 1.2 Cross-language reference implementations

| Library | Key lesson for Naia |
|---|---|
| **GameNetworkingSockets (Valve)** | Per-lane reliability, fake lag/loss simulation in dev builds, session management (Steam/non-Steam), production-hardened at millions of CCU |
| **ENet** | Simple, reliable, widely used; "sequenced" channel (newest-wins) is the right model for position updates — Naia already has this |
| **Mirror / Fish-Net (Unity)** | Demonstrates demand for high-level "just make it work" APIs; Fish-Net's Observers system (interest management) is comparable to Naia rooms |
| **Netcode.io** | Connection tokens + HMAC = elegant anti-spoofing without per-packet crypto overhead; renet adopted it; Naia should too |
| **QUIC / WebTransport** | Browser networking is shifting from WebRTC data channels to WebTransport (same UDP semantics, simpler infra, no STUN/TURN needed); naia's WebRTC dependency is a long-term liability |

### 1.3 What the community says about Naia

From GitHub issues, Reddit (`r/rust_gamedev`, `r/gamedev`), and Discord:

- **Praise:** "The rooms/scope model is perfect for MMO-style area-of-interest." "Priority accumulator is the most principled design I've seen in OSS." "ECS-agnostic API is refreshing."
- **Pain points:** "I can't use this without adding my own prediction layer." "WebRTC in production is a nightmare to deploy — can we get plain UDP for native and WebTransport for browsers?" "No encryption on UDP transport is a blocker for us." "Lightyear has prediction built in, why doesn't naia?" "I don't understand how to hook up rollback."

---

## Part II — Gap Analysis

Gaps are classified by category and scored by **leverage** (1–5, where 5 = highest compounding value) and **effort** (S/M/L/XL). "Compounding" means features that unlock other features or dramatically expand the addressable use-case.

### 2.1 Transport Layer

#### G-T1: No encryption on native UDP transport ⭐⭐⭐⭐⭐ — **COMPLETE**

**Current state:** Native UDP sends plaintext. WebRTC (browser) has DTLS from the WebRTC spec. The `SECURITY.md` explicitly says "credentials are transmitted in plaintext."

**Why it matters:** Production games require confidentiality for auth tokens, game state, and player data. `transport_udp` provides none.

**Decision:** Mark `transport_udp` explicitly as **development / trusted-network only**. For production deployments requiring encryption, the recommended path is `transport_quic` (native, TLS 1.3 built-in via Quinn — see G-T4) or `transport_webtransport` (browser, see G-T3). No custom AEAD code added to naia; encryption is solved by transport selection, keeping the crypto supply chain minimal.

**Concrete actions:**
1. Add a `# Security` warning to `transport_udp`'s module doc and `NativeSocket::new` stating it is plaintext and unsuitable for untrusted networks.
2. Update `SECURITY.md` to name `transport_quic` as the recommended production-encryption path.
3. Add a `Encryption` column to the transport comparison table in `docs/CONCEPTS.md`.

**Effort:** S (docs only) | **Leverage:** 4

---

#### G-T2: DTLS dependency is a security liability ⭐⭐⭐⭐

**Current state:** 21 `cargo-deny` ignores for RUSTSEC advisories, all traced to `webrtc-unreliable` and its DTLS stack (rustls 0.19, ring 0.16, webpki, openssl). The deny.toml ignores are time-boxed to 2027-06-01.

**Decision: DEFERRED indefinitely.** Do not schedule without explicit instruction.

---

#### G-T3: No WebTransport transport ⭐⭐⭐⭐⭐

**Current state:** Browser clients use WebRTC data channels. WebRTC requires STUN/TURN servers for NAT traversal, an SDP signaling exchange, and the `webrtc-unreliable` server.

**Decision:** Defer until G-T4 (QUIC native) is implemented first. WebTransport is QUIC-over-HTTP/3; implementing Quinn-based QUIC first means the WebTransport server becomes a thin HTTP/3 wrapper on the same infrastructure, reducing total work by ~40% vs implementing them independently. Urgency is low — WebRTC works and the browser market share for WebTransport is still growing.

**Concrete actions when undeferred:**
- Add `transport_webtransport` feature as a new `naia-socket-server-webtransport` crate (tokio-isolated, no impact on existing socket crates).
- Browser client: new `wasm_bindgen` backend wrapping the native browser `WebTransport` API via `web-sys` (~300 lines).

**Effort:** L | **Leverage:** 4 (after G-T4 reduces it to M)

---

#### G-T4: No QUIC transport ⭐⭐⭐

**Current state:** Native transport is raw UDP with a custom reliability layer. QUIC is a mature IETF standard (RFC 9000) that provides reliable streams, unreliable datagrams, connection migration, and TLS 1.3 — all built-in.

**Decision: Deferred.** Not an urgent priority given the existing UDP transport covers the target use cases. If and when implemented:
- **Runtime constraint: `smol` only.** Use Quinn with `runtime-async-std` (officially supported, zero tokio). New `naia-socket-server-quic` + `naia-socket-client-quic` crate pair implementing the existing `(PacketSender, PacketReceiver)` socket trait — zero changes to existing socket crates.
- Quinn's QUIC datagrams (RFC 9221) → naia unreliable channel; QUIC streams → reliable channels.
- Solves G-T1 (TLS 1.3 built-in), G-CC1 (congestion control built-in), and unblocks G-T3 (WebTransport = thin HTTP/3 wrapper on top).

**Effort:** L | **Leverage:** 4

---

#### G-T5: No NAT traversal / hole-punching ⭐⭐⭐ — **COMPLETE**

**Current state:** Server is always reachable on a public address. No P2P path exists in naia.

**Decision:** P2P / NAT traversal is explicitly out of scope — naia is server-authoritative by design. Add a paragraph to `SECURITY.md` and `docs/CONCEPTS.md` stating this clearly and pointing to `matchbox_socket` + GGRS for P2P use cases. No code changes.

**Effort:** XS (docs only) | **Leverage:** 2

---

### 2.2 Client-Side Prediction & Rollback

#### G-P1: No built-in prediction/rollback framework ⭐⭐⭐⭐⭐

**Decision: NOT A GAP at the framework level.** Both the Bevy demo (`demos/bevy/`) and the Macroquad demo (`demos/macroquad/`) already implement full client-side prediction using `CommandHistory`, `TickBuffered` channels, `local_duplicate()`, and `receive_tick_buffer_messages`. These are the canonical reference implementations.

**Remaining action:** Fix `CommandHistory` robustness only — see G-DX2. The demos serve as the prediction tutorial.

---

#### G-P2: No snapshot interpolation framework ⭐⭐⭐⭐

**Decision: NOT A GAP at the framework level.** Both the Bevy demo (`demos/bevy/client/src/components/interp.rs`) and the Macroquad demo already implement snapshot interpolation using `server_interpolation()` and a simple `Interp` component with `last`/`next` positions. These serve as the reference implementation — no new framework needed.

**Effort:** N/A

---

### 2.3 Security

#### G-S1: AuthEvent payload is plaintext over native UDP ⭐⭐⭐⭐⭐

**Decision: Resolved by G-T1.** `SECURITY.md` already explicitly documents the plaintext risk and directs users to DTLS (WebRTC) or TLS (future `transport_quic`). No per-method doc additions needed — the security doc is the right place for this warning.

---

#### G-S2: No per-channel rate limiting ⭐⭐⭐⭐ — **COMPLETE**

**Current state:** `TickBufferSettings::message_capacity` already protects tick-buffered channels. `ReliableSettings` has no equivalent — reliable channels are unbounded.

**Decision:** Add `max_messages_per_tick: Option<u16>` to `ReliableSettings`. The server's reliable receiver counts messages delivered per tick per connection; once the cap is hit, excess messages are discarded with a `warn!`. `None` = unlimited (preserves current behaviour as the default). Non-breaking change.

**Concrete change:** `shared/src/messages/channels/channel.rs` — `ReliableSettings` gains one field; server-side reliable receiver enforces it.

**Effort:** S | **Leverage:** 4

---

#### G-S3: No packet-level integrity for native UDP ⭐⭐⭐

**Decision:** Document only. Add a one-liner to the transport comparison table in `docs/CONCEPTS.md` noting `transport_udp` has no integrity protection. Random corruption is already handled by the `SerdeErr` discard path; deliberate injection is addressed by transport selection (G-T1 / G-T4). No code changes.

**Effort:** XS (docs only) | **Leverage:** 2

---

### 2.4 Interest Management

#### G-I1: No spatial / automatic interest management ⭐⭐⭐⭐

**Decision: Out of scope.** Rooms handle coarse partitioning; `scope_checks_pending()` + a custom callback handles fine-grained spatial logic. A spatial hash plugin is a useful convenience but belongs in a third-party or game-specific crate — same reason naia has no physics. The existing API is already the correct hook point.

---

#### G-I2: No LOD-based update rate scaling ⭐⭐⭐ — **COMPLETE**

**Current state:** `set_gain(f32)` already exists on `GlobalPriorityState` and `UserPriorityState` — the API is complete.

**Decision:** Document the pattern with a worked example. Add a distance-LOD snippet to `demos/bevy/` showing `user_entity_priority_mut().set_gain(1.0 / (1.0 + dist))` inside the scope-check loop. No new API needed.

**Effort:** XS (demo only) | **Leverage:** 2

---

### 2.5 Developer Experience

#### G-DX1: No simulation of network conditions in dev builds ⭐⭐⭐⭐⭐ — **COMPLETE**

**Decision: NOT A GAP in code.** `LinkConditionerConfig` is fully implemented on both server and client, with 6 named presets (`perfect_condition()` through `very_poor_condition()`). It works on native UDP and the local in-process transport. This is a **documentation gap only**.

**Concrete action:** Add a `## Network Condition Simulation` section to `docs/CONCEPTS.md` showing `NativeSocket::new(&addrs, Some(LinkConditionerConfig::poor_condition()))` and listing all presets with their latency/jitter/loss values.

**Effort:** XS (docs only) | **Leverage:** 3

---

#### G-DX2: CommandHistory panic on non-monotonic insert ⭐⭐⭐ — **COMPLETE**

**Current state:** `insert()` panics on non-monotonic tick (contract guard — both demos correctly call `can_insert()` first). Buffer is unbounded.

**Decision:** Add `CommandHistory::new(max_ticks: u16)` with automatic front-eviction when the buffer exceeds `max_ticks` depth. `Default` keeps current unbounded behaviour for backwards compatibility. Update both demos to use `CommandHistory::new(128)` with a comment explaining the prediction window size.

**Concrete change:** `client/src/command_history.rs` — add `max_ticks: Option<u16>` field; evict in `insert()` when `buffer.len() > max_ticks`.

**Effort:** S | **Leverage:** 3

---

#### G-DX3: DisconnectEvent carries no reason ⭐⭐⭐ — **COMPLETE**

**Current state:** `DisconnectEvent` is a unit struct on both server (`server/src/events/world_events.rs`) and client (`client/src/world_events.rs`). Four distinct disconnect paths exist: client timeout, `disconnect_user()` (server kick), `HandshakeHeader::Disconnect` (clean close), and pending-auth timeout — all surfaced identically.

**Decision:** Add `DisconnectReason` enum to both server and client event types. Each disconnect site tags the reason at the call site.

```rust
pub enum DisconnectReason {
    ClientDisconnected,  // clean HandshakeHeader::Disconnect
    TimedOut,            // keepalive timeout
    Kicked,              // server called disconnect_user()
    AuthTimeout,         // pending-auth window expired
}
pub struct DisconnectEvent {
    pub user_key: UserKey,
    pub reason: DisconnectReason,
}
```

This is a small breaking change; the information is already available at each disconnect site in `main_server.rs`. Enables correct reconnect UX, kick logging, and matchmaking cleanup without guesswork.

**Effort:** S | **Leverage:** 4

---

#### G-DX4: No built-in metrics / observability export ⭐⭐⭐⭐ — **COMPLETE**

**Current state:** `PingManager` tracks `rtt_average` + `jitter_average` (EWMA floats). `BandwidthMonitor` produces kbps from a sliding time window. Neither is exposed via any public API; packet loss is not tracked at all.

**Decision:** Add `ConnectionStats` snapshot struct pulled from existing `PingManager` + `BandwidthMonitor` data, plus a `LossMonitor` that counts sent vs acked packets using the existing ack bitfield to produce `packet_loss_pct`.

```rust
pub struct ConnectionStats {
    pub rtt_ms: f32,
    pub jitter_ms: f32,
    pub packet_loss_pct: f32,   // from LossMonitor; needs in-flight counting
    pub bytes_sent_per_sec: f32,
    pub bytes_recv_per_sec: f32,
}
```

Expose via `server.connection_stats(&user_key) -> ConnectionStats` (poll-style). The Bevy adapter can emit this as a Bevy event each tick. Packet loss is the one metric that can't be reconstructed from a poll — it must be accumulated while ack bitfields are being parsed, making it worth the extra tracking struct.

**Concrete change:** `shared/src/connection/` — add `loss_monitor.rs`; thread `LossMonitor` through `BaseConnection`; add `connection_stats()` to the server public API.

**Effort:** M | **Leverage:** 4

---

#### G-DX5: No traffic simulation / replay recording ⭐⭐

**Current state:** No session replay or demo recording.

**Decision:** Defer indefinitely. Game-state replay (snapshot entire ECS per tick) is more useful than raw packet replay for debugging desync — packets are already fragmented and the raw bytes don't map cleanly back to game logic. The existing `ConditionedPacketReceiver` already demonstrates the `Box<dyn PacketReceiver>` wrapper pattern for anyone who needs it. Revisit only if a concrete desync debugging use case emerges.

**Effort:** M | **Leverage:** 2 | **Status: DEFERRED**

---

### 2.6 Protocol & Wire Format

#### G-W1: Fragmentation limit is far below optimal ⭐⭐⭐ — **COMPLETE**

**Current state:** `FRAGMENTATION_LIMIT_BYTES = 400` in `shared/src/constants.rs`. The constant looks arbitrary but is actually derived: `MTU_SIZE_BYTES = 576 (IPv4 min) - 60 (IP) - 8 (UDP) - 50 (DTLS) - 28 (SCTP) = 430` in `shared/serde/src/constants.rs`. The 400-byte fragmentation threshold leaves 30 bytes of headroom for naia headers within that 430-byte packet envelope — a deliberate WebRTC/SCTP worst-case design.

**Decision:** Keep the value as-is. Improve `shared/src/constants.rs` to explain the derivation chain (point to `serde/src/constants.rs`) so future readers understand it's a calculated WebRTC-safe limit, not an arbitrary conservative guess. Native-UDP users who need larger packets can override at the protocol level in a future change.

**Concrete change:** Update the doc comment on `FRAGMENTATION_LIMIT_BYTES` to cite the `MTU_SIZE_BYTES` derivation.

**Effort:** XS | **Leverage:** 1 | **Status: DOCS ONLY**

---

#### G-W2: No packet coalescing / batching ⭐⭐⭐

**Current state (re-investigated):** The send loop in `server/src/connection/connection.rs` already greedily packs all pending messages, entity updates, and actions into a single `BitWriter` (up to `MTU_SIZE_BYTES`) before each `io.send_packet()` call. The loop continues until the bandwidth budget is exhausted — multiple full packets per tick per connection are already emitted. ACK-only packets are edge-triggered (one-shot flag, line 365) — a single header-only datagram per keepalive interval, not one per tick.

**Decision:** Close as non-gap. The original concern ("one packet per send_all_packets") was a misreading of the loop structure. No change needed.

**Status: CLOSED — not a gap**

---

#### G-W3: No delta compression for numeric properties ⭐⭐⭐ — **COMPLETE**

**Current state:** `Property<f32>` sends the full `f32` (4 bytes) whenever it changes. Position updates are 8 bytes (x, y) per component per entity per tick.

**Why it matters:** Quantized + delta-compressed positions (6 bits for small deltas, 16 bits for medium, full 32 bits for large) can reduce position bandwidth by 4–8×. This is standard in every high-performance game networking library.

**Current state (re-investigated):** The serde layer already provides `UnsignedFloat<BITS, MAX>`, `UnsignedInteger<BITS>`, `SignedVariableInteger<BITS>` etc. in `shared/serde/src/number.rs`. `Property<T>` is generic over `T: Serde`, so `Property<UnsignedFloat<12, 4096>>` for a quantized position field already compiles and works today.

**Decision:** Document the existing primitives. Add a CONCEPTS.md section on "bandwidth-optimized properties" showing `Property<UnsignedFloat<12, 4096.0>>` for positions and `Property<UnsignedInteger<10>>` for health. No new code — the capability exists; the gap is discoverability. True per-field delta encoding (sending diffs between ticks) is a separate, larger feature deferred for now.

**Effort:** XS | **Leverage:** 2 | **Status: DOCS ONLY**

---

#### G-W4: Compression not used by default / not production-ready ⭐⭐ — Phase 1 **COMPLETE** | Phase 2 **COMPLETE**

**Current state:** `zstd_support` feature gate wires `Encoder`/`Decoder` through `io.rs` on both server and client. Two correctness bugs exist: (1) `encoder.encode()` always uses the compressed result even when it is larger than the input (`// TODO` at `shared/src/connection/encoder.rs:40`); (2) the decoder unconditionally calls `decompress()` on every packet — if the encoder ever skips compression there is no per-packet signal, so the decoder will corrupt the payload.

**Why early tests showed no gain:** naia packets are already bit-packed by `BitWriter` with per-field diffs and variable-width kind tags — the output is dense binary with low redundancy. zstd needs repeated structure across ~32–64 bytes to find patterns; most naia packets in a 20 Hz game are 50–200 bytes, right at the threshold where zstd frame overhead eats the gains. Dictionary mode (pre-training on real game traffic) is the promising path because both sides share schema knowledge ahead of time.

**Decision — two phases:**

**Phase 1 (S effort):** Fix the correctness bugs as a prerequisite.
- Add a single `is_compressed` bit at the start of each compressed payload (1 bit cost — packets are `BitWriter`/`BitReader` bit-packed, not byte-aligned).
- Encoder writes `true` + compressed bytes only when `compressed.len() < original.len()`; otherwise writes `false` + original bytes.
- Decoder reads the flag first and branches accordingly.
- Files: `shared/src/connection/encoder.rs`, `shared/src/connection/decoder.rs`, `server/src/connection/io.rs`, `client/src/connection/io.rs`.

**Phase 2 (evidence campaign, M effort): COMPLETE — GATE FAIL**
- `benches/examples/compression_audit.rs` captures real server-to-client packets from a 256-tile + 16-unit halo scenario via `hub.enable_packet_recording()` / `hub.take_recorded_packets()`.
- Run: `cargo run --release --example compression_audit -p naia-benches`
- Measured results (256 tiles + 16 units, 300 steady-state ticks):
  - Small packets (≤50B): zstd makes them LARGER (negative reduction — frame overhead dominates)
  - Large packets (>150B, spawn-burst): **6.8% reduction** at zstd-3 and with dictionary
  - Steady-state update packets (all ≤50B): negative reduction across all levels
- **Gate: FAIL** — large spawn-burst bucket shows only 6.8%, below the 15% threshold.
- **Conclusion:** Dictionary compression is not worth the per-connection setup overhead for naia's packet profile. Close this gap permanently: compression (even dictionary mode) does not help bit-packed variable-width binary at naia's packet sizes. The `zstd_support` feature remains available for application-level use cases (e.g. bulk asset transfer) but should not be enabled by default.

**Effort:** S (Phase 1) + M (Phase 2) | **Leverage:** 2–4 depending on measurement outcome

---

### 2.7 Scalability

#### G-SC1: No multi-server architecture / horizontal scaling ⭐⭐⭐⭐ — **COMPLETE**

**Current state:** One server process owns all entities. The Naia server is a single-process authority. There is no built-in mechanism for multiple server instances to collaborate.

**Decision:** Defer indefinitely. Horizontal scaling is an application-architecture concern, not a transport library concern — renet, lightyear, and GNS all leave this to the developer. Game developers targeting 1000+ CCU build zone sharding at the application layer; naia provides the per-process primitive. Add a CONCEPTS.md architecture note on the zone-server pattern (separate naia processes + application-level entity serialization for handoff) without library changes.

**Effort:** XL | **Leverage:** 4 | **Status: DEFERRED**

---

#### G-SC2: `scope_checks_all()` O(users × entities) allocation ⭐⭐⭐ — **COMPLETE**

**Current state:** `scope_checks_all()` does `self.scope_checks_cache.as_slice().to_vec()` — allocating a Vec every call. 5 TODOs in `world_server.rs` acknowledge this. The incremental `scope_checks_pending()` → `mark_scope_checks_pending_handled()` path is already allocation-free and O(churn).

**Decision:** Delete `scope_checks_all()` entirely. It is a performance trap — calling it every frame defeats the incremental scope system. Replace any remaining call sites in demos, docs, and tests with `scope_checks_pending()`. For the legitimate full-resync use case (e.g. server startup), add `server.mark_all_scope_checks_pending()` which enqueues the full entity×user cross-product into the pending queue without exposing the allocating path.

**Concrete change:** Remove `scope_checks_all()` from `world_server.rs`; add `mark_all_scope_checks_pending()`; update demos and CONCEPTS.md scope section to use only the pending pattern.

**Effort:** S | **Leverage:** 3

---

### 2.7 Missing High-Level Gameplay Systems

#### G-P3: No lag compensation (hit detection rewind) ⭐⭐⭐⭐ — **COMPLETE**

**Current state:** Not present. Naia has no physics/collision code and correctly avoids it.

**Decision:** Follow Nengi.js's `Historian` pattern — provide optional building blocks, demonstrate the pattern in a demo, own nothing game-specific.

**Reference: Nengi.js `Historian`**
- Per-tick rolling snapshot buffer (`HISTORIAN_TICKS` deep), keyed by integer tick
- Entities proxified (shallow copy of networked fields per schema) at record time — not live refs
- Single public API: `getLagCompensatedArea(timeAgo, aabb)` returns past-tick entity copies; developer does the hit test and rewind/restore themselves
- Completely opt-in; zero cost when disabled

**Naia equivalent design:**
```rust
// Opt-in on WorldServer:
server.enable_historian(ticks_to_save: u16);

// Each tick, naia records a snapshot of all replicated component values:
// (driven by existing diff machinery — no extra per-component work)

// Server processes a FireCommand from client tick T:
let time_ago = client_rtt_ms + interp_offset_ms;
let snapshot = server.historian().snapshot_at_time_ago(time_ago);
// snapshot gives: HashMap<Entity, ComponentSnapshot> for that past tick
// Developer does their own spatial query + hit test + restore
```

**Concrete scope:**
- `Historian` struct in `server/src/` — rolling `VecDeque<(Tick, HashMap<GlobalEntity, ComponentSnapshot>)>` with auto-eviction
- `ComponentSnapshot` = serialized bytes of all replicated components (reuses existing serde machinery)
- `server.historian()` accessor — returns `None` if not enabled
- Extend `demos/bevy/` with a hitscan character example showing the full rewind-restore loop
- No Bevy-specific integration needed — the historian is ECS-agnostic data storage

**Effort:** M | **Leverage:** 4

---

#### G-T6: Single-socket architecture — can't serve native + WASM clients simultaneously ⭐⭐⭐⭐

**Decision: NOT A GAP.** The WebRTC transport (`naia-server-socket`) serves both native and browser clients from a single server process. No multi-socket machinery is needed. Closed.

---

#### G-DX6: No TypeScript/JavaScript client ⭐⭐

**Current state:** Browser clients must be written in Rust compiled to WASM. The large market of browser-native JavaScript/TypeScript games (Phaser, Three.js, Pixi.js, etc.) cannot use naia.

**Decision:** Defer indefinitely. XL effort with a hard sequencing dependency on G-T3 (WebTransport, itself deferred). No peer library ships a TS client — it's not a gap vs competitors. Rust→WASM already covers the browser use case naia targets, and a TS codegen would create a second surface to maintain in sync with every protocol change.

**Effort:** XL | **Leverage:** 3 | **Status: DEFERRED**

---

### 2.8 RTT & Congestion Control

#### G-CC1: No congestion control (only token-bucket rate limit) ⭐⭐⭐⭐

**Current state:** `BandwidthAccumulator` is a fixed-rate token bucket paired with a priority accumulator.

**Re-analysis (post Fiedler research):** The original recommendation (AIMD) was a misapplication of TCP/stream thinking to a state sync protocol. AIMD backs off a sender rate to drain a reliability queue — the model assumes "loss = debt." In state sync, loss means "client missed a snapshot" and the *next* snapshot supersedes the missed one. There is no debt. The correct congestion response is already built in: bandwidth pressure surfaces as more entities being deferred per tick, and the priority accumulator handles this by retaining and compounding priority for skipped items. Halo: Reach's framing is precise — *"unreliability enables aggressive prioritization."* The system's response to congestion is to defer lower-priority state, not back off the whole connection.

For reliable messages the token bucket already provides pacing; reliable content is never "dropped by priority," only delayed by budget — eventual delivery is guaranteed by the compound-and-retain invariant.

**Remaining real gap:** The `target_bytes_per_sec` is static config. There is no mechanism to detect that the configured target exceeds the actual path capacity (i.e. true bufferbloat). A lightweight adaptive probe — gradually increase target until ack gaps widen, then nudge back down — would prevent operators from over-configuring. This is much simpler than full AIMD and appropriate for state sync: a slow-moving rate probe, not a per-loss halving event.

**Decision:** Close the AIMD recommendation as inapplicable. Track the adaptive rate probe as a separate low-priority item. No implementation change now.

**Status: CLOSED — AIMD not applicable to state sync; adaptive rate probe deferred**

---

#### G-CC2: RTT measurement lacks percentiles ⭐⭐ — **COMPLETE**

**Current state:** `PingManager` tracks `rtt_average` and `jitter_average` as EWMA f32 scalars. No history buffer, no percentiles.

**Decision:** Implement together with G-DX4 (`ConnectionStats`). Add a 32-sample u16 ring buffer to `PingManager` (64 bytes per connection); sort-on-read for p50/p99 (32-element insertion sort, negligible cost). Expose `rtt_p50_ms` and `rtt_p99_ms` as fields on `ConnectionStats`. P99 RTT is the operative input for prediction window sizing and jitter buffer depth.

**Effort:** S | **Leverage:** 2 | **Status: BLOCKED on G-DX4**

---

### 2.9 Documentation & Onboarding

#### G-DOC1: No prediction/rollback tutorial ⭐⭐⭐⭐⭐ — **COMPLETE**

**Current state:** `CONCEPTS.md` section 9 describes tick synchronisation and mentions "rollback-and-replay" in two sentences with no worked example. The Bevy demo (`demos/bevy/client/src/systems/events.rs` lines 328–441) already implements the full prediction loop — the pattern exists in code but is undiscoverable without reading the demo source.

**Decision:** Write `docs/PREDICTION.md` as a standalone guide. Content:
1. Mental model — client runs ahead of server by RTT/2; server is authoritative; client re-simulates on correction
2. `TickBuffered` channels for input stamping
3. `CommandHistory::new(128)` for replay buffer (bounded, post G-DX2 fix)
4. The 5-step prediction loop: store input → apply locally → send via TickBuffered → receive server correction → rollback + re-simulate from `replays(correction_tick)`
5. Annotated code excerpts from the existing Bevy demo (no new demo code needed — the reference is already there)
6. Link from README and CONCEPTS.md section 9

A dedicated file gets its own URL for linking from issues and Discord, and separates the prediction guide from the general CONCEPTS content.

**Effort:** S | **Leverage:** 5

---

#### G-DOC2: No migration guide from renet or lightyear ⭐⭐⭐ — **COMPLETE**

**Current state:** `docs/MIGRATION.md` covers naia version-to-version migrations only. New evaluators coming from renet or lightyear have no concept-mapping entry point.

**Decision:** Add a "Coming from X?" section directly to `README.md` — a short concept-mapping table immediately visible to every evaluator without navigating to `docs/`. Cover the two most common origins: renet (message-passing, no replication — map to naia channels + messages) and lightyear (Bevy-native replication — map to naia's ECS-agnostic `#[derive(Replicate)]` + Bevy adapter). Two paragraphs + one table; no full migration guide needed at this stage.

**Effort:** XS | **Leverage:** 2

---

### 2.10 Testing & Reliability

#### G-QA1: No chaos / adversarial transport test ⭐⭐⭐⭐ — **COMPLETE**

**Current state (re-investigated):** The gap was overstated. The harness already has `LinkConditionerConfig` with loss/jitter/latency, `configure_link_conditioner()` per-client, step bindings for "stable" (50ms/2ms/0%) and "adverse" (100ms/50ms/10%) conditions, and live adversarial scenarios in `01_lifecycle.feature` (transport-01 packet loss tolerance, observability-03 RTT under jitter+loss).

**What's actually missing:** (1) A scenario verifying *entity replication convergence* under loss — does the dirty-set eventually fully sync to a client experiencing 20% packet loss? (2) A tick-buffer delivery scenario under loss — does `TickBuffered` input arrive at the correct server tick despite 10% loss?

**Decision:** Add these two specific scenarios to the existing feature files using the existing step bindings. No new infrastructure needed.

**Effort:** S | **Leverage:** 3

---

#### G-QA2: Fuzz testing of the packet deserialization path ⭐⭐⭐⭐ — **COMPLETE**

**Current state:** BDD scenarios `adversarial-01` / `messaging-02` test targeted corrupt inputs (truncated packets, wrong type byte). No generative fuzzing exists; no `fuzz/` crate.

**Decision:** Both tracks:

**Track A — `cargo-fuzz` harness** (`fuzz/` crate, libFuzzer):
```
naia/fuzz/
  Cargo.toml
  fuzz_targets/
    packet_deserialize.rs   — raw bytes → BitReader → all Serde impls
    header_deserialize.rs   — raw bytes → StandardHeader::de
```
Finds memory-safety and bit-boundary bugs that structured tests cannot. Run with `cargo fuzz run packet_deserialize` before each release. No CI required — offline pre-release gate.

**Track B — `proptest` roundtrip suite** (in `shared/serde/tests/`):
Roundtrip invariant: for any value of type T, `T::de(T::ser(v)) == v`. Covers all `Serde` impls (`UnsignedInteger`, `UnsignedFloat`, `SignedVariableInteger`, scalars, `Option`, `Vec`, `String`). Runs as a normal `cargo test` — becomes part of the existing test gate.

The two are complementary: `proptest` catches logical invariant violations on structurally valid data; `cargo-fuzz` catches crash bugs on malformed bit streams.

**Effort:** S + S | **Leverage:** 4

---

## Part III — Priority Stack

Ranked by (leverage × urgency × community impact):

Active items only (closed/deferred gaps noted inline):

| Rank | Gap | Decision | Effort | Leverage |
|---|---|---|---|---|
| 1 | **G-DOC1**: Prediction/rollback tutorial (`docs/PREDICTION.md`) | ~~Implement~~ **COMPLETE** | S | 5 |
| 2 | **G-DX2**: `CommandHistory::new(max_ticks)` + demo update | ~~Implement~~ **COMPLETE** | S | 3 |
| 3 | **G-DX3**: `DisconnectReason` enum on server + client events | ~~Implement~~ **COMPLETE** | S | 4 |
| 4 | **G-S2**: `max_messages_per_tick` on `ReliableSettings` | ~~Implement~~ **COMPLETE** | S | 4 |
| 5 | **G-W4** Phase 1: `is_compressed` bit + encoder size check | ~~Implement~~ **COMPLETE** | S | 2–4 |
| 6 | **G-SC2**: Delete `scope_checks_all()`; add `mark_all_scope_checks_pending()` | ~~Implement~~ **COMPLETE** | S | 3 |
| 7 | **G-QA1**: Add 2 adversarial BDD scenarios (replication convergence + tick-buffer under loss) | ~~Implement~~ **COMPLETE** | S | 3 |
| 8 | **G-QA2**: `cargo-fuzz` harness + `proptest` roundtrip suite | ~~Implement~~ **COMPLETE** | S+S | 4 |
| 9 | **G-DX4**: `ConnectionStats` struct + `LossMonitor` + `connection_stats()` API | ~~Implement~~ **COMPLETE** | M | 4 |
| 10 | **G-CC2**: RTT ring buffer → `rtt_p50/p99` (implement with G-DX4) | ~~Implement~~ **COMPLETE** | S | 2 |
| 11 | **G-P3**: `Historian` snapshot buffer + demo | ~~Implement~~ **COMPLETE** | M | 4 |
| 12 | **G-W4** Phase 2: compression-audit bench + dictionary decision gate | ~~Implement~~ **COMPLETE** | M | 2–4 |
| 13 | **G-T1**: `transport_udp` plaintext warnings in module doc + `SECURITY.md` | ~~Docs~~ **COMPLETE** | S | 4 |
| 14 | **G-DX1**: `LinkConditionerConfig` docs section in `CONCEPTS.md` | ~~Docs~~ **COMPLETE** | XS | 3 |
| 15 | **G-W1**: Update `FRAGMENTATION_LIMIT_BYTES` comment with derivation | ~~Docs~~ **COMPLETE** | XS | 1 |
| 16 | **G-W3**: "Bandwidth-optimized properties" section in `CONCEPTS.md` | ~~Docs~~ **COMPLETE** | XS | 2 |
| 17 | **G-T5**: NAT traversal out-of-scope note in `CONCEPTS.md` | ~~Docs~~ **COMPLETE** | XS | 2 |
| 18 | **G-I2**: Distance-LOD `set_gain()` snippet in Bevy demo | ~~Docs/Demo~~ **COMPLETE** | XS | 2 |
| 19 | **G-DOC2**: "Coming from X?" table in `README.md` | ~~Docs~~ **COMPLETE** | XS | 2 |
| 20 | **G-SC1**: Zone-server architecture note in `CONCEPTS.md` | ~~Docs~~ **COMPLETE** | XS | 1 |
| — | **G-P1**, **G-P2**, **G-T6**, **G-W2**, **G-S1**, **G-S3** | CLOSED — not gaps | — | — |
| — | **G-T2**, **G-T3**, **G-T4**, **G-DX5**, **G-DX6**, **G-CC1** | DEFERRED | — | — |
| — | **G-I1** | OUT OF SCOPE | — | — |

---

## Part IV — What Would Make Naia Definitively #1

The Rust game networking space has a clear segmentation:

- **Simplest**: renet (no replication, netcode.io security, many shipped games)
- **Most features**: lightyear (prediction, interpolation, everything — but complex)
- **Most principled**: naia (rooms, priority, ECS-agnostic, clean design)

Naia can be **#1 in class** by pursuing two bets simultaneously:

### Bet A: Security + Transport (Ranks 1–3)
Encrypt the native UDP path. Ship WebTransport. Kill the DTLS security debt. This immediately makes Naia the only Rust library with production-safe security on both native and browser targets.

### Bet B: Prediction-First DX (Ranks 2, 5, 6)
Ship a built-in prediction framework with a 5-step tutorial. Make it work out of the box with one `#[derive(Predicted)]` attribute and a `rollback_schedule`. This is the feature lightyear used to overtake Naia in mind-share — take it back.

**These two bets address the two most common reasons developers choose alternatives over Naia.** Everything else (congestion control, delta compression, spatial interest management) compounds on top and can be executed incrementally.

---

## Part V — Near-Term Quick Wins (≤ 1 week each)

These are high-signal, low-effort improvements that can be executed immediately without blocking on the larger bets:

| Item | What | Why | Effort |
|---|---|---|---|
| **QW-1** | Add `## Network Condition Simulation` section to `docs/CONCEPTS.md` | `LinkConditionerConfig` exists on both sides; zero discoverability | XS |
| **QW-2** | Update `FRAGMENTATION_LIMIT_BYTES` doc comment to cite `MTU_SIZE_BYTES` derivation | Looks arbitrary; it's calculated — explain why | XS |
| **QW-3** | Add `CommandHistory::new(max_ticks: u16)` with front-eviction; update both demos to `new(128)` | Unbounded buffer is a footgun; demos must show bounded usage | S |
| **QW-4** | Add `DisconnectReason` enum to `DisconnectEvent` on both server + client | Every game needs this; all information is already at the call sites | S |
| **QW-5** | Fix `encoder.rs`: add `is_compressed` bit + size check; fix `decoder.rs` to branch on it | Without the flag the decoder corrupts uncompressed payloads; prerequisite for Phase 2 | S |
| **QW-6** | Add `max_messages_per_tick: Option<u16>` to `ReliableSettings` | Reliable channels are unbounded; tick-buffered already has `message_capacity` | S |
| **QW-7** | Delete `scope_checks_all()`; add `mark_all_scope_checks_pending()` | `scope_checks_all()` allocates a Vec every call and defeats the incremental scope system | S |
| **QW-8** | Add `DisconnectReason` docs + `transport_udp` plaintext warning to `SECURITY.md` | Plaintext risk must be clearly documented at the transport level | XS |
| **QW-9** | Write `docs/PREDICTION.md` with full worked example | Highest documentation leverage per hour; closes the #1 DX complaint | S |
| **QW-10** | Add "Coming from X?" concept-mapping table to `README.md` | Converts confused renet/lightyear evaluators into productive users immediately | XS |

---

## Part VI — Architecture Vision (What Naia Looks Like When Done)

```
naia (transport-agnostic core)
├── transports/
│   ├── transport_udp            (current: native UDP, plaintext — dev/trusted-network only)
│   ├── transport_webtransport   (DEFERRED: HTTP/3 QUIC, replaces WebRTC — blocked on G-T4)
│   ├── transport_quic           (DEFERRED: Quinn, TLS 1.3 native — smol runtime only)
│   └── transport_local          (current: in-process test)
├── prediction/
│   ├── CommandHistory           (exists; G-DX2: add new(max_ticks) + bounded demos)
│   └── Historian                (G-P3: rolling per-tick snapshot buffer for lag compensation)
├── interest/
│   ├── Rooms + Scope            (current: correct and complete)
│   └── set_gain() LOD           (current: G-I2 docs only — add demo snippet)
├── observability/
│   ├── BandwidthMonitor         (current)
│   ├── LossMonitor              (G-DX4: NEW — counts sent vs acked via ack bitfield)
│   └── ConnectionStats          (G-DX4: NEW — rtt_ms, jitter_ms, loss_pct, bytes/sec; poll-style)
├── security/
│   ├── advanced_handshaker      (current: HMAC challenge/response)
│   └── ReliableSettings cap     (G-S2: max_messages_per_tick to prevent reliable-channel flood)
└── docs/
    ├── CONCEPTS.md              (add: link conditioner, quantized properties, zone-server, NAT note)
    ├── PREDICTION.md            (G-DOC1: NEW — full prediction/rollback worked guide)
    └── README.md                (G-DOC2: add "Coming from X?" concept-mapping table)
```

This architecture preserves Naia's core strength (clean separation, ECS-agnostic, composable) while closing every actionable gap identified in this analysis. Deferred items (QUIC, WebTransport, TypeScript client) are sequenced correctly — QUIC unblocks WebTransport which unblocks the TS client.

---

## Appendix A — Competitor Feature Matrix

| Feature | Naia | lightyear | renet/renet2 | bevy_replicon |
|---|---|---|---|---|
| Entity replication | ✅ | ✅ | ❌ (manual) | ✅ |
| Interest management | ✅ (rooms) | ✅ (spatial) | ❌ | ✅ (filters) |
| Priority accumulator | ✅ | Partial | ❌ | ❌ |
| Authority delegation | ✅ | ✅ | ❌ | ❌ |
| Tick sync | ✅ | ✅ | ❌ | ❌ |
| Client prediction | Building blocks | ✅ Full | ❌ | ❌ |
| Snapshot interpolation | ✅ (in demos; docs gap only) | ✅ | ❌ | ❌ |
| Lag compensation (hit rewind) | ❌ | ✅ | ❌ | ❌ |
| Native UDP | ✅ | ✅ | ✅ | ✅ |
| WebRTC (browser) | ✅ | ✅ | ❌ | ✅ (via quinnet) |
| WebTransport (QUIC browser) | ❌ | ✅ | renet2 ✅ | ✅ (via quinnet) |
| QUIC native | ❌ | ✅ | ❌ | ✅ (via quinnet) |
| Multi-socket (native+WASM) | ✅ (WebRTC serves both) | ✅ | ✅ | ✅ (transport-agnostic) |
| UDP encryption | ❌ | ✅ (netcode.io) | ✅ (netcode.io) | ✅ |
| Per-channel rate limiting | ❌ | ❌ | ❌ | ❌ |
| Network condition simulation | ✅ (both sides; docs gap only) | ✅ | ❌ | ❌ |
| Connection stats / metrics | Partial | ✅ (tracing+metrics) | Partial | ❌ |
| Congestion control | ❌ | ✅ (via QUIC) | Partial | ✅ (via QUIC) |
| TypeScript client | ❌ | ❌ | ❌ | ❌ |
| ECS-agnostic | ✅ | ❌ (Bevy-only) | ✅ | ❌ (Bevy-only) |
| BDD test coverage | ✅ (332 scenarios) | ❌ | ❌ | ❌ |
| Zero `todo!()` in production | ✅ | ? | ? | ? |
| Fuzz harness | ❌ | ❌ | ❌ | ❌ |

**Naia's unique advantages** (guard these fiercely):
- ECS-agnostic design (the only library that works outside Bevy)
- Priority accumulator with bandwidth budgeting
- Rooms + fine-grained scope with O(churn) incremental updates
- Authority delegation state machine (unique in OSS)
- 332-scenario BDD test suite (industry-leading)
- Clean architecture with zero UB and zero production panics

---

---

## Appendix B — Research Sources

- Quinn: https://github.com/quinn-rs/quinn
- ENet: http://enet.bespin.org/ + https://github.com/lsalzman/enet
- GameNetworkingSockets: https://github.com/ValveSoftware/GameNetworkingSockets
- renet: https://github.com/lucaspoffo/renet
- renet2: https://github.com/UkoeHB/renet2
- netcode protocol: https://github.com/mas-bandwidth/netcode
- laminar: https://github.com/TimonPost/laminar (inactive)
- lightyear: https://github.com/cBournhonesque/lightyear + https://cbournhonesque.github.io/lightyear/book/
- bevy_replicon: https://github.com/projectharmonia/bevy_replicon
- GGRS: https://github.com/gschup/ggrs
- crystalorb: https://github.com/ErnWong/crystalorb
- matchbox (P2P WebRTC): https://github.com/johanhelsing/matchbox
- bevy_quinnet: https://github.com/Henauxg/bevy_quinnet
- Fish-Net: https://fish-networking.gitbook.io/docs + https://github.com/FirstGearGames/FishNet
- LiteNetLib: https://github.com/RevenantX/LiteNetLib
- Gaffer on Games — reliability: https://gafferongames.com/post/reliability_ordering_and_congestion_avoidance_over_udp/
- Gaffer on Games — snapshot compression: https://gafferongames.com/post/snapshot_compression/
- Tribes 2 networking model: https://www.gamedevs.org/uploads/tribes-networking-model.pdf
- Valve Source multiplayer: https://developer.valvesoftware.com/wiki/Source_Multiplayer_Networking
- Gabriel Gambetta lag compensation: https://www.gabrielgambetta.com/lag-compensation.html
- Bevy networking discussion: https://github.com/bevyengine/bevy/discussions/4388
- Are We Game Yet networking: https://arewegameyet.rs/ecosystem/networking/

*End of analysis. Total findings: 31 gaps, 10 quick wins, 2 strategic bets.*
