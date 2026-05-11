# A-17 Implementation Plan: `naia-metrics` + `naia-bevy-metrics`

**Branch:** `dev` — NEVER commit to `main`; main is touched only at tag time.  
**Gate:** `cargo check --workspace` must be clean (zero errors, zero warnings) after every step.  
**Commit cadence:** one commit per numbered step below.

---

## Overview

naia computes rich network health data internally — RTT (mean + p99), jitter, packet loss,
bandwidth per connection — but exposes it only through a polling API. There is no path from
that data to any monitoring system without custom plumbing. This feature adds two new crates
that make naia's observable health surface available via the `metrics` crate facade, which
users back with any exporter they choose (Prometheus, statsd, in-game egui dashboard, etc.).

Zero changes to `naia-server`, `naia-client`, or `naia-shared` internals — the new crates
call existing public APIs. The only additions to existing crates are:

1. Three small convenience methods on `WorldServer` / `Server` (Step 1)
2. `DefaultServerTag` + `DefaultServerPlugin` in the Bevy server adapter (Step 2)
3. `GlobalEntityMap::entity_count()` in naia-shared (Step 1)
4. Both new crates listed in the workspace `Cargo.toml` (Step 7)

---

## Existing code you must understand before starting

### `ConnectionStats`
`shared/src/connection/connection_stats.rs`  
Plain struct, already `pub` and exported from `naia-shared`:
```rust
pub struct ConnectionStats {
    pub rtt_ms: f32,
    pub rtt_p50_ms: f32,
    pub rtt_p99_ms: f32,
    pub jitter_ms: f32,
    pub packet_loss_pct: f32,   // range [0.0, 1.0]
    pub kbps_sent: f32,
    pub kbps_recv: f32,
}
```

### `UserKey`
`server/src/user/user.rs`  
```rust
pub struct UserKey(u64);
impl BigMapKey for UserKey {
    fn to_u64(&self) -> u64 { self.0 }
    fn from_u64(value: u64) -> Self { UserKey(value) }
}
```
Use `user_key.to_u64()` to get the numeric ID for metric labels.

### Existing server query APIs
All defined on `WorldServer<E>` in `server/src/server/world_server.rs` and delegated
through `Server<E>` in `server/src/server/server.rs`:
```rust
pub fn user_keys(&self) -> Vec<UserKey>          // line ~789
pub fn connection_stats(&self, user_key: &UserKey) -> Option<ConnectionStats>
pub fn room_keys(&self) -> Vec<RoomKey>           // line ~1728
```

### `GlobalEntityMap`
`shared/src/world/entity/global_entity_map.rs`  
Has `entity_to_global_map: HashMap<E, GlobalEntity>` as a private field.  
Needs a new `pub fn entity_count(&self) -> usize` (Step 1).

### WorldServer internal fields
`server/src/server/world_server.rs` lines ~127–135:
```rust
user_store: UserStore,
room_store: RoomStore,
global_entity_map: GlobalEntityMap<E>,
```

### Bevy system sets
`adapters/bevy/shared/src/system_set.rs` — exports `SendPackets` system set.  
`adapters/bevy/shared/src/lib.rs` line 54 — re-exports `SendPackets`.

### Bevy plugin phantom-type pattern
`adapters/bevy/client/src/lib.rs` — `Plugin<T>`, `Client<T>`, `DefaultClientTag`, `DefaultPlugin`.  
The server adapter follows the same pattern but `DefaultServerTag` does not yet exist (Step 2).

### Workspace members
`Cargo.toml` root — `members = [...]` array. New crates must be added here (Step 7).

---

## Step 1 — Add `entity_count()` to `GlobalEntityMap`; add `user_count()`, `entity_count()`, `room_count()` to `WorldServer` and `Server`

### 1a. `GlobalEntityMap::entity_count()`

File: `shared/src/world/entity/global_entity_map.rs`

Add after the existing `spawn` / `despawn` methods (wherever the other `pub fn` methods live):
```rust
pub fn entity_count(&self) -> usize {
    self.entity_to_global_map.len()
}
```

