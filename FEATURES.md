# Features

## Shipped

- [x] Native (UDP) and browser (WebRTC / WASM) client support from a single codebase
- [x] Pluggable transport layer (`Socket` trait — UDP, WebRTC, and local-test implementations included)
- [x] Connection / disconnection events with customisable authentication
- [x] Heartbeats and host-timeout detection
- [x] Typed message passing: unordered-unreliable, sequenced-unreliable, unordered-reliable, ordered-reliable, tick-buffered
- [x] Typed request / response pairs over reliable channels
- [x] Entity replication with per-field delta compression (`Property<T>`)
- [x] Static entities (write-once, no per-tick diff tracking) — **server-side only**; see Planned for client-side
- [x] Replicated resources (server-side singletons, no room/scope config required)
- [x] Two-level interest management: rooms (coarse) + `UserScope` (fine-grained per-user visibility)
- [x] Authority delegation: server grants/revokes client write authority over individual entities
- [x] `give_authority` / `take_authority` — server-initiated authority transfer
- [x] Tick synchronisation: server tick, client tick (leading by RTT/2), sub-tick interpolation fractions
- [x] Client-side prediction primitives: `TickBuffered` input delivery, `CommandHistory`, `local_duplicate()`
- [x] Lag compensation: `Historian` rolling per-tick world snapshot buffer for server-side rewind hit-detection
- [x] Priority-based bandwidth allocation: per-entity gain, per-user gain, token-bucket send loop
- [x] Configurable per-connection bandwidth budget (`BandwidthConfig::target_bytes_per_sec`)
- [x] Bitwise serialisation (bit-packing, not byte-packing)
- [x] Quantized numeric types: `UnsignedInteger<N>`, `SignedVariableFloat<BITS, FRAC>`, etc.
- [x] Optional zstd packet compression with default, custom-dictionary, and dictionary-training modes
- [x] Connection diagnostics: RTT (EWMA + P50/P99), jitter, packet loss %, kbps sent/recv
- [x] Network condition simulation for local dev: `LinkConditionerConfig` (loss, latency, jitter presets)
- [x] Handshake flood mitigation (bounded pending-connection map)
- [x] Pending-auth timeout (auto-reject unauthenticated connections)
- [x] Panic-free stale-key lookup: `user_opt` / `user_mut_opt`
- [x] Reconnect correctness: clients re-receive all in-scope entities and resources on reconnect
- [x] Safety comments on all `unsafe` blocks; local-transport UB transmute eliminated
- [x] Bevy adapter (server + client) with multi-client phantom-type disambiguation
- [x] macroquad adapter (client)
- [x] BDD contract test harness (215 contracts across 8 feature files)
- [x] Criterion + iai-callgrind benchmark suite
- [x] Fuzz targets — five targets covering packet header, packet body, quantized serde types, replication protocol decoder, and handshake state machine
- [x] Enum support in `#[derive(Message)]` — enum message types with proc-macro derived serialization
- [x] `DefaultClientTag` and `DefaultPlugin` in Bevy adapter — reduces phantom-type boilerplate for single-client apps
- [x] Historian component-kind filtering — `enable_historian_filtered` snapshots only specified components
- [x] Optional `metrics` / `tracing` integration — `naia-metrics` and `naia-bevy-metrics` feature-gated observability crates
- [x] Per-connection message channel backpressure — `ReliableSettings::max_queue_depth` caps the unacknowledged message queue; `send_message` returns `Err(MessageQueueFull)` when the limit is reached

## Planned

- [x] Client-side static entities — `spawn_static_entity()` / `entity_mut.as_static()` in `naia-client`; `CommandsExt::as_static<T>()` in `naia-bevy-client`; mirrors the server-side API exactly
