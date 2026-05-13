# Feature Matrix

## Replication

| Feature | Notes |
|---------|-------|
| Entity replication | Per-field deltas through `Property<T>` |
| Static entities | Full snapshot on scope entry, no per-tick diff tracking |
| Replicated resources | Singleton values carried by hidden replicated entities |
| Client-authoritative entities | Opt-in via `Protocol::enable_client_authoritative_entities()` |
| Entity publication | Client-owned `Private`, `Public`, and `Delegated` states |
| Reconnect correctness | Re-sends in-scope entities and resources after reconnect |

## Authority

| Feature | Notes |
|---------|-------|
| Server-owned default model | Ordinary server-spawned entities/resources are server-owned |
| Authority delegation | Clients can request temporary authority over delegated entities/resources |
| Server authority control | Server can grant, deny, revoke, and reclaim authority |
| Scope-aware authority | Authority operations respect scope and delegated status |

## Interest And Bandwidth

| Feature | Notes |
|---------|-------|
| Rooms | Coarse interest groups |
| `UserScope` | Fine-grained per-user visibility |
| Scope exit policy | Despawn or persist/freeze when leaving scope |
| Priority-weighted bandwidth | Per-entity/per-user gain with token-bucket send loop |
| Per-connection bandwidth budgets | Target bytes per second |
| Message backpressure | Reliable channel queue limits return errors instead of silently growing forever |

## Messaging And Time

| Feature | Notes |
|---------|-------|
| Typed messages | Reliable/unreliable, ordered/unordered, sequenced, and tick-buffered modes |
| Typed request/response | Request trait associates each request with its response |
| Tick synchronization | Server/client ticks, RTT-aware timing, interpolation fractions |
| Prediction primitives | `TickBuffered`, `CommandHistory`, local duplicate patterns |
| Lag compensation | `Historian` snapshot buffer, including component-kind filtering |

## Transports

| Feature | Notes |
|---------|-------|
| WebRTC transport | Native and Wasm clients; DTLS; recommended production path |
| UDP transport | Native plaintext transport for dev/trusted/custom-secured deployments |
| Local transport | In-process deterministic tests and harnesses |
| Link conditioning | Loss, latency, and jitter presets for supported transports |

## Adapters And Tooling

| Feature | Notes |
|---------|-------|
| Bevy adapter | Server, client, shared protocol helpers, replicated resources |
| Macroquad/core path | Uses `naia-client` directly with `mquad` support |
| Custom world integration | Implement `WorldMutType` and `WorldRefType` |
| Metrics integration | `naia-metrics` and `naia-bevy-metrics` |
| Contract test harness | Scenario/spec coverage for replication, authority, scope, and transport behavior |
| Benchmarks and fuzzing | Criterion/iai-callgrind benches and protocol/serde fuzz targets |

## Serialization

| Feature | Notes |
|---------|-------|
| Bit-level serialization | Compact bit-packing |
| Quantized numeric types | Fixed-width and variable-width integer/float helpers |
| zstd compression | Optional default, custom dictionary, and dictionary-training modes |
| Enum messages | Supported by `#[derive(Message)]` |