Make sure it is `pub` (not `pub(super)`) — it must be callable from `server/`.

### 1b. `WorldServer` convenience methods

File: `server/src/server/world_server.rs`

Add three `pub fn` methods. Place them near the existing `user_keys()` and `room_keys()`:

```rust
pub fn user_count(&self) -> usize {
    self.user_keys().len()
}

pub fn entity_count(&self) -> usize {
    self.global_entity_map.entity_count()
}

pub fn room_count(&self) -> usize {
    self.room_keys().len()
}
```

Note: `user_count()` and `room_count()` allocate a `Vec` internally via `user_keys()` /
`room_keys()`. This is acceptable — the metrics loop already iterates `user_keys()`, so
the allocation cost is already present.

### 1c. Delegate through `Server`

File: `server/src/server/server.rs`

Find the existing delegation block for `user_keys()` and add matching delegations:

```rust
pub fn user_count(&self) -> usize {
    self.world_server().user_count()
}

pub fn entity_count(&self) -> usize {
    self.world_server().entity_count()
}

pub fn room_count(&self) -> usize {
    self.world_server().room_count()
}
```

### 1d. Verify

```
cargo check --workspace
```

Zero errors, zero warnings. Commit:
```
git add shared/src/world/entity/global_entity_map.rs \
        server/src/server/world_server.rs \
        server/src/server/server.rs
git commit -m "feat: add user_count/entity_count/room_count to Server API (A-17 prereq)"
```

---

## Step 2 — Add `DefaultServerTag` + `DefaultPlugin` to naia-bevy-server

File: `adapters/bevy/server/src/lib.rs`

Check whether `DefaultServerTag` already exists. If it does not (expected: it does not),
add at the bottom of the file, mirroring the client adapter's pattern exactly:

```rust
/// Phantom tag type for single-server Bevy apps.
///
/// Pass this as the `T` parameter to [`Plugin`], [`Server`], and event types
/// when your app connects to exactly one server instance. For multi-server
/// apps define your own tag structs instead.
pub struct DefaultServerTag;

/// Alias for [`Plugin<DefaultServerTag>`] — for single-server apps.
pub type DefaultPlugin = Plugin<DefaultServerTag>;
```

Verify and commit:
```
cargo check --workspace
git add adapters/bevy/server/src/lib.rs
git commit -m "feat: add DefaultServerTag + DefaultPlugin to naia-bevy-server (A-17 prereq)"
```

---

## Step 3 — Create `naia-metrics` crate

Create the directory and files:

```
metrics/
  Cargo.toml
  src/
    lib.rs
    names.rs
    server.rs
    client.rs
```

### `metrics/Cargo.toml`

```toml
[package]
name = "naia-metrics"
version = "0.25.0"
authors = ["naia authors"]
description = "Observability emission layer for naia game networking — emits ConnectionStats via the metrics crate facade"
license = "MIT OR Apache-2.0"
edition = "2021"
publish = false   # set to true when publishing to crates.io

[dependencies]
metrics = "0.24"
naia-shared = { path = "../shared", default-features = false }
```

`naia-shared` is needed only for the `ConnectionStats` type. The `default-features = false`
keeps the dep minimal — we don't need transport, Bevy, or anything else.

### `metrics/src/names.rs`

All metric name strings as `pub const`. No metric name string literal appears anywhere else.

