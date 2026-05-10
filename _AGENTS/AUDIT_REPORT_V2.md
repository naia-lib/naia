# Naia — Audit Report V2

**Date:** 2026-05-10  
**Auditor:** claude-sonnet-4-6  
**Gate at audit time:** `cargo test --test '*'` — all test suites pass (0 failures); `cargo deny check advisories` — ok (all 24 ignores time-boxed to 2027-06-01). No `namako` package found in the workspace; test infrastructure lives in `naia-test-harness` (doc-tests pass, 0 failures).

---

## Track A — API Design & Ergonomics

### A.1 — Naming consistency

**OK.**  
`Server` and `Client` mirror each other well: `send_message` / `receive_message`, `send_all_packets` / `receive_all_packets`, `scope_checks_all` / `scope_checks_pending`. The `entity_enable_delegation` / `entity_request_authority` / `entity_release_authority` names are consistent between the two sides. The Bevy adapter reuses the same method names for every adapter method. One minor asymmetry: the server exposes both `scope_checks_all()` and `scope_checks_pending()` with explicit distinction; neither is paired with a client-side analogue (clients don't iterate scopes), which is correct but worth documenting.

### A.2 — Fallibility

**MINOR.**  
`Server::send_message` returns `Result<(), NaiaServerError>` — good. `Server::broadcast_message` returns `()` and silently discards per-user errors (the loop calls `let _ = self.send_message_inner(…)`). For a library consumer, a broadcast that partially fails (e.g., a stale user key) is unobservable. This is acceptable for broadcasts (partial delivery is expected during disconnect churn) but is not documented.

`Server::user()` and `Server::user_mut()` (forwarded through `MainServer`) panic on missing key rather than returning `Option`. The API offers `user_exists()` as a pre-check, but callers who skip it will see a panic rather than a Result.

### A.3 — Footguns

**NOTABLE.**  
Several public API methods panic on misuse rather than returning errors:

- `Server::user()` — `panic!("No User exists for given Key!")` — reachable if caller holds a stale UserKey from a disconnected client.
- `Server::configure_entity_replication()` — panics if entity not yet replicating, or if `server_owned` is `Private`.
- `Server::spawn_entity_inner` / `despawn_entity` — indirect panics via `expect("Cannot despawn non-existant entity!")`.
- `Client/Server IO` — `expect("Cannot call Client.send_packet() until you call Client.connect()!")` — reachable if send is called before connect in certain async setups.

All of these represent misuse violations documented nowhere in the API signatures. Since `UserKey` is a freely-copyable value that outlives the connection it was issued for, using a stale key after `DisconnectEvent` is a plausible mistake.

**Proposed fix:** Convert `Server::user()` and `Server::user_mut()` to return `Option<UserRef>` / `Option<UserMut>`, or add `#[must_use]` guard docs. At minimum, document `# Panics` on every method that can panic with user-provided keys.

### A.4 — Builder / config ergonomics

**OK.**  
`ServerConfig::default()` and `ClientConfig::default()` are sensible (30s disconnect timeout, 4s heartbeat, 50ms tick). All fields are public and directly settable. Misconfiguration (e.g., heartbeat > timeout) is not validated at construction time, but the consequences are observable (clients disconnect quickly), not silent data corruption. `Protocol::builder()` returns a default `Protocol` and is pleasant to use.

### A.5 — Generic bounds

**MINOR.**  
The `E: Copy + Eq + Hash + Send + Sync` bound is present on every struct and impl that carries entity keys. No doc comment in any file explains why each bound is required. A one-line `// E: Copy (entity keys are value types, no heap), Eq+Hash (used as map keys), Send+Sync (shared across tasks)` on the primary `Server<E>` struct would prevent confusion for first-time users.

---

## Track B — Error Handling

### B.1 — Error type coverage

**OK.**  
`NaiaServerError`: 5 variants (Message, Wrapped, SendError, RecvError, UserNotFound). `NaiaClientError`: 5 variants (Message, Wrapped, SendError, RecvError, IdError). Both implement `Display` usefully. `UserNotFound` is specific enough to act on. `IdError(u16)` exposes the bad ID value for debugging.

### B.2 — Panic vs Result

**NOTABLE.**  
The `grep` for `unwrap()` in production code returned 0 results — all former `unwrap()` calls have been replaced with `let … else` guards or `expect()` with contextual messages. However, bare `panic!("")` (empty message) appears 5 times in `server/src/transport/udp.rs` in the URL parsing helper `url_str_to_addr`/`url_to_addr`. These are called at server startup from user-provided config strings; the actual error is logged before the panic but the panic message itself is empty, making crash reports difficult to diagnose.

Also notable: `host_engine.rs` panics with `"Cannot accept message for an entity that does not exist in the engine"` on receipt of a protocol message for an unknown entity. This should be an internal invariant, but a crafted or reordered packet sequence from a buggy client could trigger it.

**Proposed fix for udp.rs panics:** Replace `panic!("")` with `panic!("{}", SOCKET_PARSE_FAIL_STR)` or similar non-empty message at each site.

**Proposed fix for host_engine panic:** Replace with a `warn!` + `return` to avoid crashing the server on a malformed remote message (see Track K.1 relationship).

### B.3 — Event-based error surfacing

**OK.**  
Errors during packet decoding surface as `warn!` log messages plus a `continue` (the packet is dropped). The disconnect event fires when the timeout expires. No error is silently absorbed without either a log or an event. The handshake `TODO: bubble this up` comments (advanced_handshaker.rs:143, simple_handshaker.rs:129) are present (see Track J.1) but the fallback is a `warn!` + `return None`, not a silent loss.

### B.4 — Error granularity in transport

**MINOR.**  
`NaiaServerError::RecvError` and `NaiaClientError::RecvError` carry no payload — a consumer cannot distinguish "socket closed" from "buffer full" from "OS error". `SendError(SocketAddr)` is better — it at least identifies which peer failed. For production telemetry this is a minor gap; recovery action in both cases is "wait for disconnect event."

---

## Track C — Safety & Soundness

### C.1 — Unsafe inventory

**NOTABLE.**  
All unsafe sites found:

| File | Pattern | Safety comment? |
|---|---|---|
| `server/src/transport/local/data.rs:49` | `transmute` to `&'static [u8]` | None |
| `client/src/transport/local/data.rs:72` | `transmute` to `&'static [u8]` | None |
| `server/src/error.rs:52-53` | `unsafe impl Send/Sync` | None |
| `client/src/error.rs:43-44` | `unsafe impl Send/Sync` | None |
| `shared/src/world/world_reader.rs:86,233` | `extern "Rust"` fn calls | Comment present (e2e_debug only) |
| `shared/src/world/sync/remote_entity_channel.rs:230` | Same | Same |
| `shared/src/world/local/local_world_manager.rs:706` | Same | Same |
| `adapters/bevy/shared/src/component_access.rs:149` | `as_unsafe_world_cell()` | None |
| `adapters/bevy/shared/src/world_proxy.rs:490` | `as_unsafe_world_cell()` | None |
| `socket/server/src/session.rs:540` | `Pin::new_unchecked` | Has a comment ("TODO: This could be catastrophic") |
| `socket/client/src/backends/miniquad/*` | FFI globals via static mut | None |
| `socket/client/src/backends/wasm_bindgen/packet_sender.rs:58-59` | `unsafe impl Send/Sync` | None |
| `shared/src/backends/miniquad/timer.rs` | FFI calls | None |

### C.2 — Transmute and lifetime extension

**CRITICAL.**  
Both `server/src/transport/local/data.rs` and `client/src/transport/local/data.rs` use `std::mem::transmute` to extend a borrowed slice into `&'static [u8]`. The pattern:

```rust
let payload_ref = self.last_payload.lock();
let (addr, payload_slice) = payload_ref.as_ref().unwrap();
let static_ref: &'static [u8] = unsafe { std::mem::transmute(payload_slice.as_ref()) };
Ok(Some((*addr, static_ref)))
```

The `payload_ref` is a `MutexGuard` that is dropped at end of scope. The `static_ref` borrows data inside `self.last_payload` (an `Arc<Mutex<…>>`). However, the `MutexGuard` is dropped *before* `static_ref` is used by the caller, meaning the lock is released and a new call to `receive()` could overwrite `last_payload` while the caller holds the "static" reference. This is undefined behavior: the caller can observe the reference pointing to mutated (or freed if the `Box` is reallocated) data.

**Proposed fix:** Change `receive()` to return `Result<Option<Box<[u8]>>>` (owned), or return the `MutexGuard` via a guard wrapper that ties the lifetime to the caller's scope. The transmute serves no purpose if ownership is transferred. The local transport is test-only infrastructure, but the unsoundness is real.

### C.3 — Manual Send/Sync impls

**MINOR.**  
`NaiaServerError` and `NaiaClientError` both contain `Box<dyn Error>` (without `+ Send + Sync` on the inner trait). This is the reason for the manual impl — the inner error type is not automatically `Send + Sync`. The impls are correct as written (if the contained boxed error is used only in a single-threaded context), but should carry a `// Safety:` comment explaining the constraint on callers who store non-Send errors inside the `Wrapped` variant.

The `wasm_bindgen` `PacketSenderImpl` `unsafe impl Send/Sync` is expected on wasm32 (single-threaded, no real threads), but has no comment.

### C.4 — FFI and extern blocks

**MINOR.**  
The `extern "Rust"` blocks in `world_reader.rs`, `remote_entity_channel.rs`, and `local_world_manager.rs` call `client_saw_spawn_increment()` etc., which are only active under `#[cfg(feature = "e2e_debug")]`. These are feature-gated correctly and the symbols must be provided by the test harness. The risk is: if the test harness ever fails to define these symbols, the linker error will occur at test time, not library time — acceptable. No doc/comment explains this linking contract.

The miniquad backend `extern "C"` blocks access mutable statics without synchronization (safe on single-threaded wasm32, but undocumented).

---

## Track D — Documentation

### D.1 — Public API coverage

**MINOR.**  
`Server<E>` struct doc is thorough, including a minimal server loop example. `Client<E>` lib.rs has a crate-level doc with a connection setup example and loop. Key methods on `Server`: `send_message`, `broadcast_message`, `spawn_entity`, `scope_checks_all`, `accept_connection`, `reject_connection` — all have doc comments. Gaps: `pause_entity_replication()`, `resume_entity_replication()`, `enable_entity_replication()` (Bevy-only), `disable_entity_replication()` — these have no doc comment at all.

### D.2 — Accuracy

**OK.**  
Spot-checked 10 methods against implementation. All docs match the code. `scope_checks_all()` and `scope_checks_pending()` docs correctly describe the O(churn) vs O(N) distinction.

### D.3 — Crate-level docs

**OK.**  
`naia-shared` has a 3-line `//!` block. `naia-client` has a complete crate-level doc including connection setup and main loop. `naia-server`'s `Server<E>` struct doc includes a minimal loop. These are accurate for today's code.

### D.4 — Safety comments on unsafe

**NOTABLE.**  
No `// Safety:` comment exists on any of the `unsafe` blocks in production code (see Track C.1 table). This is a finding for every unsafe site listed.

**Proposed fix:** Add `// Safety:` comments to all unsafe blocks describing what invariant must hold. Priority: the two `transmute` sites (C.2) and the `Send/Sync` impls.

### D.5 — Example correctness

Demos not audited (excluded by scope). The docs use `text`-fenced code blocks for the minimal loops, so they do not run as doctests. This avoids false-positive doc test failures but also means example correctness is unverified by CI.

---

## Track E — Test Coverage

### E.1 — BDD spec completeness

**OK.**  
7 feature files covering: foundations, lifecycle, messaging, replication, visibility, authority, resources. Authority is the most complete — 38 concrete scenarios plus 16 `@PolicyOnly` stubs. Reconnect is covered (`connection-28` Scenario 10 in `01_lifecycle.feature`). Scope changes (in/out) covered in `04_visibility.feature`. Message ordering, reliability, request/response all covered in `02_messaging.feature`.

### E.2 — Deferred scenarios

**MINOR.**  
All `@Deferred` scenarios are also `@PolicyOnly` (they are policy/design invariants not testable via the harness API). No behavior-covering scenario is deferred due to missing implementation. The 17 deferred lifecycle scenarios are legitimately untestable (heartbeat timeout requires real wall-clock, malformed token requires transport-layer injection, etc.).

### E.3 — Error path coverage

**NOTABLE.**  
Happy paths are well covered. The following error paths are explicitly marked `@PolicyOnly` and have no executable scenario:

- Malformed identity token rejected (connection-23)
- Expired / reused tokens (connection-25)
- Heartbeat timeout disconnect (connection-19)
- Non-holder write rejection for delegated entities (entity-delegation-10, entity-authority-03)

The malformed-packet receive path is hit by the `read_malformed_packet` test in `compile_fail/` but this is a compile test, not a behavioral test. No scenario exercises the server's handling of a truncated or corrupted Data packet (the code discards it with `warn!`, which is correct — but uncovered by BDD).

**Proposed fix:** Add adversarial-packet scenarios to `00_foundations.feature` or a new `08_adversarial.feature`. These require harness support for injecting raw malformed bytes.

### E.4 — Reconnection

**OK.**  
Scenario `[connection-28]` covers: client connects, disconnects, reconnects, server has 1 connected client. Server-initiated disconnect is covered by multiple scenarios. Client-initiated disconnect is covered. The state-clean property after disconnect is tested indirectly by the lifecycle gate.

### E.5 — Property / fuzz gaps

**MINOR.**  
Sequence number wrapping is exercised by unit tests in `wrapping_number.rs` (6 wrap-around tests). However, there are no property-based tests for:
- Priority accumulator behavior under varied arrival patterns
- Tick buffer insert/prune under sequence number wrap
- `SequenceBuffer` correctness at the 65535→0 boundary under concurrent insert/remove

These are low-risk (the unit tests are deterministic and complete for the documented boundary cases) but property-based fuzz would add confidence.

---

## Track F — Dependency Health

### F.1 — Direct dependency audit

**OK.**  
Core dependencies: `smol`, `async-std` family, `log`, `naia-serde`, `naia-socket-shared`, `parking_lot`, `cfg-if`. All are actively maintained. `webrtc-unreliable` (server) and `webrtc-unreliable-client` (client) are older unmaintained packages locked at the DTLS stack version, but are the known P0 DTLS migration target (deferred to 2027-06-01 per MEMORY.md).

### F.2 — Security advisories

**MINOR** (by policy, not severity).  
`cargo deny check advisories` passes cleanly. There are 24 ignore entries in `deny.toml`, all time-boxed to 2027-06-01. The entries fall into three clusters:

1. **DTLS stack** (rustls 0.19, ring 0.16, webpki, reqwest 0.11) — 11 advisories, all blocked on the webrtc-unreliable-client migration.
2. **Unmaintained crates** (aesni, aes-soft, cpuid-bool, ring <0.17, rustls-pemfile 1.x) — 5 advisories, same root.
3. **Demo crates only** (adler, paste, macroquad) — 3 advisories, not in library.
4. **Narrow unsoundness** (rand custom logger, tokio broadcast) — naia's usage is unaffected.
5. **smol/fastrand** (instant unmaintained) — 1 advisory, resolves when smol bumps fastrand.

The high-severity items (RUSTSEC-2024-0336 rustls infinite-loop, RUSTSEC-2025-0004 openssl UAF) are real CVEs but are in the WebRTC transport path only; UDP-only deployments are unaffected. All ignores have justifications and expiry dates.

### F.3 — Duplicate dependencies

**MINOR.**  
`async-channel` has two versions (v1.9.0 via smol, v2.3.1 via blocking→smol). `async-io` v1.13.0 and v2.6.0 coexist. `async-lock` v2.8.0 and v3.4.0 coexist. `base16ct` has two versions. These are all transitive from `smol` vs `blocking` version split, not directly controllable without patching smol or the webrtc crates. Not actionable in isolation.

### F.4 — Feature bloat

**OK.**  
Feature flags (`wbindgen`, `mquad`, `transport_udp`, `transport_local`, `bevy_support`, `zstd_support`, `e2e_debug`, `test_utils`) are well-scoped. The `zstd_support` feature is optional and gated throughout `encoder.rs`. The `e2e_debug` feature adds atomic counters and extern hooks used only in tests.

---

## Track G — Protocol Correctness

### G.1 — Deserialization hardening

**OK.**  
Every `StandardHeader::de`, `HandshakeHeader::de`, `ComponentKind::de`, `EntityMessage::de` call returns `Result<_, SerdeErr>` and is handled with either `let Ok(…) else { continue/return None/warn }`. The server's `receive_all_packets` loop discards malformed packets with `warn!` and `continue`. The client's `process_all_packets` discards with `warn!`. No `unwrap()` on deserialization results in production code. Truncated packets cause `SerdeErr` which propagates cleanly.

### G.2 — State machine completeness

**OK.**  
`EntityAuthStatus` has 5 variants. Transitions are explicit in `entity_auth_status.rs` via `can_request`/`can_release`/`can_mutate`/`can_write`. Illegal server-side calls to `can_request`/`can_release` produce an `unreachable!` with a helpful message — good. The entity channel state machine (`EntityChannelState`: Despawned/Spawned) handles re-spawn correctly by resetting buffers. The `ScopeChecksCache` state machine is covered by 10 unit tests including a 10K-step randomized churn test.

### G.3 — Sequence number handling

**OK.**  
`wrapping_number.rs` provides `sequence_greater_than`, `sequence_less_than`, `wrapping_diff` with 12 unit tests covering wrap-around at both ends of the u16 range. `SequenceBuffer::insert` correctly uses `sequence_less_than` to reject stale entries. `AckManager` uses `sequence_greater_than` for packet ordering. `tick_buffer_receiver_channel` uses `sequence_greater_than` for tick ordering.

### G.4 — Tick buffer correctness

**OK.**  
`TickBufferReceiverChannel::insert`: messages from past ticks are rejected (`sequence_greater_than(*message_tick, *host_tick)` check). Messages far in the future are rejected (`buffer_limit_tick` check). Duplicate tick+index combinations are de-duplicated via `HashMap<ShortMessageIndex, MessageContainer>` per tick. The `TODO: should there be a maximum buffer size?` comment at line 144 notes a potential unbounded growth issue — see Track I.2.

### G.5 — Authority state consistency

**OK.**  
The authority state machine is well-tested by BDD (38 concrete scenarios). The `send_reset_authority_messages` / `send_all_packets` pending_auth_grants flush pattern ensures SetAuthority messages are sent to all in-scope clients after the entity is registered on the client (one-tick delay). The `entity_give_authority` path explicitly avoids double-send by sending SetAuthority in the per-connection loop and NOT pushing to `auth_grants`. This is documented in the Scenario(38) comment.

---

## Track H — Performance & Allocation

### H.1 — Hot path allocation survey

**MINOR.**  
`scope_checks_all()` returns `self.scope_checks_cache.as_slice().to_vec()` — a `Vec` clone on every call. At 1,262 CCU × 2^16 entities this is O(CCU × entities) allocation per tick. The method's doc correctly warns about this and recommends `scope_checks_pending()` for static-scope games, but the default example pattern in demos uses `scope_checks_all()`.

`broadcast_message_inner` calls `self.user_keys().iter().cloned().collect()` which allocates a `Vec<UserKey>` on every broadcast. Minor, not on the per-entity hot path.

Encoding path: `Encoder::encode` (non-zstd) does `payload.to_vec()` every call — one allocation per outgoing packet. This is inherent to the encode interface returning `&[u8]` via an internal buffer.

### H.2 — Broadcast allocation

**OK.**  
`MessageContainer` is `Arc<Box<dyn Message>>` since the broadcast optimization landed. `broadcast_message_inner` wraps once in `MessageContainer::new`, then `send_message_inner` clones the Arc (atomic refcount increment) per user. No `clone_box()` heap allocation per user. This is correctly documented in `message_container.rs`.

### H.3 — Scope check scaling

**OK.**  
`ScopeChecksCache` is O(1) read (returns a slice reference). The push-based mutations are O(churn) — only fire when rooms/users/entities change. The debug-build equivalence assertion runs every 1024th read with a full O(rooms × users × entities) recompute. Production is clean.

### H.4 — Priority accumulator

**OK.**  
Priority accumulators are per-user (`UserPriorityState`) and per-entity. Old priority data is cleaned when entities leave scope via `despawn_entity_worldless` → `ScopeChecksCache::on_entity_despawned`. There is no explicit bound on accumulator size, but the accumulator only exists for entities in scope (which is bounded by `ScopeChecksCache`).

---

## Track I — Configuration & Limits

### I.1 — Magic numbers

**MINOR.**  
- `REDUNDANT_PACKET_ACKS_SIZE = 32` in `ack_manager.rs` — named constant, good.
- `DEFAULT_SEND_PACKETS_SIZE = 256` in `ack_manager.rs` — named, good.
- `CacheMap::with_capacity(64)` in `advanced_handshaker.rs` — the 64-entry digest cache is unnamed. A replay-attack window that caches 64 challenge digests is an undocumented design choice.
- Scope-check assertion period `1024` in `world_server.rs` — inline literal, not a named constant.
- `FRAGMENTATION_LIMIT_BYTES = 400` in `constants.rs` — named, but rationale not documented (why 400 and not 1200?).

### I.2 — Unbounded collections

**NOTABLE.**  
`server/src/connection/tick_buffer_receiver_channel.rs` has a `VecDeque` of `(Tick, HashMap<ShortMessageIndex, MessageContainer>)`. Messages from future ticks are capped by `message_capacity` (from `TickBufferSettings`). However, if a misbehaving client sends many distinct tick values all within the capacity window, the VecDeque can grow to `message_capacity` entries, each potentially holding multiple messages. There is no per-connection message-count cap. The `TODO: should there be a maximum buffer size?` at line 144 acknowledges this.

Similarly, `server/src/handshake/advanced_handshaker.rs` has `address_to_timestamp_map: HashMap<SocketAddr, Timestamp>` (line 185) that grows unboundedly with distinct source addresses. An attacker sending handshake packets from many spoofed source addresses causes unbounded HashMap growth before any authentication completes.

**Proposed fix for tick buffer:** Add a per-connection total message count cap in `TickBufferReceiverChannel` that discards excess messages and optionally increments a suspicion counter.

**Proposed fix for address_to_timestamp_map:** Apply the same LRU eviction pattern used by `timestamp_digest_map` (CacheMap). Bound to a configurable `max_pending_connections` value (default ~1024).

### I.3 — Timeout and interval defaults

**OK.**  
`disconnection_timeout_duration = 30s`, `heartbeat_interval = 4s` — appropriate for WAN. `handshake_pings = 10` with `ping_interval = 1s` means 10s maximum handshake time — acceptable. `tick_interval = 50ms` (20Hz) default — appropriate for a game server. None of the defaults explain *why* the value was chosen; adding rationale comments to `ConnectionConfig::default()` would help operators tune for LAN vs WAN.

---

## Track J — Code Quality & Maintainability

### J.1 — TODO / FIXME density

**NOTABLE.**  
32 TODOs found in production code. By category:

| Category | Count | Examples |
|---|---|---|
| Performance (caching) | 7 | `"TODO: we can make this more efficient by caching which Entities are in each User's scope"` (×5 in world_server.rs) |
| Correctness / unfinished | 4 | `"TODO: Get actual host_type"`, `"TODO: Use extracted component_kinds"` in `host_world_manager.rs:430-431` |
| Bubble error up | 3 | handshaker.rs ×2, base_time_manager.rs ×1 |
| Design debt | 2 | `"TODO! make this agnostic to type of entity"` (entity_command.rs), `"TODO: refactor this to use a generic type"` |
| Incomplete feature | 1 | Session.rs `"This could be catastrophic.. I don't understand futures"` |

**CRITICAL concern:** `host_world_manager.rs:430-431` — the `on_delivered_migrate_response` function has two TODOs that pass empty/wrong values (`HostType::Client` hardcoded, `HashSet::new()` for component_kinds). This function is `#[allow(dead_code)]` and appears to be stub code for an unimplemented migration path. The function body is comments-only steps 2-4 and 9. If this migration path is ever activated, it will produce incorrect behavior silently.

**Proposed fix:** Mark `on_delivered_migrate_response` with a `compile_error!` or `unimplemented!()` body to prevent silent activation. Track the implementation in a named issue.

### J.2 — Dead code

**MINOR.**  
`#[allow(dead_code)]` appears at 22 sites. Most are justified:

- `shared/src/world/sync/bevy_integration.rs` — 6 items, all documented `// Public API for external Bevy adapters`. Correct.
- `shared/src/world/sync/auth_channel.rs` — 2 items: `state()` and `receiver_buffer_pop_front_until_and_excluding()` — getter/debug helpers.
- `shared/src/world/sync/auth_channel_receiver.rs` — 3 items: receiver inspection helpers.
- `shared/src/connection/bandwidth_accumulator.rs` — 3 items: `bytes_sent`, `bytes_received` measurement fields.
- `server/src/world/global_world_manager.rs` — 1 item: a getter.
- `shared/src/world/host/host_world_manager.rs:403` — the `on_delivered_migrate_response` function (see J.1 — CRITICAL).

The bandwidth_accumulator dead-code fields suggest the bandwidth measurement feature may be incomplete.

### J.3 — Clippy suppressions

**OK.**  
6 suppressions total:
- `#[allow(clippy::type_complexity)]` ×2 — in `bigmap.rs` for genuinely complex nested types. Justified.
- `#[allow(clippy::too_many_arguments)]` — in `world_writer.rs`. The function takes 7 parameters; extracting them into a struct would add indirection. Justified.
- `#[allow(clippy::module_inception)]` — `client/src/connection/mod.rs`. Module named `connection` containing a `connection.rs`. Minor, accepted.
- `#[cfg_attr(feature = "transport_udp", allow(dead_code))]` — transport module gating. Correct.

### J.4 — Largest files

The 10 largest:
1. `world_server.rs` — 3,699 LOC. Justified: contains the complete server-side entity replication, authority, delegation, scope, and tick logic. Complex but coherent. The 5 repeated "TODO: cache entity scope" comments suggest a future extract-to-scope-cache-manager refactor.
2. `client.rs` — 2,646 LOC. The full client protocol state machine. Similar to world_server.rs — dense but not disorganized.
3. `shared/derive/src/replicate.rs` — 1,499 LOC. Proc-macro code; large but self-contained.
4. `shared/src/world/local/local_world_manager.rs` — 1,390 LOC. Mixed host/remote world management. Could be split but is internally consistent.
5. `shared/src/world/component/entity_property.rs` — 1,158 LOC. Component property diffing logic. Justified complexity.

No file is doing unrelated things; the size is from protocol density, not lack of structure.

### J.5 — Comment quality

**OK.**  
The authority state machine in `entity_auth_status.rs` has thorough "why" comments on each method explaining the client-vs-server invariant. `scope_checks_cache.rs` has a block comment explaining the O(churn) design rationale. `message_container.rs` explains the `Arc<Box<dyn Message>>` design decision and why each path is safe. The weakest area is `session.rs` (socket layer) where the `// TODO: This could be catastrophic.. I don't understand futures` comment has not been resolved.

---

## Track K — Security

### K.1 — Auth path hardening

**NOTABLE.**  
The advanced handshaker's `timestamp_digest_map` caps at 64 entries — good for HMAC replay resistance. However, `address_to_timestamp_map` (which stores validated timestamps per source address) is unbounded and grows with each unique source IP (see I.2). An attacker sending handshake Step-2 packets from many IPs fills this map indefinitely before any auth decision is made.

A connection that never sends auth: in `MainServer::accept_connection`, the server calls `accept_connection` explicitly from the application callback. A client that completes the network handshake but never triggers `accept_connection` stays in the `users` map indefinitely — there is no timeout on the pending-auth state.

**Proposed fix:** Apply the same CacheMap LRU pattern to `address_to_timestamp_map`. Add a pending-connection timeout: if `accept_connection` or `reject_connection` is not called within N seconds of the auth callback, auto-reject.

### K.2 — Input validation on packet receive

**OK.**  
All packet fields are deserialized via `naia-serde` which propagates `SerdeErr` on truncation or out-of-range values. Entity keys arriving from clients are looked up in `LocalEntityMap` before use — unknown keys are silently discarded. Component kind IDs arriving from clients are validated against the `ComponentKinds` registry. No array-index-from-packet-field pattern was found.

### K.3 — Amplification

**MINOR.**  
A single Ping packet from any source address causes the server to call `self.time_manager.process_ping()` and send a Pong. This is O(1) work and doesn't require an established connection. A flood of Ping packets from spoofed addresses causes O(N) Pong sends to legitimate-looking addresses (UDP amplification). The Pong payload is similar in size to the Ping, so the amplification factor is ~1x — not a significant DRDoS risk. A rate-limit on Pong responses per source address would be a robust mitigation.

### K.4 — Resource exhaustion

**NOTABLE.**  
Two resource exhaustion vectors:

1. **address_to_timestamp_map** (see K.1, I.2): unbounded HashMap growth from spoofed source IPs.
2. **`been_handshaked_users: HashMap<SocketAddr, UserKey>`** in the advanced handshaker: also grows with completed-handshake addresses and is never pruned. When a client disconnects, `been_handshaked_users` retains the entry. Over time (long-running server with high churn), this map grows proportionally to total historical connections, not current connections.

**Proposed fix:** Clear `been_handshaked_users` entries on user disconnect in `MainServer::user_delete`.

---

## Summary

| Track | OK | MINOR | NOTABLE | CRITICAL |
|---|---|---|---|---|
| A — API Design | A.1, A.4 | A.2, A.5 | A.3 | — |
| B — Error Handling | B.1, B.3 | B.2 (empty panics), B.4 | B.2 (host_engine) | — |
| C — Safety & Soundness | C.4 | C.3 | C.1 (no Safety comments) | C.2 (transmute lifetime) |
| D — Documentation | D.2, D.3, D.5 | D.1 | D.4 (no Safety comments) | — |
| E — Test Coverage | E.1, E.2, E.4 | E.3, E.5 | — | — |
| F — Dependency Health | F.1, F.4 | F.2, F.3 | — | — |
| G — Protocol Correctness | G.1, G.2, G.3, G.4, G.5 | — | — | — |
| H — Performance | H.2, H.3, H.4 | H.1 | — | — |
| I — Configuration & Limits | I.3 | I.1 | I.2 (unbounded collections) | — |
| J — Code Quality | J.3, J.4, J.5 | J.2 | J.1 (migration stub TODOs) | J.1 (host_world_manager stub) |
| K — Security | K.2 | K.3 | K.1, K.4 | — |

**Total:** 22 OK · 13 MINOR · 9 NOTABLE · 1 CRITICAL

---

## Recommended Action List (NOTABLE + CRITICAL, prioritized by impact)

### 1. [CRITICAL] — Transmute lifetime extension in local transport (C.2)

**Files:** `server/src/transport/local/data.rs:49`, `client/src/transport/local/data.rs:72`  
**Risk:** Undefined behavior: the `MutexGuard` is dropped before the caller uses the "static" reference, allowing a concurrent `receive()` call to overwrite or free the underlying buffer.  
**Fix:** Change `receive()` to return `Result<Option<Box<[u8]>>>` (owned bytes, zero-copy with `into_boxed_slice()` already in hand) or introduce a guard wrapper that borrows from `Arc<Mutex<…>>` through the guard's lifetime. The transmute achieves nothing that a proper lifetime annotation cannot.

---

### 2. [NOTABLE] — address_to_timestamp_map unbounded growth (I.2 / K.1 / K.4)

**File:** `server/src/handshake/advanced_handshaker.rs:185`  
**Risk:** DoS: an attacker floods handshake Step-1 from many source IPs, growing the server's HashMap without limit, eventually causing OOM.  
**Fix:** Apply `CacheMap` (already exists in `server/src/handshake/cache_map.rs`) to `address_to_timestamp_map` with a configurable capacity (e.g., `max_pending_connections`, default 1024). Entries are evicted LRU when capacity is reached.

---

### 3. [NOTABLE] — been_handshaked_users never pruned (K.4)

**File:** `server/src/handshake/advanced_handshaker.rs` — `been_handshaked_users: HashMap<SocketAddr, UserKey>`  
**Risk:** Memory leak proportional to historical connection count on long-running servers.  
**Fix:** Call `been_handshaked_users.remove(&user_address)` in the user deletion path (`MainServer::user_delete`).

---

### 4. [NOTABLE] — Incomplete migration stub in host_world_manager (J.1)

**File:** `shared/src/world/host/host_world_manager.rs:403-449`  
**Risk:** `on_delivered_migrate_response` is dead code with two `// TODO` placeholders passing wrong values. If the migration feature is activated, component state will silently be lost.  
**Fix:** Replace the function body with `unimplemented!("on_delivered_migrate_response: migration path not implemented — see TODO comments")` to prevent silent misuse. Remove `#[allow(dead_code)]`.

---

### 5. [NOTABLE] — Panics on stale UserKey (A.3)

**Files:** `server/src/server/main_server.rs:202`, `server/src/server/world_server.rs:1194`  
**Risk:** Library consumer holding a `UserKey` from a disconnected user causes a server panic at a call like `server.user(&stale_key)` rather than returning `None` or `Err`.  
**Fix:** Convert `Server::user()` / `Server::user_mut()` to return `Option<UserRef>` / `Option<UserMut>`. Add `# Panics` documentation to any method that intentionally retains panic behavior for contract violations.

---

### 6. [NOTABLE] — No Safety comments on any unsafe block (C.1 / D.4)

**Files:** All 20 unsafe sites listed in Track C.1.  
**Risk:** Future maintainers cannot distinguish "safe by invariant" from "safe because we got lucky."  
**Fix:** Add `// Safety: <invariant>` comment before every `unsafe` block. Priority order: transmute sites, `Send/Sync` impls, Bevy `as_unsafe_world_cell` sites.

---

### 7. [NOTABLE] — Pending-auth connection has no timeout (K.1)

**File:** `server/src/server/main_server.rs` — `accept_connection` / `reject_connection` flow  
**Risk:** A client completing the network handshake but the application never calling `accept_connection` / `reject_connection` keeps the user in `self.users` indefinitely, consuming memory.  
**Fix:** Add a `pending_auth_timeout` to `ServerConfig` (default 10s). In `WorldServer::handle_disconnects`, also check users whose auth address is still set (i.e., awaiting auth decision) and auto-reject them after the timeout.

---

### 8. [NOTABLE] — host_engine panics on unknown entity message (B.2)

**File:** `shared/src/world/sync/host_engine.rs:62-63`  
**Risk:** A message for an unknown entity (reordered packets, stale message after despawn) causes `panic!("Cannot accept message for an entity that does not exist in the engine")`. This can crash the process if the panic is not caught.  
**Fix:** Replace with `warn!("host_engine: message for unknown entity {:?}, discarding", msg); return;`. If the entity was legitimately despawned, the message is stale and can be safely discarded.

---

### 9. [NOTABLE] — Empty panic messages in URL parsing (B.2)

**File:** `server/src/transport/udp.rs:365, 370, 374, 391, 398`  
**Risk:** `panic!("")` produces an empty panic message, making crash reports useless. The error reason is logged before the panic but the panic itself carries no context.  
**Fix:** Replace with `panic!("{}", SOCKET_PARSE_FAIL_STR)` or consolidate into a single `panic!("server_url_str is not a valid URL: {err_description}")` using the already-computed error string.
