# Feature Matrix

## Shipped

| Feature | Notes |
|---------|-------|
| Native (UDP) and browser (WebRTC / WASM) client support | Single codebase for both targets |
| Pluggable transport layer | `Socket` trait — UDP, WebRTC, and local-test implementations included |
| Connection / disconnection events with customisable authentication | |
| Heartbeats and host-timeout detection | |
| Typed message passing | Unordered-unreliable, sequenced-unreliable, unordered-reliable, ordered-reliable, tick-buffered |
| Typed request / response pairs | Over reliable channels |
| Entity replication with per-field delta compression | `Property<T>` change-detection |
| Static entities | Write-once, no per-tick diff tracking |
| Replicated resources | Server-side singletons, no room/scope config required |
| Two-level interest management | Rooms (coarse) + `UserScope` (fine-grained per-user visibility) |
| Authority delegation | Server grants/revokes client write authority over individual entities |
| `give_authority` / `take_authority` | Server-initiated authority transfer |
| Tick synchronisation | Server tick, client tick (leading by RTT/2), sub-tick interpolation fractions |
| Client-side prediction primitives | `TickBuffered` input delivery, `CommandHistory`, `local_duplicate()` |
| Lag compensation | `Historian` rolling per-tick world snapshot buffer for server-side rewind hit-detection |
| Priority-based bandwidth allocation | Per-entity gain, per-user gain, token-bucket send loop |
| Configurable per-connection bandwidth budget | `BandwidthConfig::target_bytes_per_sec` |
| Bitwise serialisation | Bit-packing, not byte-packing |
| Quantized numeric types | `UnsignedInteger<N>`, `SignedVariableFloat<BITS, FRAC>`, etc. |
| Optional zstd packet compression | Default, custom-dictionary, and dictionary-training modes |
| Connection diagnostics | RTT (EWMA + P50/P99), jitter, packet loss %, kbps sent/recv |
| Network condition simulation | `LinkConditionerConfig` (loss, latency, jitter presets) |
| Handshake flood mitigation | Bounded pending-connection map |
| Pending-auth timeout | Auto-reject unauthenticated connections |
| Panic-free stale-key lookup | `user_opt` / `user_mut_opt` |
| Reconnect correctness | Clients re-receive all in-scope entities and resources on reconnect |
| Safety comments on all `unsafe` blocks | Local-transport UB transmute eliminated |
| Bevy adapter | Server + client with multi-client phantom-type disambiguation |
| macroquad adapter | Client |
| BDD contract test harness | 215 contracts across 8 feature files |
| Criterion + iai-callgrind benchmark suite | |
| Fuzz targets | Five targets: packet header, packet body, quantized serde, replication decoder, handshake state machine |
| Enum support in `#[derive(Message)]` | Enum message types with proc-macro derived serialization |
| `DefaultClientTag` and `DefaultPlugin` | Reduces phantom-type boilerplate for single-client Bevy apps |
| Historian component-kind filtering | `enable_historian_filtered` snapshots only specified components |
| Optional `metrics` / `tracing` integration | `naia-metrics` and `naia-bevy-metrics` feature-gated crates |
| Per-connection message channel backpressure | `ReliableSettings::max_queue_depth`; `send_message` returns `Err(MessageQueueFull)` |

## Planned

| Feature | Status |
|---------|--------|
| `transport_quic` — TLS 1.3 native transport (Quinn-based) | XL effort, no set timeline |
| Per-component replication toggle | Fine-grained enable/disable per component on a replicated entity (issue #186) |
| iOS / Android native client socket | Blocked on `transport_quic` |
