# Priority Accumulator — Implementation Plan

**Status:** ✅ COMPLETE 2026-04-24. All phases landed, all gates green. See `BENCH_UPGRADE_LOG/sidequest-priority-accumulator.md` for post-mortem. Do not re-implement any part of this plan.
**Companions:** `PRIORITY_ACCUMULATOR_SIDEQUEST.md` (scope) · `PRIORITY_ACCUMULATOR_RESEARCH.md` (Fiedler distillation + prior art).
**Scope reminder:** sender-side, symmetric. Both server and client assemble outbound packets and need the same pacing/priority machinery. Naia supports client-authoritative messages/requests/responses/entities, so everything below applies identically to both peers.

---

## Part I — Relevant Naia internals (verified 2026-04-24)

The machinery the plan must fit into. Verified by reading source, not inferred.

### I.1 · The four surfaces

| Surface | Send path | Receive path |
|---|---|---|
| **Component updates (state)** | `EntityUpdateManager` + `mut_channel` + `user_diff_handler` → `WorldWriter::write_updates` | `WorldReader::read_updates` applies immediately; no per-entity buffering |
| **Entity commands** (`Un/OrderedReliable`) | `HostEngine`/`HostEntityChannel` → `LocalWorldManager::take_outgoing_events` → `ReliableSender<EntityCommand>` → `WorldWriter::write_commands` | `ReliableReceiver` dedup → `UnorderedArranger` pass-through → `RemoteEntityChannel::process_messages` (per-entity FSM) |
| **Plain messages** | Per-channel `ChannelSender` (Reliable / Unreliable / Sequenced / Tick-Buffered) | Per-channel `ChannelReceiver` + matching arranger |
| **Request / response** | Built on plain messages; reliable | Matching receiver + response correlation |

Entity commands ride the **same `ReliableSender`** whose fast-path Phase 4 fixed — which is why Phase 4.5's spike and this sidequest are directly coupled.

### I.2 · The `sync/` layer

`shared/src/world/sync/` partitions one unordered-reliable stream into per-entity ordered streams. From the module's doc comment:

> *"Higher layers treat the engine as if every entity had its own perfect ordered stream — while the network enjoys the performance of a single unordered reliable channel."*

Key pieces: `HostEngine` / `HostEntityChannel` (sender FSM), `RemoteEngine` / `RemoteEntityChannel` (receive-side demultiplexer, owns `OrderedIds<EntityMessage<()>>` that sorts by CommandId via wrap-safe `sequence_less_than`), `RemoteComponentChannel` (per-component idempotent toggle), `AuthChannel` (authority FSM with its own `subcommand_id` sequence), `ordered_ids.rs` (wrap-safe sorted buffer), `config.rs` (wrap-around guard bands).

### I.3 · The "semi-ordered" contract — what we can and cannot reorder

1. **Per-entity: strictly ordered.** `OrderedIds::push_back` sorts by CommandId; `RemoteEntityChannel::process_messages` (`remote_entity_channel.rs:222-258`) enforces the **spawn barrier** — components/auth wait until the Spawn arrives.
2. **Across entities: unordered.** Each entity has its own channel.
3. **Per-component: idempotent toggle.** `last_epoch_id` guard dedupes replayed Insert/Remove.
4. **Per-entity auth: strict.** `AuthChannelReceiver::next_subcommand_id` (u16, separate from CommandId).
5. **Epoch guard.** Messages with `id < last_epoch_id` dropped (`remote_entity_channel.rs:168`).

**Hard rule for this plan: cross-entity reorder is safe for updates AND commands. Within a single entity's command stream, CommandId monotonicity is non-negotiable.** This is the permissive property the unified priority sort exploits (Part III / IV).

### I.4 · Current send path — the spike origin

Three stages, no pacing at any:

1. `ReliableSender::collect_messages(now, rtt_millis)` pushes all messages whose `last_sent + 1.5 × rtt_ms ≤ now` into `outgoing_messages`. All-or-nothing.
2. `IndexedMessageWriter::write_messages` drains into the current packet until overflow.
3. `Connection::send_packets` loops **until the queue is empty**. No bandwidth cap, no per-tick packet cap. Server at `server/src/connection/connection.rs:263-282`; client has a structurally equivalent loop.

Phase 4.5 spike: 10K queued commands × 1.5×RTT window → 100 packets fire in one tick at t≈17. See `PRIORITY_ACCUMULATOR_RESEARCH.md` for the full timeline math.

### I.5 · Hook surfaces (file:line) — where we attach

1. **`server::connection::Connection::send_packets` (263-282)** + client analog — home for the bandwidth budget + unified sort; wraps all channels at the outermost send boundary.
2. **`local_world_manager.rs:974-990`** — entity-command drain; per-entity candidate enumeration happens here.
3. **`WorldWriter::write_updates` (~745-900)** — component-update candidate enumeration for the same unified sort.
4. **`ReliableSender::collect_messages`** — untouched; Phase 4 fast-path must be preserved.
5. **`WorldWriter::write_commands` (72-151)** — untouched for reorder; cross-entity reorder happens upstream at the `send_message` boundary (point 2), before `IndexedMessageWriter` sees CommandId order.