```rust
// Server aggregate metrics (no label)
pub const SERVER_CONNECTED_USERS: &str = "naia_server_connected_users";
pub const SERVER_TOTAL_ENTITIES:  &str = "naia_server_total_entities";
pub const SERVER_TOTAL_ROOMS:     &str = "naia_server_total_rooms";

// Server per-connection metrics (label: user_id)
pub const SERVER_CONN_RTT_MS:      &str = "naia_server_conn_rtt_ms";
pub const SERVER_CONN_RTT_P99_MS:  &str = "naia_server_conn_rtt_p99_ms";
pub const SERVER_CONN_JITTER_MS:   &str = "naia_server_conn_jitter_ms";
pub const SERVER_CONN_PACKET_LOSS: &str = "naia_server_conn_packet_loss";
pub const SERVER_CONN_KBPS_SENT:   &str = "naia_server_conn_kbps_sent";
pub const SERVER_CONN_KBPS_RECV:   &str = "naia_server_conn_kbps_recv";

// Client connection metrics (no label — one connection per process)
pub const CLIENT_CONN_RTT_MS:      &str = "naia_client_conn_rtt_ms";
pub const CLIENT_CONN_JITTER_MS:   &str = "naia_client_conn_jitter_ms";
pub const CLIENT_CONN_PACKET_LOSS: &str = "naia_client_conn_packet_loss";
pub const CLIENT_CONN_KBPS_SENT:   &str = "naia_client_conn_kbps_sent";
pub const CLIENT_CONN_KBPS_RECV:   &str = "naia_client_conn_kbps_recv";
```

### `metrics/src/server.rs`

```rust
use naia_shared::ConnectionStats;
use crate::names;

/// Emit the three server-wide aggregate gauges.
///
/// Call once per tick after [`Server::send_all_packets`].
pub fn emit_server_aggregates(user_count: usize, entity_count: usize, room_count: usize) {
    metrics::gauge!(names::SERVER_CONNECTED_USERS).set(user_count as f64);
    metrics::gauge!(names::SERVER_TOTAL_ENTITIES).set(entity_count as f64);
    metrics::gauge!(names::SERVER_TOTAL_ROOMS).set(room_count as f64);
}

/// Emit the six per-connection gauges for one user.
///
/// `user_id` is `UserKey::to_u64()`. Call once per connected user per tick.
pub fn emit_server_connection_stats(stats: &ConnectionStats, user_id: u64) {
    let id = user_id.to_string();
    let label = [("user_id", id.as_str())];
    metrics::gauge!(names::SERVER_CONN_RTT_MS,      &label[..]).set(stats.rtt_ms as f64);
    metrics::gauge!(names::SERVER_CONN_RTT_P99_MS,  &label[..]).set(stats.rtt_p99_ms as f64);
    metrics::gauge!(names::SERVER_CONN_JITTER_MS,   &label[..]).set(stats.jitter_ms as f64);
    metrics::gauge!(names::SERVER_CONN_PACKET_LOSS, &label[..]).set(stats.packet_loss_pct as f64);
    metrics::gauge!(names::SERVER_CONN_KBPS_SENT,   &label[..]).set(stats.kbps_sent as f64);
    metrics::gauge!(names::SERVER_CONN_KBPS_RECV,   &label[..]).set(stats.kbps_recv as f64);
}
```

Note: The `metrics` crate macro syntax for dynamic labels may differ slightly depending on
the exact 0.24.x patch. Consult `metrics` crate docs if the above does not compile; the
intent is to pass `"user_id"` as the label key and the decimal `u64` string as the value.

### `metrics/src/client.rs`

```rust
use naia_shared::ConnectionStats;
use crate::names;

/// Emit the five client-side connection gauges.
///
/// The client has exactly one connection, so no label is needed.
/// Call once per tick after [`Client::send_all_packets`].
pub fn emit_client_connection_stats(stats: &ConnectionStats) {
    metrics::gauge!(names::CLIENT_CONN_RTT_MS).set(stats.rtt_ms as f64);
    metrics::gauge!(names::CLIENT_CONN_JITTER_MS).set(stats.jitter_ms as f64);
    metrics::gauge!(names::CLIENT_CONN_PACKET_LOSS).set(stats.packet_loss_pct as f64);
    metrics::gauge!(names::CLIENT_CONN_KBPS_SENT).set(stats.kbps_sent as f64);
    metrics::gauge!(names::CLIENT_CONN_KBPS_RECV).set(stats.kbps_recv as f64);
}
```

