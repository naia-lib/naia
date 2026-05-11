# Features

## Shipped

- [x] Native (UDP) and browser (WebRTC / WASM) client support from a single codebase
- [x] Pluggable transport layer (`Socket` trait — UDP, WebRTC, and local-test implementations included)
- [x] Connection / disconnection events with customisable authentication
- [x] Heartbeats and host-timeout detection
- [x] Typed message passing: unordered-unreliable, sequenced-unreliable, unordered-reliable, ordered-reliable, tick-buffered
- [x] Typed request / response pairs over reliable channels
- [x] Entity replication with per-field delta compression (`Property<T>`)
- [x] Static entities (write-once, no per-tick diff tracking)
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
- [x] Fuzz targets for packet header and packet body deserialisation

## Planned

- [ ] `transport_quic` — TLS 1.3 native transport (Quinn-based); XL effort, no set timeline
- [ ] Enum support in `#[derive(Message)]` — proc-macro gap, issue #163
- [ ] Per-component replication toggle — fine-grained enable/disable per component on a replicated entity, issue #186
- [ ] `DefaultClientTag` alias in Bevy adapter — reduces phantom-type boilerplate for single-client apps
- [ ] Historian component-kind filtering — opt-in allowlist to snapshot only specific components
- [ ] Additional fuzz targets: quantized serde types, replication protocol decoder, handshake state machine
- [ ] Optional `metrics` / `tracing` integration (feature-gated, no core API changes)
- [ ] iOS / Android native client socket (blocked on `transport_quic`)