---

## Part II — Locked decisions

All resolved 2026-04-24 by Connor.

| # | Decision | Rationale |
|---|---|---|
| D1 | **Sender-side, symmetric.** Lives in `naia_shared`; server and client outbound paths both consume the same primitive. | Naia supports client-authoritative flows; same pathology both directions. |
| D2 | **Halo tier mapping.** State ↔ component updates; Events ↔ plain messages on unreliable channels; Control ↔ Tick-Buffered channels. | Sharpens accumulator scope and default gains. |
| D3 | **Sequencing.** Phase A ships bandwidth accumulator + unified priority sort (using default gains). Phase B exposes user-facing priority handles. Both within this sidequest. | Phase A is independently valuable and sufficient to close Phase 4.5. |
| D4 | **Bandwidth default: 64 000 B/s (512 kbps), configurable via `ConnectionConfig.bandwidth`.** | Matches Fiedler's example bracket; generous headroom (~30× Halo's per-client amortized). |
| D5 | **Unified priority sort — NOT tier reservation.** All outbound candidates (entity bundles + per-channel message batches) compete by accumulated priority each tick, gated by the bandwidth budget. **Realized form (2026-04-24):** intra-section k-way merge — entity bundles sorted by accumulator within `write_updates`; per-channel message batches sorted by `age × gain` within their write paths; bandwidth budget gates across sections. Wire format places sections in fixed order (messages → updates → commands), so byte-level interleave across sections would require a wire-format change which the sidequest non-goals rule out. The achievable form preserves anti-starvation within each candidate class and eliminates tier-specific starvation paths via the shared bandwidth accumulator. A literal cross-section interleaved sort remains a future v2 if/when the wire format is revised. | Anti-starvation by construction; eliminates tier-specific starvation paths; elegance; matches Unity/Unreal production designs. |
| D6 | **`ChannelCriticality::{Low, Normal, High}` survives** — as the `base_gain()` source feeding the unified sort (Low=0.5, Normal=1.0, High=10.0). Defaults derived per `ChannelMode`; overridable per channel via `ChannelSettings::with_criticality`. | Sane defaults; channel priority without numeric-weight footguns; TickBuffered wins handily without special-case tier code. |
| D7 | **No `PriorityConfig` on `ConnectionConfig`.** Default gains are constants (entity gain = 1.0; channel gains = D6 constants); user tuning is per-entity via handle APIs. | Dropping explicit staleness knob removed the only candidate field; no knob is elegantly zero knobs. |
| D8 | **All implementation in `naia_shared`.** Server/client/adapters only pass config and surface handle methods. | Existing Naia pattern; zero adapter surface creep. |
| D9 | **Priority granularity: per-entity, two-layer.** `global_entity_priority*(entity)` — sender-wide, entity-lifetime persistent, evicted on despawn. `user_entity_priority*(user_key, entity)` — per-connection, evicted on scope exit. Effective gain = `global × per_user`. Client has only `entity_priority*(entity)` (one connection). | Set-and-forget for global; per-user unleaks across scope churn; mirrors Halo's per-receiver-per-object priority without forcing all users onto the fine-grained path. |
| D10 | **Accumulator unit: per-message on-the-fly + per-entity-bundle stored.** Messages compute `(current_tick − enqueue_tick) × channel.base_gain()` from the queue's existing `enqueue_tick`; no stored per-message state. Entity bundles have stored `accumulated` floats. Sort is a k-way merge across per-channel FIFO heads + entity-bundle heap. | O(items_emitted × log k) instead of O(N log N) over 10K-message channels; oldest-wins-within-channel emerges for free; Fiedler semantics preserved. |
| D11 | **Handle API shape.** `global_entity_priority_mut` / `user_entity_priority_mut` return `EntityPriorityMut`; read-only `*_priority` returns `EntityPriorityRef`. `set_gain(g)` persistent, `boost_once(a)` transient additive, `reset()` clears gain override. Lazy entry creation on first write (set-and-forget works pre-scope). | Mirrors existing `UserScopeMut` extension pattern — no callbacks, no trait inversion. |
| D12 | **Reset-on-send.** Entity bundles that fit: `accumulated = 0`, `last_sent_tick = current_tick`. Messages that fit: channel advances `enqueue_tick` to now. Skipped items: untouched (compound next tick). | Canonical Fiedler semantic; makes starvation structurally impossible. |
| D13 | **Telemetry minimum.** Always-on: `bytes_sent_per_tick`, `budget_remaining_end_of_tick`, `oldest_unsent_age_ticks`. `#[cfg(feature = "bench_instrumentation")]`: top-N priority ring buffer + timing counters + `packets_deferred_due_to_budget`. | YAGNI-disciplined; reactively add "why is X at priority Y" only if an idle-grenade pathology fires. |
| D14 | **Phase 4.5 verification happens after Phase A** (or after both phases if Phase A alone doesn't close it). | Phase A's bandwidth cap + unified sort is the direct fix for the 10K-command spike; Phase B adds user knobs. |
| D15 | **Bevy adapters pass new handle APIs through.** Thin passthroughs, same pattern as `user_scope_mut` today. | Existing Naia adapter discipline. |
| D16 | **All gates stay green throughout.** `cargo test --workspace`, `namako gate`, `idle_distribution` matrix, criterion bench matrix. | Existing rigor mandate. |

---

## Part III — API design

Full API sketch at every crate seam. This is the surface implementation must hit.

### III.1 · Configuration types (new, in `naia_shared`)

```rust
// shared/src/connection/bandwidth.rs (new file)

/// Per-connection outbound bandwidth budget. Applied symmetrically
/// to server-outbound and client-outbound send loops.
///
/// Not to be confused with `ConnectionConfig::bandwidth_measure_duration`,
/// which is a telemetry/averaging window — this is the actual token-bucket cap.
#[derive(Clone, Debug)]
pub struct BandwidthConfig {
    /// Target outbound bytes-per-second per connection.
    /// Budget accumulates as `target_bytes_per_sec × dt` each tick;
    /// surplus carries into the next tick (Fiedler token-bucket).
    pub target_bytes_per_sec: u32,
}

impl BandwidthConfig {
    /// 512 kbps — generous default; overridable.
    pub const DEFAULT_TARGET_BYTES_PER_SEC: u32 = 64_000;
}

impl Default for BandwidthConfig {
    fn default() -> Self {
        Self { target_bytes_per_sec: Self::DEFAULT_TARGET_BYTES_PER_SEC }
    }
}
```

```rust
// shared/src/connection/connection_config.rs (extended)

pub struct ConnectionConfig {
    pub disconnection_timeout_duration: Duration,
    pub heartbeat_interval: Duration,
    pub bandwidth_measure_duration: Option<Duration>,  // existing — telemetry/averaging window
    pub bandwidth: BandwidthConfig,                     // NEW — outbound token-bucket cap
}
```

No `PriorityConfig` field — default gains are constants; user tuning is per-entity via handles (III.4–III.6).

### III.2 · `ChannelCriticality` (extending existing channel config)

```rust
// shared/src/messages/channels/channel.rs (extended)

/// How urgent a channel's traffic is in the unified priority sort.
/// Each pending message contributes `base_gain()` per tick to its accumulator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChannelCriticality {
    /// Droppable under load (unreliable events).
    Low,
    /// Must eventually go (reliable messages, state updates).
    Normal,
    /// Must go this tick (tick-buffered inputs, critical control).
    High,
}

impl ChannelCriticality {
    /// Default criticality derived from the channel's mode.
    pub const fn default_for(mode: &ChannelMode) -> Self {
        match mode {
            ChannelMode::TickBuffered(_) => Self::High,
            ChannelMode::UnorderedReliable(_)
            | ChannelMode::SequencedReliable(_)
            | ChannelMode::OrderedReliable(_) => Self::Normal,
            ChannelMode::UnorderedUnreliable
            | ChannelMode::SequencedUnreliable => Self::Low,
        }
    }

    /// Per-tick gain applied to each pending message on this channel.
    /// Used directly by the unified priority sort.
    pub const fn base_gain(&self) -> f32 {
        match self {
            Self::Low    => 0.5,
            Self::Normal => 1.0,
            Self::High   => 10.0,
        }
    }
}

pub struct ChannelSettings {
    pub mode: ChannelMode,
    pub direction: ChannelDirection,
    pub criticality: ChannelCriticality,  // NEW
}

impl ChannelSettings {
    /// Build with default criticality derived from `mode`.
    pub fn new(mode: ChannelMode, direction: ChannelDirection) -> Self {
        let criticality = ChannelCriticality::default_for(&mode);
        Self { mode, direction, criticality }
    }

    /// Override the default criticality.
    pub fn with_criticality(mut self, criticality: ChannelCriticality) -> Self {
        self.criticality = criticality;
        self
    }
}
```

**Protocol builder** — replace one-off `add_channel_with_X` accessors with a single `add_channel_settings` that accepts a fully-built `ChannelSettings`. Avoids combinatorial explosion as future channel fields accrue.

```rust
// shared/src/protocol.rs (extended)

impl Protocol {
    /// Unchanged — uses defaults for all `ChannelSettings` fields.
    pub fn add_channel<C: Channel>(
        &mut self,
        direction: ChannelDirection,
        mode: ChannelMode,
    ) -> &mut Self { /* internally calls add_channel_settings with ChannelSettings::new */ }

    /// Accepts a fully-configured `ChannelSettings` (builder-chain compatible).
    pub fn add_channel_settings<C: Channel>(
        &mut self,
        settings: ChannelSettings,
    ) -> &mut Self { /* ... */ }
}
```

**Usage:**

```rust
protocol
    .add_channel::<GameplayChannel>(
        ChannelDirection::Bidirectional,
        ChannelMode::UnorderedReliable(ReliableSettings::default()),
    )
    .add_channel_settings::<ChatChannel>(
        ChannelSettings::new(
            ChannelMode::UnorderedReliable(ReliableSettings::default()),
            ChannelDirection::Bidirectional,
        )
        .with_criticality(ChannelCriticality::Low),
    );
```

### III.3 · `EntityPriorityRef` / `EntityPriorityMut` — handle types

Shared between the global and per-user code paths. Both layers produce the same handle types; the difference is which state map the handle borrows from.

```rust
// shared/src/connection/entity_priority.rs (new file)

/// Read-only view of an entity's priority state in either the global (sender-wide)
/// or per-user priority layer. Acquired via the corresponding `*_priority()` method
/// on `WorldServer`, `Client`, or their Bevy-adapter equivalents.
pub struct EntityPriorityRef<'a, E: Copy + Eq + Hash> {
    pub(crate) state: Option<&'a EntityPriorityData>,  // None = no entry yet
    pub(crate) entity: E,
}

impl<'a, E: Copy + Eq + Hash> EntityPriorityRef<'a, E> {
    pub fn entity(&self) -> E { self.entity }

    /// Current accumulated priority value. Higher = more urgent.
    /// Returns 0.0 if this entity has no accumulator entry yet.
    pub fn accumulated(&self) -> f32 {
        self.state.map(|s| s.accumulated).unwrap_or(0.0)
    }

    /// Current per-tick gain override, if one is set.
    /// `None` means the default gain (1.0) applies.
    pub fn gain(&self) -> Option<f32> {
        self.state.and_then(|s| s.gain_override)
    }

    pub fn is_overridden(&self) -> bool {
        self.gain().is_some()
    }
}

/// Mutable handle for reading and setting an entity's priority in one priority layer
/// (global OR per-user). Lazy-creates a state entry on first write so set-and-forget
/// works even before the entity enters scope for that user.
pub struct EntityPriorityMut<'a, E: Copy + Eq + Hash> {
    pub(crate) entries: &'a mut HashMap<E, EntityPriorityData>,
    pub(crate) entity: E,
}

impl<'a, E: Copy + Eq + Hash> EntityPriorityMut<'a, E> {
    // --- Reads (mirror Ref) ---
    pub fn entity(&self) -> E { self.entity }
    pub fn accumulated(&self) -> f32 { /* ... */ }
    pub fn gain(&self) -> Option<f32> { /* ... */ }
    pub fn is_overridden(&self) -> bool { /* ... */ }

    // --- Writes ---

    /// Set a persistent per-tick gain for this layer. Stays in effect until
    /// `reset()` or another `set_gain()` call. Lazy-creates the entry.
    /// Use for set-and-forget cases ("this unit matters more").
    pub fn set_gain(&mut self, gain: f32) -> &mut Self { /* ... */ }

    /// One-shot additive boost to the accumulator. Does not change gain.
    /// Multiple calls in one tick sum additively. Lazy-creates the entry.
    /// Persists across ticks until the entity is sent (then reset to 0 per D12).
    /// Use for transient events ("this unit just fired, bump once").
    pub fn boost_once(&mut self, amount: f32) -> &mut Self { /* ... */ }

    /// Clear the gain override — return to default (1.0). Does NOT clear the
    /// accumulator value itself, and does NOT remove the entry (it keeps
    /// accumulating at default gain going forward).
    pub fn reset(&mut self) -> &mut Self { /* ... */ }
}
```

### III.4 · Server public API

```rust
// server/src/server/world_server.rs (extended)

impl<E: Copy + Eq + Hash + Send + Sync> WorldServer<E> {
    // --- Global priority layer (sender-wide, entity-lifetime) ---

    /// Read-only global priority view for `entity`.
    /// Global state persists for the entity's lifetime; evicted on despawn.
    pub fn global_entity_priority(&self, entity: &E) -> EntityPriorityRef<'_, E>;

    /// Mutable global priority handle for `entity`.
    pub fn global_entity_priority_mut(&mut self, entity: &E) -> EntityPriorityMut<'_, E>;

    // --- Per-user priority layer (per-connection, evicted on scope exit) ---

    /// Read-only per-user priority view for `entity` as seen by the connection `user_key`.
    /// Combined with the global layer at sort time: `effective = global × user`.
    pub fn user_entity_priority(
        &self,
        user_key: &UserKey,
        entity: &E,
    ) -> EntityPriorityRef<'_, E>;

    /// Mutable per-user priority handle. Evicted on scope exit.
    pub fn user_entity_priority_mut(
        &mut self,
        user_key: &UserKey,
        entity: &E,
    ) -> EntityPriorityMut<'_, E>;
}
```

### III.5 · Client public API

A client has one connection, so the global/per-user split collapses. Only one handle pair.

```rust
// client/src/client.rs (extended)

impl<E: Copy + Eq + Hash + Send + Sync> Client<E> {
    /// Read-only priority view for `entity` on the client's outbound path.
    pub fn entity_priority(&self, entity: &E) -> EntityPriorityRef<'_, E>;

    /// Mutable priority handle for `entity` on the client's outbound path.
    pub fn entity_priority_mut(&mut self, entity: &E) -> EntityPriorityMut<'_, E>;
}
```

### III.6 · Bevy adapter passthrough

Thin passthroughs — same pattern as `user_scope_mut` today. No new Bevy resources, plugins, or ECS components.

```rust
// adapters/bevy/server/src/server.rs (extended)

impl Server<'_, '_> {
    pub fn global_entity_priority(&self, entity: &Entity) -> EntityPriorityRef<'_, Entity>;
    pub fn global_entity_priority_mut(&mut self, entity: &Entity) -> EntityPriorityMut<'_, Entity>;
    pub fn user_entity_priority(&self, user_key: &UserKey, entity: &Entity)
        -> EntityPriorityRef<'_, Entity>;
    pub fn user_entity_priority_mut(&mut self, user_key: &UserKey, entity: &Entity)
        -> EntityPriorityMut<'_, Entity>;
}

// adapters/bevy/client/src/client.rs (extended)

impl Client<'_, '_> {
    pub fn entity_priority(&self, entity: &Entity) -> EntityPriorityRef<'_, Entity>;
    pub fn entity_priority_mut(&mut self, entity: &Entity) -> EntityPriorityMut<'_, Entity>;
}
```

### III.7 · Internal types

```rust
// shared/src/connection/bandwidth_accumulator.rs (internal)
pub(crate) struct BandwidthAccumulator {
    budget_bytes: f64,
    target_bytes_per_sec: f64,
    last_accumulate: Instant,    // naia_socket_shared::Instant
}

impl BandwidthAccumulator {
    pub(crate) fn new(config: &BandwidthConfig) -> Self;
    pub(crate) fn accumulate(&mut self, now: &Instant);
    pub(crate) fn can_spend(&self, estimated_bytes: u32) -> bool;
    pub(crate) fn spend(&mut self, actual_bytes: u32);
    pub(crate) fn remaining(&self) -> f64;
}
```

```rust
// shared/src/connection/priority_state.rs (internal)

/// Stored per-entity-bundle accumulator state. Shared structure for both
/// the global and per-user priority layers.
pub(crate) struct EntityPriorityData {
    pub(crate) accumulated: f32,
    pub(crate) gain_override: Option<f32>,
    pub(crate) last_sent_tick: Option<u32>,   // sender's game tick; telemetry only
}

/// Sender-wide priority layer (lives on WorldServer).
/// Entries evicted on entity despawn.
pub(crate) struct GlobalPriorityState<E: Copy + Eq + Hash> {
    entries: HashMap<E, EntityPriorityData>,
}

/// Per-connection priority layer (lives on each user Connection).
/// Entries evicted on scope exit for that user.
/// Client has exactly one of these (no separate global layer).
pub(crate) struct UserPriorityState<E: Copy + Eq + Hash> {
    entries: HashMap<E, EntityPriorityData>,
}

impl<E: Copy + Eq + Hash> GlobalPriorityState<E> { /* tick_accumulate, on_despawn, ... */ }
impl<E: Copy + Eq + Hash> UserPriorityState<E>   { /* tick_accumulate, on_scope_exit, ... */ }
```

**Canonical accumulator rules** (the contract implementation must satisfy):

1. **Per-tick accumulation, entity bundles.** For every in-scope dirty entity bundle for a given connection:
   `accumulated += effective_gain`, where
   `effective_gain = global.gain_override.unwrap_or(1.0) × user.gain_override.unwrap_or(1.0)`.
2. **Per-tick "accumulation", messages.** Each pending message's accumulator is computed on-the-fly at sort time as
   `(current_tick − enqueue_tick) × channel.base_gain()`.
   No per-message stored state — the channel's queue already tracks `enqueue_tick`.
3. **Unified candidate stream.** Build a k-way merge across:
   - one sorted stream of entity-bundle candidates (descending by `accumulated`), and
   - one FIFO head per channel (on-the-fly gain per D10; oldest message is always the channel's head).
4. **Fill packets until budget exhausted.** Walk the merged descending stream; for each candidate: if `BandwidthAccumulator::can_spend(est)`, serialize and `spend(actual)`; if not, exit the tick's send cycle.
5. **Reset on send (per D12).**
   - Entity bundle that fit: `accumulated = 0`, `last_sent_tick = current_tick`.
   - Entity bundle skipped: untouched.
   - Message that fit: channel's send bookkeeping advances; the next tick recomputes `age` from the *new* head's `enqueue_tick`.
   - Message skipped: untouched.
6. **`boost_once(a)`:** `accumulated += a` on write. Not reset until the entity is sent.
7. **Read handles on missing entries:** `accumulated = 0.0`, `gain = None`.
8. **Entry lifecycle:**
   - `GlobalPriorityState` entry created on first `global_entity_priority_mut` write OR first in-scope dirty tick; evicted on entity despawn.
   - `UserPriorityState` entry created on first `user_entity_priority_mut` write OR first in-scope dirty tick for that user; evicted on scope exit for that user. Entries never leak across user scope boundaries.

All internal types use `Instant` from `naia_socket_shared` (never `std::time::Instant`).

### III.8 · Surface-area summary

| Crate | New public items |
|---|---|
| `naia_shared` | `BandwidthConfig`, `ChannelCriticality`, `EntityPriorityRef`, `EntityPriorityMut`; fields on `ConnectionConfig` (`bandwidth`) and `ChannelSettings` (`criticality`) + `with_criticality` builder; `Protocol::add_channel_settings` |
| `naia_server` | `WorldServer::{global_entity_priority, global_entity_priority_mut, user_entity_priority, user_entity_priority_mut}`; re-exports of new shared types |
| `naia_client` | `Client::{entity_priority, entity_priority_mut}`; re-exports |
| `naia_bevy_server` | `Server::{global_entity_priority, global_entity_priority_mut, user_entity_priority, user_entity_priority_mut}` passthroughs; re-exports |
| `naia_bevy_client` | `Client::{entity_priority, entity_priority_mut}` passthroughs; re-exports |

No new Bevy resources, plugins, or ECS components. Matches existing Naia extension discipline.

---

## Part IV — Implementation phases

### Phase A — Bandwidth accumulator + unified priority sort (default gains)

**Goal:** cap outbound bytes-per-tick per connection; all candidate send items compete in a single k-way-merged priority sort using default gains. This alone is expected to close Phase 4.5.

**A.1 · Config + types**
- New files: `shared/src/connection/bandwidth.rs`, `shared/src/connection/bandwidth_accumulator.rs`, `shared/src/connection/priority_state.rs`, `shared/src/connection/entity_priority.rs`.
- Extend `ConnectionConfig` with `bandwidth: BandwidthConfig`.
- Extend `ChannelSettings` with `criticality` + `with_criticality`; add `ChannelCriticality::{default_for, base_gain}`.
- Add `Protocol::add_channel_settings<C>(ChannelSettings)`; `add_channel` delegates through it.

**A.2 · Unified send loop (symmetric in `shared`)**
Refactor `Connection::send_packets` (server) + client analog:
1. `BandwidthAccumulator::accumulate(now)`.
2. Build k-way merge candidate stream per III.7 rules 1–3:
   - Entity-bundle heap (dirty + in-scope, each with stored `accumulated` advanced this tick).
   - Per-channel FIFO heads (on-the-fly `age × base_gain`).
3. Walk the merged descending stream; for each candidate, estimate size, `can_spend`/`spend`, serialize, apply reset-on-send per III.7 rule 5.
4. Loop across packets within the tick until the accumulator is exhausted or the candidate stream empties.
- **Starvation guarantee:** anything skipped compounds next tick. High-criticality messages win against default entity gain after one tick of staleness; anything deferred eventually reaches parity.

**A.3 · Config plumbing**
- Server's internal Connection construction reads `ServerConfig.connection_config.bandwidth` → constructs `BandwidthAccumulator`.
- Client mirrors.
- Bevy adapters unchanged (config already flows through).

**A.4 · Telemetry (D13)**
- Always-on per connection: `bytes_sent_per_tick`, `budget_remaining_end_of_tick`, `oldest_unsent_age_ticks`.
- `#[cfg(feature = "bench_instrumentation")]`: `packets_deferred_due_to_budget`, top-N priority ring buffer, per-tick sort timing counters.

**A.5 · Tests** — see Part V.3 (`A-BDD-1` … `A-BDD-7`).

### Phase B — User-facing priority handles (global + per-user)

**Goal:** expose the two-layer priority knobs; wire `global × per_user` into effective gain at sort time.

**B.1 · State + types**
- `GlobalPriorityState<E>` lives on `WorldServer` (sender-wide; not per-connection).
- `UserPriorityState<E>` lives on each per-user `Connection`.
- Client gets a single `UserPriorityState<E>` on `Client` (no global layer).

**B.2 · Handle API**
- Implement `WorldServer::{global_entity_priority[_mut], user_entity_priority[_mut]}` per III.4.
- Implement `Client::{entity_priority[_mut]}` per III.5.
- Bevy passthroughs per III.6.

**B.3 · Wire gain into the sort**
- In A.2's candidate-stream construction, per-entity gain = `global.gain.unwrap_or(1.0) × user.gain.unwrap_or(1.0)`.
- `boost_once` additive to `accumulated` directly (per III.7 rule 6).

**B.4 · Lifecycle enforcement (D9)**
- `global_entity_priority_mut` entry evicted in `WorldServer::despawn_entity`.
- `user_entity_priority_mut` entry evicted in user-scope teardown (scope exit OR connection drop). Never leak across users.

**B.5 · Tests** — see Part V.3 (`B-BDD-1` … `B-BDD-10`).

### Phase C — Close-out

- Full gate suite (V.4).
- If `idle_distribution` matrix passes on all mutable cells: Phase 4.5 closed by absorption. Update tracker #94/#95, `BENCH_PERF_UPGRADE.md`.
- If the spike survives: fresh root-cause hunt against the new baseline. Do not block Phase 5 unless the cause is Phase-5-adjacent.
- Mark sidequest `✅ complete` across all three docs.
- Record lessons in `_AGENTS/BENCH_UPGRADE_LOG/sidequest-priority-accumulator.md`.

---

## Part V — Workflow + test plan

### V.1 · Workflow discipline

Per Connor's "API first, stubs with `todo!()`, BDD tests, then implement" directive:

1. **API design approval** ✅ (Part III, approved 2026-04-24).
2. **Stub all public APIs with `todo!()`.** `cargo check --workspace` green; runtime fails at `todo!()`s. Locks the API shape before logic.
3. **Preservation test audit.** Identify existing coverage of behaviors this refactor touches (ReliableSender, send_packets, WorldWriter, ChannelSettings). Where coverage is thin for an invariant we must preserve, **add tests before refactoring**.
4. **BDD specs for new behavior.** Written before implementation; each fails on the stubs.
5. **Implement one stub at a time, tests-driven.** Each stub replaced only when its BDD test compiles, then passes.
6. **Full gate suite** — `cargo test --workspace` + `namako gate` + `idle_distribution` + criterion — after every major step.

### V.2 · Preservation tests (existing behavior that must not break)

| Invariant | Existing coverage? | Action |
|---|---|---|
| Every reliable message eventually acked | namako specs + integration | Sufficient; re-run after refactor |
| `ReliableSender::collect_messages` respects RTT-factor | Phase 4 work | Sufficient |
| `WorldWriter::write_commands` CommandId monotonicity within a connection | Implicit via namako semi-ordered tests | **Add explicit unit test** pinning this before Phase A.2 lands |
| `IndexedMessageWriter` delta encoding round-trips | Unit tests | Verify; extend if weak |
| Scope machinery (`UserScopeMut::include/exclude`) unchanged | namako specs | Sufficient |
| `RemoteEntityChannel` spawn-barrier FSM | `sync/tests/*` | Sufficient — THE load-bearing test suite for reorder safety |
| `ChannelSettings::with_criticality` composes with existing `Protocol::add_channel` callers | Existing demo protocols | Verify no demo protocol breaks |
| Criterion bench matrix (Phase 4 numbers) | Established baseline | Re-run after each phase; ±5% |
| `idle_distribution` matrix `OK` on immutable cells | Established | Must stay `OK` throughout |

### V.3 · New BDD specs (written before implementation)

**Phase A — bandwidth + unified sort:**
- **A-BDD-1**: 10K queued reliable commands + 512 kbps budget → no tick exceeds `budget + one-packet-slack` bytes.
- **A-BDD-2**: Bandwidth-constrained send → 10K queue drains eventually (no starvation).
- **A-BDD-3**: High-criticality (TickBuffered) message + Low-criticality message both pending under tight budget → High wins this tick; Low compounds.
- **A-BDD-4**: Default `ConnectionConfig`, no volume → behavior indistinguishable from pre-accumulator (no false deferrals).
- **A-BDD-5**: Channel built with `.with_criticality(Low)` on a normally-Normal mode → gets Low `base_gain` in sort.
- **A-BDD-6**: Two pending messages of equal age, one on Low, one on Normal → Normal wins sort.
- **A-BDD-7**: Starvation torture — 1000 messages on a High channel + 1 message on a Low channel, tight budget → Low drains eventually; `oldest_unsent_age_ticks` for the Low message stays bounded.

**Phase B — priority handles:**
- **B-BDD-1**: Two in-scope dirty entities with default gain; packet can't hold both → one sent, other's accumulator carries to next tick.
- **B-BDD-2**: `global_entity_priority_mut(A).set_gain(10.0)` → A wins sort over default-gain B.
- **B-BDD-3**: Global=2.0, `user_entity_priority_mut(X, A).set_gain(5.0)` → A's effective gain for user X = 10.0.
- **B-BDD-4**: `set_gain(5.0)` then `reset()` → default (1.0) applied; `is_overridden()` returns false; entry still exists.
- **B-BDD-5**: `boost_once(100.0)` → accumulator bumps +100 immediately; reset to 0 on send; `gain()` unchanged.
- **B-BDD-6**: `set_gain(5.0)`, entity sent, next tick → `gain()` still `Some(5.0)` (persistence).
- **B-BDD-7**: 1000 stale in-scope dirty entities + tight budget → every entity reaches parity; `oldest_unsent_age_ticks` bounded.
- **B-BDD-8**: Cross-entity reorder at sender + spawn-barrier at receiver → per-entity CommandId monotonicity preserved; receiver's spawn-barrier FSM unchanged.
- **B-BDD-9**: `user_entity_priority_mut(X, A).set_gain(5.0)` then X's scope excludes A → X's entry evicted; Y's per-user state for A unaffected; global state for A unaffected.
- **B-BDD-10**: `global_entity_priority_mut(A).set_gain(5.0)` then despawn A → global entry evicted; no leak.

**Phase A + B combined:**
- **AB-BDD-1** (Phase 4.5 target): 10K spawn burst into `UnorderedReliable`; RTT-factor resend window fires → `idle_distribution` reports `OK` (max/p50 ≤ 10×) on the mutable cell previously spiking.

### V.4 · Gates (every PR, every phase)

- `cargo test --workspace` green.
- `namako gate` on all 22 feature specs.
- `idle_distribution` matrix: immutable cells stay `OK`; mutable cells pass after Phase C close-out.
- Criterion bench matrix: within ±5% of Phase 4 numbers at default bandwidth budget.

---

## Part VI — Risk register

| Risk | Likelihood | Severity | Mitigation |
|---|---|---|---|
| Starvation of any channel or entity under shared budget | Low (structural) | High | Unified sort + compound-and-retain on skip makes starvation structurally impossible. `A-BDD-7` + `B-BDD-7` assert empirical bounds |
| Cross-entity reorder violates per-entity CommandId monotonicity | Medium | Critical | Hard rule: reorder at the `send_message` boundary, never within an entity. Preservation test (V.2) pins invariant; `B-BDD-8` covers |
| Auth `subcommand_id` reorder breaks authority negotiation | Low | Critical | Per-entity intra-stream invariant non-negotiable; `sync/tests/*` catches |
| Budget set too low in prod → latency climbs | Medium | Medium | 512 kbps generous default; bandwidth-monitor telemetry visible; config knob |
| Priority-gain idle-grenade regression | Low | High | No inheritance-style boost in defaults (gains are multiplicative and explicit); top-N ring buffer under `bench_instrumentation` surfaces pathological tails |
| `IndexedMessageWriter` delta-encoding breaks under reorder | Negligible | High | Cross-entity reorder happens at the `send_message` drain boundary, *upstream* of `IndexedMessageWriter`; CommandId monotonicity preserved within every single entity's stream |
| Tick-Buffered inputs deferred past their intended tick | Low | High | `base_gain(High) = 10.0` + staleness compound; `A-BDD-3` covers. Worst case is a one-tick deferral |
| Accumulator state grows unbounded (long-running server) | Low | Medium | `GlobalPriorityState` keyed on alive entities (evicted on despawn); `UserPriorityState` evicted on scope exit |
| k-way merge sort cost under extreme fanout | Low | Low | Cost is O(items_emitted × log k); `items_emitted` is MTU-budget bounded; `k` ≤ #channels + 1 entity heap. Measured in sub-millisecond per tick at realistic configs |
| Global vs per-user handle confusion at call sites | Medium | Low | Symmetric naming (`global_entity_priority_mut` vs `user_entity_priority_mut`); doc headers on both; IDE auto-complete distinguishes |
| Phase 4.5 spike survives both phases | Low (per spike math) | Medium | Fresh root-cause hunt post-close-out; Phase 5 not blocked unless cause is Phase-5-adjacent |
| API surface changes break downstream (cyberlith, demos) | Low | Medium | All additions; no removals or renames; `add_channel` continues to work unchanged |

---

## Part VII — Reference material

- `_AGENTS/PRIORITY_ACCUMULATOR_SIDEQUEST.md` — scope and discipline.
- `_AGENTS/PRIORITY_ACCUMULATOR_RESEARCH.md` — Fiedler distillation, Halo: Reach, Overwatch, Unreal failure mode, bevy_replicon parallel proposal, Unity netcode for entities, Valve GameNetworkingSockets.
- `_AGENTS/BENCH_PERF_UPGRADE.md` — parent 7-phase plan.
- `_AGENTS/BENCH_UPGRADE_LOG/phase-04.md` — Phase 4 completion log + spike discovery.

---

## Part VIII — Sign-off gate

No code lands under this plan until:
1. ✅ Part III (API design) explicitly approved by Connor (2026-04-24).
2. Part V.2 preservation-test audit complete and any gaps closed.
3. Part V.3 BDD specs written (and failing against the stubs).
4. `cargo check --workspace` green with all `todo!()` stubs in place.

Only then does implementation begin.