Note: `rtt_p99_ms` is intentionally omitted on the client side. The client's `TimeManager`
does not maintain a percentile ring buffer — only the server-side `PingManager` does.
Emitting the mean value under the p99 name would be misleading. Client-side p99 tracking
is deferred to Phase 2.

### `metrics/src/lib.rs`

```rust
//! Observability emission layer for naia game networking.
//!
//! Emits network health data — RTT, jitter, packet loss, bandwidth — via the
//! [`metrics`] crate facade. Install any compatible exporter at startup and
//! naia's data flows to your monitoring backend automatically.
//!
//! # Non-Bevy usage
//!
//! Call once per tick after `server.send_all_packets()`:
//!
//! ```rust,ignore
//! naia_metrics::emit_server_aggregates(
//!     server.user_count(),
//!     server.entity_count(),
//!     server.room_count(),
//! );
//! for user_key in server.user_keys() {
//!     if let Some(stats) = server.connection_stats(&user_key) {
//!         naia_metrics::emit_server_connection_stats(&stats, user_key.to_u64());
//!     }
//! }
//! ```
//!
//! For Bevy apps, use [`naia-bevy-metrics`] instead — it handles emission
//! automatically via a plugin.

pub mod names;
mod server;
mod client;

pub use server::{emit_server_aggregates, emit_server_connection_stats};
pub use client::emit_client_connection_stats;
```

---

## Step 4 — Create `naia-bevy-metrics` crate

```
adapters/bevy/metrics/
  Cargo.toml
  src/
    lib.rs
    server_plugin.rs
    client_plugin.rs
```

### `adapters/bevy/metrics/Cargo.toml`

```toml
[package]
name = "naia-bevy-metrics"
version = "0.25.0"
authors = ["naia authors"]
description = "Bevy plugins for naia metrics — automatically emits ConnectionStats each tick"
license = "MIT OR Apache-2.0"
edition = "2021"
publish = false   # set to true when publishing

[features]
server = ["dep:naia-bevy-server"]
client = ["dep:naia-bevy-client"]

[dependencies]
naia-metrics    = { path = "../../metrics" }
naia-bevy-shared = { path = "../shared" }
bevy_app        = { version = "0.18" }
bevy_ecs        = { version = "0.18" }
naia-bevy-server = { path = "../server", optional = true }
naia-bevy-client = { path = "../client", optional = true }
```

### `adapters/bevy/metrics/src/server_plugin.rs`

```rust
#![cfg(feature = "server")]

use std::marker::PhantomData;
use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::IntoScheduleConfigs;
use naia_bevy_server::Server;
use naia_bevy_shared::SendPackets;
use naia_metrics::{emit_server_aggregates, emit_server_connection_stats};

/// Bevy plugin that emits naia server metrics once per tick, immediately
/// after naia's [`SendPackets`] system.
///
/// Generic over the same phantom tag type `T` used by [`naia_bevy_server::Plugin<T>`]
/// and [`Server<T>`]. For single-server apps, use [`DefaultServerMetricsPlugin`].
pub struct NaiaServerMetricsPlugin<T: Send + Sync + 'static>(PhantomData<T>);

impl<T: Send + Sync + 'static> Default for NaiaServerMetricsPlugin<T> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<T: Send + Sync + 'static> Plugin for NaiaServerMetricsPlugin<T> {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, emit_server_metrics::<T>.after(SendPackets));
    }
}

fn emit_server_metrics<T: Send + Sync + 'static>(server: Server<T>) {
    emit_server_aggregates(
        server.user_count(),
        server.entity_count(),
        server.room_count(),
    );
    for user_key in server.user_keys() {
        if let Some(stats) = server.connection_stats(&user_key) {
            emit_server_connection_stats(&stats, user_key.to_u64());
        }
    }
}
```

### `adapters/bevy/metrics/src/client_plugin.rs`

```rust
#![cfg(feature = "client")]

use std::marker::PhantomData;
use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::IntoScheduleConfigs;
use naia_bevy_client::Client;
use naia_bevy_shared::SendPackets;
use naia_metrics::emit_client_connection_stats;

