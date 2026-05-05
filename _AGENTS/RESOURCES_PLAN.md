# Replicated Resources — Spec & Implementation Plan

**Status:** DRAFT v2 for review (2026-05-05)
**Owner:** Connor + Claude (twin)
**Branch target:** `release-0.25.0-e`
**Prereqs landed:** `is_static` Remote tagging (5ac5b51b), static-entity ID pool, priority accumulator (Fiedler pacing), entity authority/delegation/migration
**Revision note:** v2 supersedes v1. Major changes per Connor feedback: dynamic-by-default with per-insertion static/dynamic choice; no new App ext trait (extends `AppRegisterComponentEvents`); single comprehensive `.feature` file (no 10-file proliferation); per-resource priority via existing entity-priority API (no new tier); user-facing `InsertResourceEvent` / `UpdateResourceEvent` / `RemoveResourceEvent` mirroring the component-event story so users see zero entity/component semantics; explicit treatment of the bevy-resource ↔ entity-component mirror challenge.

---

## 0. TL;DR

Add a first-class **Replicated Resource** primitive to Naia, modelled on Bevy `Resource`. A Resource:

- Is a `#[derive(Replicate)]` struct with `Property<T>` fields (identical syntax to a Component).
- Is a per-world singleton — at most one instance per type per `World`.
- Is server-authoritative by default with full opt-in client-authoritative + delegated-authority support (V1).
- Replicates with the same diff-tracked, per-field dirty-bit machinery as Components.
- Surfaces in the Bevy adapter as a normal `Res<R>` / `ResMut<R>`. Users never see an entity, component, or other internal artifact.

**Core architectural decision:** A Replicated Resource is internally a hidden 1-component entity. The entire feature falls out of the existing entity pipeline plus three additions: a `ResourceKinds` registry, a `TypeId↔Entity` map per side, and a per-tick **bevy↔entity mirror system** that bridges the two storage locations.

---

## 1. Reflection — Why 1-Component-Entity Wins

I went into this design assuming Resources would need a parallel pipeline. After reading the codebase end-to-end, that assumption was wrong.

### The reuse calculus

Naia's existing component-replication pipeline already gives us, **per (entity, component) pair**:

| Capability | Where it lives today |
|---|---|
| Per-field dirty-bit tracking | `Property<T>::DerefMut` → `PropertyMutator::mutate(idx)` → `DirtyQueue` (`shared/src/world/update/mut_channel.rs`) |
| Per-client diff masks for partial updates | `EntityUpdateManager` (`shared/src/world/update/entity_update_manager.rs`) |
| Wire framing for spawn / update / insert / remove / despawn | `EntityMessageType` + `WorldWriter` / `WorldReader` |
| Reliable retransmit until acked | `HostEngine` + ack window |
| Authority delegation (`enable_delegation`, `request_authority`, `release_authority`, migration, **disconnect-reclaim**) | `shared/src/world/sync/auth_channel*.rs`, `server/src/world/server_auth_handler.rs:141`, `server/src/server/world_server.rs:2195` |
| Bevy change-detection bridge (`Mut<C>` → `PropertyMutator`) | `adapters/bevy/server/src/plugin.rs` (`HostSyncChangeTracking`) |
| Static-vs-dynamic ID separation (per-spawn user choice) | `enable_replication` vs `enable_static_replication` (`adapters/bevy/server/src/commands.rs:13-46`) |
| **Per-entity priority gain** | `EntityPriorityMut::set_gain(f32)` / `boost_once(f32)` (`shared/src/connection/entity_priority.rs`) — composes per-connection × global multiplicatively (`world_server.rs:73-105`) |
| Multi-world | `release-0.25.0-b` work in progress |

**A Resource is just a Component on a singleton entity.** The new code is:

1. A `ResourceKinds` table marking which `ComponentKind`s are resources.
2. A `TypeId<R> ↔ GlobalEntity` registry per side per world.
3. Auto-scoping resource entities to all users (skip room/scope checks for resource entities).
4. A **bevy-resource ↔ entity-component mirror system** (one per replicated resource type, registered by the bevy adapter's `replicate_resource` extension).
5. User-facing event types (`InsertResourceEvent<R>`, `UpdateResourceEvent<R>`, `RemoveResourceEvent<R>`) that strip entity context out of the equivalent component events.
6. Surface API on `Server`, `Client`, `Commands`, `App`, and the `Protocol` builder.

That's it. Every other line of "new" Resource code is a thin re-projection of an entity operation.

### The mirror challenge (the only real architectural wrinkle)

The bevy `Resource` storage (`World.resources`) and the hidden naia entity component (`World.entities[hidden_entity]::<R>`) are **two different storage locations holding the same logical value**. Mutations to one do not propagate to the other automatically. We must bridge them.

**Constraint:** the user must see standard `Res<R>` / `ResMut<R>` (Connor: "Naia should try to hide networking details behind Bevy primitives as much as is reasonable" + "users should see zero entity/component semantics"). So the bevy-resource side is the user-facing surface; the hidden entity component is the wire-replication surface.

**Mirror mechanism (per replicated resource type R, one sync system added by `app.replicate_resource::<R>()`):**

```
Server-side outgoing path (user → wire):
  user does *res_mut.field = v                      via ResMut<R>
  → Property<T>::DerefMut on the bevy-resource side fires its mutator
  → mutator records (R, field_idx) into a per-resource SyncDirtyTracker
  → sync system later this tick:
       for each (R, field_idx) in tracker:
         clone field value from bevy-resource → call Property::set on the entity-component
       (Property::set on the entity side fires the entity-component's mutator
        → DirtyQueue → normal outgoing replication path)

Server-side incoming path (network → user, only for delegated resources held by client):
  incoming UpdateComponent message applied to entity-component (via existing pipeline)
  → entity-component fields updated via Property::set (using bypass-style write that does
    NOT re-trigger the mutator, mirroring the existing remote-apply path)
  → adapter event hook fires: copy fields from entity-component → bevy-resource
    using bevy's bypass_change_detection on the bevy side to avoid spurious change ticks,
    then explicitly set_changed() on the bevy-resource so user systems see the change

Client-side incoming path (network → user):
  same as server-side incoming path

Client-side outgoing path (user → wire, only after the client requested authority):
  same shape as server-side outgoing path; gated by entity-component's authority status
```

The sync mutator on the bevy-resource side is **a different `PropertyMutator` impl** from the entity-component's mutator — it just records dirty indices into a small per-type vec. Cost: O(dirty fields) per tick per replicated resource type. Negligible.

**Echo prevention** (delegated resources): the network-incoming side uses bypass-style writes on both the entity-component and the bevy-resource, so the sync mutator's dirty-tracker never sees those writes. Only user-driven `ResMut<R>` mutations enter the dirty-tracker → only user mutations replicate outward. This eliminates the loop.

### Honest tradeoffs

- **One `u16` ID per resource per server-run** (from whichever pool the user chose at insert time). Trivial.
- **Two storage locations** (bevy resource + entity component) — the mirror system bridges them. Cost analyzed above.
- **The hidden entity must not leak into user-visible events.** Mitigation: the bevy adapter's event-emission path filters entities present in `ResourceRegistry` out of `SpawnEntityEvent` / `DespawnEntityEvent` / `Insert/Update/RemoveComponentEvent` streams, and re-routes them as `Insert/Update/RemoveResourceEvent<R>` instead.
- **Schema evolution**: same story as Components today — `Replicate` derive's NetId-bound serialization breaks across protocol changes. No new problem.
- **Wire identification of resource entities**: implicit — receiver checks the incoming component kind against `protocol.resource_kinds.kind_set`. Zero wire overhead.

---

## 2. Goals & Non-Goals

### Goals (V1)

- A user can `#[derive(Replicate)]` a struct, register it via `protocol.add_resource::<R>()`, and have it transparently replicated server↔client per their authority configuration.
- Server-authoritative resources: server is the only writer; clients receive and observe.
- Client-authoritative + delegated resources: same migration, authority, and disconnect-reclaim semantics as entities, exposed via `commands.request_resource_authority::<R>()` / `commands.release_resource_authority::<R>()`.
- Bevy adapter: user accesses replicated resources via `Res<R>` / `ResMut<R>` — *zero* Naia-specific syntax in the user's day-to-day system code.
- Runtime insert/remove via Naia-aware Bevy `commands.replicate_resource(value)` / `commands.replicate_resource_static(value)` / `commands.remove_replicated_resource::<R>()` (mirroring the entity-spawn API which uses explicit Naia entry points like `enable_replication`).
- Diff-tracked partial updates (only mutated `Property<T>` fields go on the wire).
- Per-resource priority knob via the existing per-entity priority API; default gain 1.0 (no special tier).
- Per-spawn static-vs-dynamic choice (mirrors `enable_replication` vs `enable_static_replication`).
- Per-world isolation (multi-world support).
- User-facing `InsertResourceEvent<R>` / `UpdateResourceEvent<R>` / `RemoveResourceEvent<R>` on both sides; users never see the underlying entity.

### Non-goals (V1)

- Per-room or per-client scoping of resources. Resources are global.
- `NonSend` / `!Sync` resources. Replicated resources must be `Send + Sync`.
- `FromWorld` initialization (mirrors a local `World`; not meaningful across the wire).
- Resource hooks/observers (Bevy doesn't have them in stable either; defer until they do).
- A separate "Resource channel" — resources ride the existing entity-update channel.
- Replication of `Handle<T>` or other `Asset`-flavored values. Users serialize stable identifiers themselves.
- `Local<T>` / `NonSendMut<T>`-style local-only resources — out of scope per Connor.
- A new App extension trait — extend the existing `AppRegisterComponentEvents`.
- A new "Resource" priority tier — use the existing per-entity gain API.

---

## 3. Design Decisions (V1, locked-in)

| # | Decision | Rationale |
|---|---|---|
| D1 | **Resources are 1-component entities under the hood** | Maximum reuse of host/remote pipeline + authority + Bevy bridge. |
| D2 | Server-authoritative by default; opt-in delegation via `ReplicationConfig::delegated()` at insert time | Mirrors entity story exactly; no surprise. |
| D3 | Client-authoritative resources land in V1 | Authority/delegation machinery already exists; no reason to defer. |
| D4 | Global only — no per-room/per-client scoping | Matches Bevy's "one Resource per World" semantics. |
| D5 | Per-world (one set of resources per Naia `World`) | Matches multi-world in-flight work; matches Bevy semantics. |
| D6 | **Naia-specific Commands entry-point** for replication: `commands.replicate_resource(value)` / `commands.replicate_resource_static(value)`. Standard `commands.insert_resource(...)` does NOT replicate (purely local). | Mirrors the existing entity story: `commands.spawn(...)` doesn't replicate; user calls `enable_replication`. The Naia API is the explicit gate. |
| D7 | Reuse the diff-tracked update path; no new channel | Free correctness, free retransmit, free perf. |
| D8 | Bevy access via standard `Res<R>` / `ResMut<R>` | Networking is invisible to user code. Matches the Component story. |
| D9 | **Per-resource priority via existing entity-priority API**; default gain 1.0; user can call `server.resource_priority_mut::<R>().set_gain(f32)` to bump | No new priority tier. Consistent with Component/Channel/Message priority story. |
| D10 | Server-authoritative resources: read-only on client (no `ResMut<R>` write surface accepted) | Mirrors server-authoritative components. Attempted client writes return `AuthorityError::ServerHoldsAuthority`. |
| D11 | Client-authoritative resources after delegation: client `ResMut<R>` writes propagate via mirror → entity component → wire | Same as client-authoritative components after delegation. |
| D12 | **Per-insertion static-vs-dynamic choice** via `commands.replicate_resource(value)` (dynamic) vs `commands.replicate_resource_static(value)` (static); core API has matching `server.insert_resource(value)` / `server.insert_static_resource(value)` | Mirrors `enable_replication` / `enable_static_replication` exactly. No default — user picks. |
| D13 | **User-facing events**: `InsertResourceEvent<R>` / `UpdateResourceEvent<R>` / `RemoveResourceEvent<R>` on both sides; bevy-adapter event-emission path filters resource entities out of component-event streams and re-routes as resource events | Per Connor: users see ZERO entity/component semantics for resources. |
| D14 | Identifying a resource entity on the wire is implicit (component kind is marked as a resource kind in the protocol's `ResourceKinds` table) | Zero wire overhead — no marker component, no extra bits. |
| D15 | **Disconnect-with-authority** → revert to `EntityAuthStatus::Available` (next requester or server take_authority); resource persists with last-committed value | Mirrors entity behavior exactly (`server_auth_handler.rs:155-158`, `world_server.rs:2195-2220`). |
| D16 | **Server-attempt-to-mutate-delegated-resource-it-doesn't-own** → returns `AuthorityError::ClientHoldsAuthority`; value unchanged | Mirrors entity behavior. |
| D17 | **`replicate_resource()` on `App` does not exist as a separate trait method**. Resource event registration uses `add_resource_events::<R>()` added to the existing `AppRegisterComponentEvents` trait. Protocol-level registration of `R` as a resource happens via `protocol.add_resource::<R>()` inside a user `ProtocolPlugin` (mirroring how `protocol.add_component::<C>()` works). | Per Connor: don't proliferate trait imports. |

---

## 4. Architecture

### 4.1 Type registry

```rust
// shared/src/world/resource/resource_kinds.rs (NEW)
pub struct ResourceKinds {
    type_to_kind: HashMap<TypeId, ComponentKind>, // R::TypeId → underlying ComponentKind
    kind_set: HashSet<ComponentKind>,             // O(1) "is this kind a resource?"
}

impl ResourceKinds {
    pub fn register<R: Replicate + 'static>(&mut self, kind: ComponentKind) { /* … */ }
    pub fn is_resource(&self, kind: &ComponentKind) -> bool { /* … */ }
    pub fn kind_for<R: Replicate + 'static>(&self) -> Option<ComponentKind> { /* … */ }
}
```

Wired into `Protocol`:

```rust
// shared/src/protocol.rs (MODIFIED)
pub struct Protocol {
    pub component_kinds: ComponentKinds,
    pub message_kinds: MessageKinds,
    pub channel_kinds: ChannelKinds,
    pub resource_kinds: ResourceKinds, // NEW
    // ...
}

impl Protocol {
    pub fn add_resource<R: Replicate>(&mut self) -> &mut Self {
        let kind = self.component_kinds.add_component::<R>();
        self.resource_kinds.register::<R>(kind);
        self
    }
}
```

Users register inside a `ResourcesPlugin` that implements `ProtocolPlugin`, mirroring `ComponentsPlugin`:

```rust
// in user's shared protocol crate:
pub struct ResourcesPlugin;
impl ProtocolPlugin for ResourcesPlugin {
    fn build(&self, protocol: &mut Protocol) {
        protocol.add_resource::<Score>();
        protocol.add_resource::<MatchState>();
    }
}
```

### 4.2 Per-world resource registry

```rust
// shared/src/world/resource/resource_registry.rs (NEW)
pub struct ResourceRegistry {
    by_type: HashMap<TypeId, GlobalEntity>,
    by_entity: HashMap<GlobalEntity, TypeId>, // O(1) "is this entity a resource?"
}
```

- Host side: `HostWorldManager` owns one. `insert_resource::<R>(value)` allocates an entity (dynamic or static per the chosen API), inserts the registry entry, then routes through the existing `EntityCommand::SpawnWithComponents`.
- Remote side: `RemoteWorldManager` owns one. When `WorldReader` decodes a `SpawnWithComponents` and any component kind is in `protocol.resource_kinds.kind_set`, the registry records `(entity → TypeId)` and `(TypeId → entity)`.

### 4.3 Auto-scoping

Resource entities bypass room/scope checks. Implementation: in the server's scope resolver (`server/src/server/world_server.rs` scope-pending loop), entities present in `ResourceRegistry` are unconditionally included in every user's scope. No new `ScopeMode` variant — just an early-return in the resolution path.

### 4.4 Priority — uses existing per-entity API

No new priority tier. Resource entities default to gain 1.0 (the same as any other entity). Users tune priority via:

```rust
// Core (non-Bevy):
server.resource_priority_mut::<MyR>().set_gain(2.0);  // double its per-tick accumulator growth
server.resource_priority_mut::<MyR>().boost_once(50.0); // one-shot bump

// Bevy adapter wraps:
let mut server = ...; // Server SystemParam
server.resource_priority_mut::<MyR>().set_gain(2.0);
```

Internally these resolve to `EntityPriorityMut::set_gain` / `boost_once` on the resource's hidden entity. Both per-connection and global priority apply identically.

### 4.5 Bevy adapter — outgoing mirror (sender side)

`SyncDirtyTracker<R>` is a small per-resource-type, per-Bevy-app dirty-field tracker:

```rust
// adapters/bevy/shared/src/resource_sync.rs (NEW)
pub struct SyncDirtyTracker<R> {
    dirty_fields: Vec<u8>, // property indices
    _phantom: PhantomData<R>,
}

pub struct SyncMutator<R: Replicate + 'static> {
    tracker: Arc<Mutex<SyncDirtyTracker<R>>>, // shared with sync system
}

impl<R: Replicate + 'static> PropertyMutator for SyncMutator<R> {
    fn mutate(&mut self, property_index: u8) -> bool {
        self.tracker.lock().dirty_fields.push(property_index);
        true
    }
}
```

The bevy-resource side of `R` has its `Property<T>` mutators set to `SyncMutator<R>` (replacing the default). The entity-component side has the normal entity-component mutator (already wired by the existing host pipeline).

Per-tick sync system (added by `app.replicate_resource::<R>()` on the server adapter):

```rust
fn sync_resource_outgoing_R(
    mut server: Server,
    bevy_res: Option<ResMut<R>>,
    tracker: Res<SyncDirtyTracker<R>>,
) {
    let Some(bevy_res) = bevy_res else { return };
    let mut dirty = tracker.dirty_fields.lock().drain(..).collect::<Vec<_>>();
    if dirty.is_empty() { return; }
    dirty.sort_unstable();
    dirty.dedup();
    let entity = server.resource_entity::<R>().expect("resource registered");
    server.copy_dirty_fields_to_entity_component::<R>(entity, &bevy_res, &dirty);
    // copy_dirty_fields_to_entity_component calls Property::set on the entity-component
    // which fires the entity-component's mutator → DirtyQueue → normal outgoing path
}
```

### 4.6 Bevy adapter — incoming mirror (receiver side)

When the entity-component receives an update from the wire (via the existing remote-apply path), the bevy adapter's incoming-event hook copies the changed fields into the bevy `Resource`:

```rust
fn sync_resource_incoming_R(
    mut bevy_res_world: ResMut<R> /* bypassing change detection initially */,
    mut server_or_client: ServerOrClient,
    mut events: EventReader<InternalResourceUpdateBridge<R>>,
) {
    for event in events.read() {
        let entity = event.entity;
        let entity_component = server_or_client.entity_component::<R>(entity);
        // Use bypass_change_detection for the field copy to avoid double-marking
        let res_ref = bevy_res_world.bypass_change_detection();
        copy_changed_fields(entity_component, res_ref, &event.changed_field_indices);
        // Then explicitly mark changed so user systems see the update
        bevy_res_world.set_changed();
    }
    // The incoming-mirror writes do NOT touch SyncDirtyTracker → no echo loop
}
```

For initial insertion (`InsertComponentEvent` for a resource kind), the system inserts the bevy `Resource` via `commands.insert_resource(value)` (using `bypass_change_detection`-equivalent on the first insert path) and then immediately fires `InsertResourceEvent<R>` to the user.

For removal (`RemoveComponentEvent` for a resource kind), the system calls `commands.remove_resource::<R>()` and fires `RemoveResourceEvent<R>`.

### 4.7 Event filtering

The bevy adapter's component-event emission path (in `adapters/bevy/{server,client}/src/events.rs`) is augmented to:

1. Check whether the entity is in `ResourceRegistry`.
2. If yes: route the event to the resource-event stream (`InsertResourceEvent<R>` / `UpdateResourceEvent<R>` / `RemoveResourceEvent<R>`) and DROP the original component-event for that entity.
3. If no: emit normally.

`SpawnEntityEvent` / `DespawnEntityEvent` for resource entities are unconditionally suppressed (resources don't have an "entity" identity from the user's perspective).

### 4.8 User-visible event types

```rust
// adapters/bevy/server/src/events.rs (additions)
#[derive(bevy_ecs::message::Message)]
pub struct InsertResourceEvent<R: Replicate> {
    pub user_key: UserKey,
    _phantom: PhantomData<R>,
}

#[derive(bevy_ecs::message::Message)]
pub struct UpdateResourceEvent<R: Replicate> {
    pub user_key: UserKey,
    _phantom: PhantomData<R>,
}

#[derive(bevy_ecs::message::Message)]
pub struct RemoveResourceEvent<R: Replicate> {
    pub user_key: UserKey,
    pub resource: R,
}

// adapters/bevy/client/src/events.rs (additions)
#[derive(bevy_ecs::message::Message)]
pub struct InsertResourceEvent<T: Send + Sync + 'static, R: Replicate> {
    _phantom_t: PhantomData<T>,
    _phantom_r: PhantomData<R>,
}

#[derive(bevy_ecs::message::Message)]
pub struct UpdateResourceEvent<T: Send + Sync + 'static, R: Replicate> {
    pub tick: Tick,
    _phantom_t: PhantomData<T>,
    _phantom_r: PhantomData<R>,
}

#[derive(bevy_ecs::message::Message)]
pub struct RemoveResourceEvent<T: Send + Sync + 'static, R: Replicate> {
    _phantom_t: PhantomData<T>,
    pub resource: R,
}
```

Note the deliberate parallel to `InsertComponentEvent<C>` / etc. **No `entity` field** on any resource event — that's the user-visibility contract.

### 4.9 App-extension surface (no new trait)

```rust
// adapters/bevy/server/src/app_ext.rs (EXTENDED — no new trait)
pub trait AppRegisterComponentEvents {
    fn add_component_events<C: Replicate>(&mut self) -> &mut Self;
    fn add_bundle_events<B: ReplicateBundle>(&mut self) -> &mut Self;

    /// Register R as a replicated resource: enables InsertResourceEvent<R> /
    /// UpdateResourceEvent<R> / RemoveResourceEvent<R> message types and
    /// installs the per-resource sync systems (outgoing + incoming mirrors).
    /// The protocol must also register R via `protocol.add_resource::<R>()`
    /// in the shared ProtocolPlugin.
    fn add_resource_events<R: Replicate + Resource>(&mut self) -> &mut Self;  // NEW
}

// adapters/bevy/client/src/app_ext.rs — symmetric, with <T> generic
pub trait AppRegisterComponentEvents {
    fn add_component_events<T, C>(&mut self) -> &mut Self where T: Send + Sync + 'static, C: Replicate;
    fn add_bundle_events<T, B>(&mut self) -> &mut Self where T: Send + Sync + 'static, B: ReplicateBundle;
    fn add_resource_events<T, R>(&mut self) -> &mut Self where T: Send + Sync + 'static, R: Replicate + Resource; // NEW
}
```

### 4.10 Commands extension surface

```rust
// adapters/bevy/server/src/commands.rs (EXTENDED on Commands, not EntityCommands)
pub trait ServerCommandsExt {
    fn replicate_resource<R: Replicate + Resource>(&mut self, server: &mut Server, value: R);
    fn replicate_resource_static<R: Replicate + Resource>(&mut self, server: &mut Server, value: R);
    fn remove_replicated_resource<R: Replicate + Resource>(&mut self, server: &mut Server);
    fn configure_replicated_resource<R: Replicate + Resource>(
        &mut self,
        server: &mut Server,
        config: ReplicationConfig,
    );
}

// adapters/bevy/client/src/commands.rs (EXTENDED — for client-side delegated authority)
pub trait ClientCommandsExt {
    fn request_resource_authority<R: Replicate + Resource>(&mut self, client: &mut Client) -> Result<(), AuthorityError>;
    fn release_resource_authority<R: Replicate + Resource>(&mut self, client: &mut Client);
    fn resource_authority<R: Replicate + Resource>(&self, client: &Client) -> Option<EntityAuthStatus>;
}
```

### 4.11 Surface API summary (combined view)

```rust
// User code, shared protocol (one ProtocolPlugin like ComponentsPlugin):
pub struct ResourcesPlugin;
impl ProtocolPlugin for ResourcesPlugin {
    fn build(&self, protocol: &mut Protocol) {
        protocol.add_resource::<Score>();
        protocol.add_resource::<MatchState>();
    }
}

// User code, server App:
App::default()
    .add_plugins(ServerPlugin::new(server_config, protocol()))
    .add_component_events::<Position>()
    .add_resource_events::<Score>()             // single trait, no new import
    .add_resource_events::<MatchState>()
    .add_systems(Startup, init)
    .add_systems(Update, my_systems)
    .run();

// User code, server init system:
fn init(mut commands: Commands, mut server: Server) {
    commands.replicate_resource(&mut server, Score::new(0, 0));        // dynamic
    commands.replicate_resource_static(&mut server, MatchState::lobby()); // static
    commands.configure_replicated_resource::<Score>(&mut server, ReplicationConfig::delegated());
}

// User code, server gameplay system (no Naia syntax visible):
fn award_point(mut score: ResMut<Score>) {
    *score.home += 1; // standard Bevy ResMut + Property DerefMut → replication fires
}

// User code, client:
App::default()
    .add_plugins(ClientPlugin::<Main>::new(client_config, protocol()))
    .add_component_events::<Main, Position>()
    .add_resource_events::<Main, Score>()
    .add_resource_events::<Main, MatchState>()
    .add_systems(Update, (read_score, on_score_update));

fn read_score(score: Option<Res<Score>>) {
    let Some(score) = score else { return };
    println!("Score: {}-{}", *score.home, *score.away);
}

fn on_score_update(mut events: EventReader<UpdateResourceEvent<Main, Score>>, score: Res<Score>) {
    for _ in events.read() {
        ui_show_score(*score.home, *score.away);
    }
}

// Client requests authority on a delegated resource:
fn try_take_score_auth(mut commands: Commands, mut client: Client<Main>) {
    commands.request_resource_authority::<Score>(&mut client).ok();
}
```

---

## 5. Wire Format

**No new wire types.** Resource entities ride the existing `EntityMessageType::SpawnWithComponents` / `Update` / `Despawn` / authority-channel messages. The receiver-side "this is a resource" decision is local — driven by checking the incoming component kind against `protocol.resource_kinds.kind_set`.

Resource entities allocated via `commands.replicate_resource_static(...)` use the `static_generator` ID pool (`is_static=1` on the wire). Resource entities allocated via `commands.replicate_resource(...)` use the `generator` (dynamic) pool (`is_static=0`). Both work — the choice is the user's, mirroring entity spawn semantics.

---

## 6. Spec-Driven Development Plan

We follow the existing SDD flow (see `_AGENTS/SYSTEM.md`): write the comprehensive Gherkin spec → add step bindings → all tests RED → implement until GREEN.

### 6.1 Spec file (single, comprehensive)

```
test/specs/features/replicated_resources.feature
```

This single file covers the entire Replicated Resources domain: registration, insert/remove (dynamic + static variants), per-field diff updates, initial sync on connect, authority delegation (request, hold, mutate, release, server-rejection-when-delegated), disconnect-with-authority reclamation, per-resource priority, multi-world isolation, runtime lifecycle, Bevy adapter ergonomics. The full Gherkin is in §10 Appendix.

### 6.2 Step bindings

```
test/tests/src/steps/replicated_resources.rs
```

Step bindings map Gherkin → operations on the existing test harness `Scenario` plus new ctx methods.

### 6.3 New harness ctx methods

```rust
// test/harness/src/harness/scenario.rs (additions)
impl<'a> ServerCtx<'a> {
    pub fn insert_resource<R: Replicate>(&mut self, resource: R);                      // dynamic
    pub fn insert_static_resource<R: Replicate>(&mut self, resource: R);               // static
    pub fn resource<R: Replicate>(&self) -> Option<&R>;
    pub fn resource_mut<R: Replicate>(&mut self) -> Option<ResourceMut<'_, R>>;
    pub fn remove_resource<R: Replicate>(&mut self);
    pub fn configure_resource<R: Replicate>(&mut self, config: ReplicationConfig);
    pub fn resource_priority_mut<R: Replicate>(&mut self) -> ResourcePriorityMut<'_, R>;
}

impl<'a> ClientCtx<'a> {
    pub fn resource<R: Replicate>(&self) -> Option<&R>;
    pub fn resource_mut<R: Replicate>(&mut self) -> Option<ResourceMut<'_, R>>; // succeeds only when client holds authority
    pub fn request_resource_authority<R: Replicate>(&mut self) -> Result<(), AuthorityError>;
    pub fn release_resource_authority<R: Replicate>(&mut self);
    pub fn resource_authority_status<R: Replicate>(&self) -> Option<EntityAuthStatus>;
}
```

### 6.4 Test layout (RED before implementation)

```
test/tests/src/steps/replicated_resources.rs   — step bindings
test/specs/features/replicated_resources.feature — Gherkin spec
shared/src/world/resource/tests/                — unit tests for ResourceKinds + ResourceRegistry
```

Gate command: `namako gate --adapter-cmd "cargo run --manifest-path test/npa/Cargo.toml --" --specs-dir test/specs`. Goal: this gate is RED at end of Phase R1, GREEN at end of Phase R8.

---

## 7. Implementation Phases

Land in order; each phase has its own RED → GREEN gate.

### Phase R1 — Spec freeze (no implementation)
- Write the full `replicated_resources.feature` (text in §10).
- Write step bindings — they compile but reference unimplemented core API → bindings fail at runtime.
- Add `ResourceKinds`, `ResourceRegistry` *type stubs* (compile-only, panic-on-use) so test-harness ctx methods compile.
- **Gate:** `namako lint` PASS, `namako gate` shows the expected scenario failures.
- **Deliverable:** complete failing spec + bindings committed.

### Phase R2 — Core: registry + protocol wiring
- `ResourceKinds` (real impl).
- `ResourceRegistry` (real impl, on both `HostWorldManager` and `RemoteWorldManager`).
- `Protocol::add_resource::<R>()`.
- `WorldReader` populates remote `ResourceRegistry` on `SpawnWithComponents` containing a resource kind.
- **Gate:** registration scenarios in `replicated_resources.feature` pass.

### Phase R3 — Core: insert / remove (both static + dynamic) + auto-scope
- `server.insert_resource::<R>(value)` (dynamic) + `server.insert_static_resource::<R>(value)` (static).
- `server.remove_resource::<R>()`.
- `client.resource::<R>()` lookup.
- Auto-scope resource entities into every user's scope (server-side scope-pending early-return).
- **Gate:** insert/remove/initial-sync/runtime-lifecycle scenarios pass.

### Phase R4 — Core: updates, priority API
- Verify `Property<T>` mutations on the resource entity flow through normally (should require zero new code beyond the registry plumbing).
- Implement `server.resource_priority_mut::<R>()` (thin wrapper over `EntityPriorityMut` indexed via the registry).
- **Gate:** update + priority scenarios pass.

### Phase R5 — Core: authority & delegation
- `server.configure_resource::<R>(ReplicationConfig::delegated())` propagates to the resource entity.
- `client.request_resource_authority::<R>()` / `release_resource_authority::<R>()` → underlying entity authority ops.
- Server-side mutation gate: server-authoritative resources reject client write attempts; delegated resources reject server writes once client holds authority (mirroring entity behavior).
- Disconnect-with-authority: covered by existing entity disconnect path (`world_server.rs:2195`) — verify via spec scenario.
- **Gate:** all authority scenarios pass.

### Phase R6 — Core: multi-world
- `ResourceRegistry` is per-`World`.
- Per-world isolation tested.
- **Gate:** multi-world scenarios pass.

### Phase R7 — Bevy adapter (server side)
- Extend `AppRegisterComponentEvents` with `add_resource_events::<R>()`.
- Extend `Commands` (via `ServerCommandsExt`) with `replicate_resource` / `replicate_resource_static` / `remove_replicated_resource` / `configure_replicated_resource`.
- Define `InsertResourceEvent<R>` / `UpdateResourceEvent<R>` / `RemoveResourceEvent<R>` Bevy `Message` types.
- Implement `SyncMutator<R>`, `SyncDirtyTracker<R>`, and the per-resource outgoing sync system.
- Implement event-emission filter: resource entities are suppressed from `SpawnEntityEvent` / `DespawnEntityEvent` / component events; equivalent resource events fire instead.
- Implement incoming sync (server-side, for delegated resources held by a client): network update → entity component → bevy resource (using `bypass_change_detection` then `set_changed`).
- **Gate:** all bevy-server scenarios pass.

### Phase R8 — Bevy adapter (client side)
- Extend `AppRegisterComponentEvents` with `add_resource_events::<T, R>()`.
- Extend `Commands` (via `ClientCommandsExt`) with `request_resource_authority` / `release_resource_authority` / `resource_authority`.
- Define client-side event types (parameterized by `T`).
- Implement client-side sync system (network → bevy resource).
- Implement client-side outgoing sync (only fires when client holds authority).
- **Gate:** all `replicated_resources.feature` scenarios GREEN under both server-only and client-included harness; full `cargo test --workspace` green; `cargo check -p naia-bevy-client --target wasm32-unknown-unknown` clean.

### Phase R9 — Polish
- Documentation: `_AGENTS/RESOURCES.md` user-facing guide; update `demos/bevy/{shared,server,client}` adding a sample replicated resource.
- Capacity check: confirm a few hundred resources don't degrade entity replication numbers (run existing 31/0/0 benches).
- **Gate:** docs reviewed, demo runs, benches still 31/0/0.

---

## 8. Risk Register

| Risk | Mitigation |
|---|---|
| Bevy resource ↔ entity component mirror infinite-loops | Mirror writes use `bypass_change_detection` on the Bevy side and a "from-network" gate on the entity-component side. `SyncDirtyTracker` is only touched by user-driven `ResMut<R>` mutations. |
| Hidden entity leaks into `EventReader<SpawnEntityEvent>` etc. | Phase R7/R8: event-emission filter suppresses entities present in `ResourceRegistry`. |
| `add_resource_events` registered without matching `protocol.add_resource` | Document explicitly; consider runtime panic with helpful message at first sync attempt. |
| `commands.replicate_resource` called twice for the same `R` | Second call returns/panics with `ResourceAlreadyExists` (mirrors `commands.entity(e).insert(c)` re-insert behavior on a singleton). Tested in spec. |
| Resource ID exhausted under pathological resource counts | Static or dynamic u16 pool = 65k resources, recycled at 60s TTL. Hard cap; document. |
| Removing-then-re-inserting a resource produces stale state on slow clients | Resource entity is despawned then re-spawned with a fresh ID; existing entity migration / despawn-ack handles transition. Tested in spec scenario. |
| User defines a Resource that isn't `Send + Sync` | Compile-time error from `Replicate` + `Resource` bound. |
| User mutates `ResMut<R>` for a server-authoritative resource on the client | Property mutator on the bevy-resource side fires, but the outgoing sync system gates on authority status — no entity-component update occurs. The local bevy `Res<R>` is then re-overwritten by the next incoming sync, restoring server-truth. Document this as "soft rejection" matching the per-Connor entity story; consider stronger gating in Phase R8 (e.g. `ResMut<R>` SystemParam returns Option that's None when server-authoritative on client side). |

---

## 9. Open Questions for Connor (post-v2)

Most v1 questions were resolved by Connor's feedback. Remaining items:

1. **Stronger client-side write gating for server-authoritative resources.** The "soft rejection" in the risk table is the simplest implementation. A stricter alternative: client-side `ResMut<R>` returns `None` (or compile-time-rejects) for server-authoritative resources. Stricter is safer but requires per-resource type-state plumbing in the SystemParam impl. Recommendation: ship soft rejection in V1, harden if it bites.

2. **Auth status SystemParam.** Should `ResMut<R>` itself check authority and behave differently, or is it a separate `client.resource_authority::<R>()` API that the user must call manually before mutating? Component story uses the latter (manual `entity.authority()` check). Recommendation: match Component story (manual check).

3. **`InsertResourceEvent<R>` semantics for client connecting to a pre-existing resource.** Should it fire on the client even though the resource was inserted long before the client's connection? I argue YES — from the client's perspective, this is the first time they see the resource, which is "insert from their POV." Mirror how `InsertComponentEvent<C>` fires on a client when an already-existing entity comes into scope. Recommendation: fire.

---

## 10. Appendix — Comprehensive Gherkin Spec

> Single file: `test/specs/features/replicated_resources.feature`. Reviewed and tightened during Phase R1 before commit.

```gherkin
Feature: Replicated Resources
  As a Naia user, I want first-class server-replicated singleton state
  (analogous to Bevy Resources) with per-field diff tracking, optional
  client authority delegation, and ergonomic Bevy-native access.

  Background:
    Given a Naia protocol with replicated resource type "Score"
    And replicated resource type "MatchState"
    And replicated resource type "PlayerSelection" configured as delegable

  # ---------------------------------------------------------------------------
  # Registration & basic insert/observe (dynamic + static)
  # ---------------------------------------------------------------------------

  Scenario: server inserts a dynamic resource and a connected client observes it
    Given a server and one connected client
    When the server inserts Score { home: 0, away: 0 } as a dynamic resource
    And one full replication round trip elapses
    Then the client's Score is present
    And the client's Score.home equals 0
    And the client's Score.away equals 0

  Scenario: server inserts a static resource and a connected client observes it
    Given a server and one connected client
    When the server inserts MatchState { phase: "lobby" } as a static resource
    And one full replication round trip elapses
    Then the client's MatchState is present
    And the client's MatchState.phase equals "lobby"
    And the wire ID for the MatchState resource entity has is_static set to true

  Scenario: client connects after the resource was already inserted
    Given a server with Score { home: 5, away: 2 } already inserted at startup
    When a client connects and the handshake completes
    Then the client's Score is present within the first replication packet
    And the client's Score.home equals 5
    And the client's InsertResourceEvent for Score fired exactly once

  Scenario: re-inserting an already-existing resource is rejected
    Given a server with Score already inserted
    When the server attempts to insert Score again
    Then the operation returns a ResourceAlreadyExists error
    And the existing Score value is unchanged

  # ---------------------------------------------------------------------------
  # Per-field diff updates
  # ---------------------------------------------------------------------------

  Scenario: single field update transmits only the dirty field
    Given a server with Score { home: 0, away: 0 } and one connected client
    And the initial replication round trip has elapsed
    When the server mutates Score.home to 3
    And one replication round trip elapses
    Then the client's Score.home equals 3
    And the client's Score.away equals 0
    And the most recent server-to-client packet contains exactly one Score field update bit set

  Scenario: multiple sequential field updates coalesce within a tick
    Given a server with Score { home: 0, away: 0 } and one connected client
    And the initial replication round trip has elapsed
    When the server mutates Score.home to 1, then 2, then 3 within the same tick
    And one tick elapses
    Then the most recent server-to-client packet contains exactly one Score.home update
    And the client's Score.home equals 3

  # ---------------------------------------------------------------------------
  # Removal and re-insertion
  # ---------------------------------------------------------------------------

  Scenario: server removes a resource and the client observes the removal
    Given a server with MatchState { phase: "lobby" } and one connected client
    And the initial replication round trip has elapsed
    When the server removes MatchState
    And one replication round trip elapses
    Then the client's MatchState is absent
    And the client's RemoveResourceEvent for MatchState fired exactly once

  Scenario: insert, remove, re-insert with different value
    Given a server with one connected client
    When the server inserts MatchState { phase: "lobby" } as static
    And one replication round trip elapses
    Then the client's MatchState.phase equals "lobby"

    When the server removes MatchState
    And one replication round trip elapses
    Then the client's MatchState is absent

    When the server inserts MatchState { phase: "match" } as static
    And one replication round trip elapses
    Then the client's MatchState.phase equals "match"

  # ---------------------------------------------------------------------------
  # Authority delegation (V1 client-authoritative)
  # ---------------------------------------------------------------------------

  Scenario: client requests authority on a delegable resource and receives it
    Given a server with delegable PlayerSelection { selected_id: 0 } and connected client "alice"
    And the initial replication round trip has elapsed
    When alice requests authority on PlayerSelection
    And one replication round trip elapses
    Then alice's authority status for PlayerSelection is "Granted"

  Scenario: client-held authority allows client mutation that propagates to server
    Given alice holds authority on PlayerSelection
    When alice mutates PlayerSelection.selected_id to 7
    And one replication round trip elapses
    Then the server's PlayerSelection.selected_id equals 7

  Scenario: server-side mutation rejected while client holds authority
    Given alice holds authority on PlayerSelection
    When the server attempts to mutate PlayerSelection.selected_id to 99
    Then the attempt returns AuthorityError::ClientHoldsAuthority
    And the value remains 0

  Scenario: client releases authority and server reclaims
    Given alice holds authority on PlayerSelection
    And alice has set selected_id to 5
    When alice releases authority on PlayerSelection
    And one replication round trip elapses
    Then the server-side authority status for PlayerSelection is "Available"
    And subsequent client mutations from alice are rejected with AuthorityError::ServerHoldsAuthority

  Scenario: client disconnects while holding authority — authority reverts to Available, value persists
    Given alice holds authority on PlayerSelection
    And alice has set selected_id to 5
    When alice disconnects ungracefully
    And the server's disconnect-detection elapses
    Then the server's authority status for PlayerSelection is "Available"
    And the resource value remains the last value alice committed (5)
    And the resource is not despawned

  # ---------------------------------------------------------------------------
  # Per-resource priority (existing entity-priority API)
  # ---------------------------------------------------------------------------

  Scenario: per-resource priority gain affects send ordering under bandwidth pressure
    Given a server with replicated resource Score and 5000 dynamic entities each with Position
    And the server has set the priority gain for Score to 10.0
    And one connected client with constrained outbound bandwidth of 8 KB/tick
    And the initial replication round trip has elapsed
    When the server mutates Score.home and Position on every entity in the same tick
    Then the next outbound packet contains the Score update before any Position update

  Scenario: default priority gain is 1.0
    Given a server with replicated resource Score
    Then the server's reported priority gain for Score is 1.0

  # ---------------------------------------------------------------------------
  # Multi-world isolation
  # ---------------------------------------------------------------------------

  Scenario: resources in different worlds do not bleed across
    Given a server with worlds "world_a" and "world_b" both registering Score
    When the server inserts Score { home: 1, away: 0 } in world_a
    And the server inserts Score { home: 100, away: 0 } in world_b
    Then world_a's Score.home equals 1
    And world_b's Score.home equals 100
    And mutating world_a's Score does not change world_b's Score

  # ---------------------------------------------------------------------------
  # Bevy adapter ergonomics — user sees ZERO entity/component semantics
  # ---------------------------------------------------------------------------

  Scenario: server-side standard Bevy ResMut mutation replicates
    Given a Bevy server App with `add_resource_events::<Score>()` registered
    And `commands.replicate_resource(&mut server, Score::new(0, 0))` has been called
    And one connected client
    And the initial replication round trip has elapsed
    When a server system runs `*res_mut.home = 10` via `ResMut<Score>`
    And one replication round trip elapses
    Then the client's `Res<Score>.home` equals 10

  Scenario: client-side resource appears as a standard Bevy Res
    Given a Bevy client App with `add_resource_events::<Main, Score>()` registered
    And the server has inserted Score { home: 5, away: 2 }
    When the client connects and the initial replication round trip elapses
    Then a client system reading `Res<Score>` sees home=5, away=2

  Scenario: user receives InsertResourceEvent / UpdateResourceEvent / RemoveResourceEvent — never SpawnEntityEvent
    Given a Bevy server App and connected Bevy client with Score replicated
    When the server inserts, mutates, then removes Score
    And replication completes
    Then the client received exactly one InsertResourceEvent<Main, Score>
    And the client received at least one UpdateResourceEvent<Main, Score>
    And the client received exactly one RemoveResourceEvent<Main, Score>
    And the client received zero SpawnEntityEvent<Main> attributable to Score
    And the client received zero DespawnEntityEvent<Main> attributable to Score
    And the client received zero InsertComponentEvent<Main, Score>

  Scenario: client requests authority via Commands extension (Bevy ergonomics)
    Given a Bevy server App with delegable PlayerSelection and connected Bevy client "alice"
    When alice's Bevy system runs `commands.request_resource_authority::<PlayerSelection>(&mut client)`
    And one replication round trip elapses
    Then alice's `commands.resource_authority::<PlayerSelection>(&client)` returns Some(Granted)
    And alice can mutate `ResMut<PlayerSelection>` and the change replicates to the server
```

---

## 11. Provenance & Open Edits

- v1 drafted 2026-05-05.
- v2 (this revision) 2026-05-05 after Connor feedback: dynamic-by-default with per-insertion choice; single trait extension (no proliferation); single comprehensive `.feature` file; sane priority defaults via existing entity-priority API; user-facing `Insert/Update/RemoveResourceEvent` so users see zero entity/component semantics; explicit treatment of mirror challenge.
- Pending Connor review on §9 remaining open questions.
- After review, this doc moves to `_AGENTS/ARCHIVE/` as phases complete (per the "don't re-audit completed plans" doctrine), with a per-phase COMPLETE marker added to the top.