/// Bevy plugin that emits naia client metrics once per tick, immediately
/// after naia's [`SendPackets`] system.
///
/// Generic over the same phantom tag type `T` used by [`naia_bevy_client::Plugin<T>`]
/// and [`Client<T>`]. For single-server apps, use [`DefaultClientMetricsPlugin`].
pub struct NaiaClientMetricsPlugin<T: Send + Sync + 'static>(PhantomData<T>);

impl<T: Send + Sync + 'static> Default for NaiaClientMetricsPlugin<T> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<T: Send + Sync + 'static> Plugin for NaiaClientMetricsPlugin<T> {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, emit_client_metrics::<T>.after(SendPackets));
    }
}

fn emit_client_metrics<T: Send + Sync + 'static>(client: Client<T>) {
    if let Some(stats) = client.connection_stats() {
        emit_client_connection_stats(&stats);
    }
}
```

### `adapters/bevy/metrics/src/lib.rs`

```rust
//! Bevy plugins for naia game networking metrics.
//!
//! Add [`NaiaServerMetricsPlugin`] (or [`DefaultServerMetricsPlugin`]) to your
//! Bevy [`App`] and naia's network health data is emitted automatically each
//! tick via the [`metrics`] crate facade.
//!
//! # Setup
//!
//! ```rust,ignore
//! // 1. Install a metrics exporter at startup (user's choice of backend):
//! //    e.g. metrics_exporter_prometheus, metrics_exporter_statsd, etc.
//!
//! // 2. Add the plugin:
//! App::new()
//!     .add_plugins(NaiaServerPlugin::new(server_config(), protocol()))
//!     .add_plugins(DefaultServerMetricsPlugin::default())
//!     .run();
//! ```
//!
//! # Features
//!
//! Enable `server` for [`NaiaServerMetricsPlugin`]; `client` for
//! [`NaiaClientMetricsPlugin`]. Both can be enabled simultaneously for
//! listen-server setups.

#[cfg(feature = "server")]
mod server_plugin;
#[cfg(feature = "client")]
mod client_plugin;

#[cfg(feature = "server")]
pub use server_plugin::NaiaServerMetricsPlugin;
#[cfg(feature = "client")]
pub use client_plugin::NaiaClientMetricsPlugin;

// Convenience aliases matching the DefaultPlugin / DefaultClientTag pattern
// used across the naia Bevy adapters.
#[cfg(feature = "server")]
pub use naia_bevy_server::DefaultServerTag;
#[cfg(feature = "server")]
pub type DefaultServerMetricsPlugin = NaiaServerMetricsPlugin<DefaultServerTag>;

#[cfg(feature = "client")]
pub use naia_bevy_client::DefaultClientTag;
#[cfg(feature = "client")]
pub type DefaultClientMetricsPlugin = NaiaClientMetricsPlugin<DefaultClientTag>;
```

---

## Step 5 — Add both crates to the workspace

File: `Cargo.toml` (workspace root)

In the `members = [...]` array, add both new crates after the existing adapter entries:

```toml
"metrics",
"adapters/bevy/metrics",
```

Place `"metrics"` near the top-level crates (`"client"`, `"server"`, `"shared"`, …).
Place `"adapters/bevy/metrics"` after the other `adapters/bevy/*` entries.

Do **not** add either to `default-members` — the metrics crates are optional and should
not affect the default build.

---

## Step 6 — Verify everything builds

```bash
cargo check --workspace
```

Expected: zero errors, zero warnings.

If you get an error about the `metrics` macro syntax, check the exact `metrics = "0.24"`
API in its documentation. The label-passing syntax may need adjustment — the intent is:
- Label key: the string `"user_id"`
- Label value: the decimal u64 string (`user_key.to_u64().to_string()`)

Also verify wasm32 builds cleanly (naia enforces this on CI for client crates):

```bash
cargo check -p naia-metrics --target wasm32-unknown-unknown
cargo check -p naia-bevy-metrics --features client --target wasm32-unknown-unknown
```

`naia-metrics` and the client plugin must compile for wasm32 because naia-bevy-client
targets the browser. The `metrics` crate supports wasm32.

---

## Step 7 — Commit

```bash
git add metrics/ adapters/bevy/metrics/ Cargo.toml Cargo.lock
git commit -m "feat(A-17): add naia-metrics and naia-bevy-metrics observability crates

- naia-metrics: emits ConnectionStats via metrics crate facade for server and client
- naia-bevy-metrics: NaiaServerMetricsPlugin<T> + NaiaClientMetricsPlugin<T> with
  DefaultServerMetricsPlugin / DefaultClientMetricsPlugin convenience aliases
- 14 gauges total: 3 server aggregate, 6 server per-connection (user_id label), 5 client
- all metric names as pub const in naia_metrics::names — no magic strings at call sites
- scheduling: Update after SendPackets — fresh data from the just-completed tick"
```

---

## Metric catalog (reference)

| Metric name | Type | Label | Source field |
|-------------|------|-------|--------------|
| `naia_server_connected_users` | Gauge | — | `server.user_count()` |
| `naia_server_total_entities` | Gauge | — | `server.entity_count()` |
| `naia_server_total_rooms` | Gauge | — | `server.room_count()` |
| `naia_server_conn_rtt_ms` | Gauge | `user_id` | `stats.rtt_ms` |
| `naia_server_conn_rtt_p99_ms` | Gauge | `user_id` | `stats.rtt_p99_ms` |
| `naia_server_conn_jitter_ms` | Gauge | `user_id` | `stats.jitter_ms` |
| `naia_server_conn_packet_loss` | Gauge | `user_id` | `stats.packet_loss_pct` |
| `naia_server_conn_kbps_sent` | Gauge | `user_id` | `stats.kbps_sent` |
| `naia_server_conn_kbps_recv` | Gauge | `user_id` | `stats.kbps_recv` |
| `naia_client_conn_rtt_ms` | Gauge | — | `stats.rtt_ms` |
| `naia_client_conn_jitter_ms` | Gauge | — | `stats.jitter_ms` |
| `naia_client_conn_packet_loss` | Gauge | — | `stats.packet_loss_pct` |
| `naia_client_conn_kbps_sent` | Gauge | — | `stats.kbps_sent` |
| `naia_client_conn_kbps_recv` | Gauge | — | `stats.kbps_recv` |

`naia_client_conn_rtt_p99_ms` is intentionally absent — the client doesn't track RTT
percentiles. Deferred to Phase 2 (add 32-sample ring buffer to client-side `TimeManager`).

---

## What is explicitly out of scope (Phase 2)

- **Replication event counters** — `naia_server_spawns_total`, `naia_server_despawns_total`,
  `naia_server_component_inserts_total`, `naia_server_component_removes_total`. Requires
  adding `metrics::counter!` call sites inside `naia-shared/src/world/host/` internals.

- **Channel-level message throughput** — `naia_messages_sent_total{channel="..."}`. Requires
  instrumenting the channel send/receive paths in `naia-shared`.

- **Client-side RTT p99** — add 32-sample ring buffer to `client/src/connection/time_manager.rs`
  matching the server-side `PingManager`.

- **Any specific exporter** — naia ships no exporter. Users install their own at startup.
  The `metrics` facade is zero-overhead when no exporter is installed (no-op recorder).

---

## Verification checklist before pushing

- [ ] `cargo check --workspace` — zero errors, zero warnings
- [ ] `cargo check -p naia-metrics --target wasm32-unknown-unknown` — clean
- [ ] `cargo check -p naia-bevy-metrics --features client --target wasm32-unknown-unknown` — clean
- [ ] Both new crates appear in `Cargo.toml` workspace members
- [ ] `DefaultServerTag` is exported from `adapters/bevy/server/src/lib.rs`
- [ ] All metric name strings are only in `names.rs` — no string literals elsewhere in the crates
- [ ] `git branch` shows `dev`, not `main`
