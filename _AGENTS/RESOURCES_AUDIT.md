# Replicated Resources — Post-Implementation Audit

**Date:** 2026-05-05
**Branch:** `release-0.25.0-e` (HEAD: `f2adffa8`)
**Author:** Claude (twin), self-audit of the R1–R9 + Mode B + D13 work
**Scope:** every line of code I added or touched for the Replicated Resources feature, plus a handful of broader naia patterns I had to engage with along the way.

This doc is honest about gaps in my own work. The Resources feature ships and works, but a few sharp edges, code smells, and outright duplications are tracked here so they don't ossify.

---

## Connor's verdict (recorded 2026-05-05)

| Item | Verdict | Notes |
|---|---|---|
| A1 client-side mirror | **FIX** | Implement |
| A2 mirror_single_field panic | **FIX** | debug_assert + safer release |
| A3 dead `resource_kinds` field | **FIX — delete entirely** | Don't preserve for hypothetical R5-receive flow |
| B1 sealed trait alias | **FIX** | Promote `ReplicatedResource` |
| B2 trait rename | **FIX — both options A AND B** | `CommandsExt` → `EntityCommandsExt`; `CommandsExtServer` → `ServerCommandsExt`; same on client |
| B3 `AuthorityError::ResourceNotPresent` | **FIX** | Add variant |
| B4 event-constructor asymmetry | **INVALID** | "There will always be a user_key context" — my asymmetry concern was wrong; the server-side constructor's `user_key` requirement is correct |
| B5 4× duplicated `resource(closure)` | **FIX** | Shared helper |
| B6 `Option<&UserKey>` wart | **LEAVE** | Fine as-is |
| **B7 over-clone in Mode A path** | **DECISION REQUIRED — see "Mode A vs Mode B" below; we picked Mode B exclusively** | Removes the conditional entirely |
| B8 `app.replicate_resource_at_startup` | **REJECTED** | API design follows Bevy standards; users use `Startup` system + `commands.replicate_resource(...)` |
| C1–C9 cleanup | **FIX ALL** | C8 = option A (remove unnecessary `mut`) |
| D1 mutex | **FIX — option a (parking_lot)** | Drop poisoning + lock-fairness overhead |
| D2 per-tick dispatcher | **FIX** | Single dispatcher system |
| D3 Commands deferral | **DOCUMENT ONLY** | Behavior is fine |
| F1–F5 test gaps | **FIX ALL** | Comprehensive coverage including Bevy-app integration tests |
| **Namako SDD** | **MUST HAVE** | Comprehensive `.feature` + step bindings + passing tests in the SDD pipeline |
| G1–G4 broader naia | Out of scope for this audit cycle | Tracked for future sprints |

---

## Mode A vs Mode B — clarified, and the chosen mode

I conflated these two terms during implementation in a way that isn't crisp. Here's the disambiguation Connor asked for:

### What I was calling "Mode A"

> User accesses `R` via `Query<&R>` over the hidden resource entity. The bevy-resource side is not used. `Res<R>` would not work (or would panic — no resource of that type registered).

This was effectively a **fallback for users who didn't call `add_resource_events::<R>()`** — a vestigial branch in `commands.rs` that early-returned from `install_bevy_resource_mirror_if_present` when the `SyncDirtyTracker<R>` wasn't present.

It's not really a mode — it's "what happens when the user partially registered a resource."

### What I was calling "Mode B"

> User accesses `R` via standard `Res<R>` / `ResMut<R>`. The bevy adapter maintains a bevy-side mirror that's kept in sync with the entity-component via the `SyncMutator<R>` + `mirror_single_field` machinery. Per-field diff preserved.

This is the **real spec contract** — the design doc has always said `Res<R>`/`ResMut<R>` is the user surface (D8 of the plan). Mode B IS Replicated Resources.

### Decision: Mode B exclusively

Per Connor: "we likely need to pick ONE mode and keep it that way."

**We pick Mode B.** Concretely:

1. `R: ReplicatedResource` (the new sealed trait alias from B1) requires **all three** bounds: `Replicate + Component<Mutability=Mutable> + bevy::Resource`. This is a hard requirement — no escape hatch.
2. `add_resource_events::<R>()` becomes the canonical registration entry point. Calling `commands.replicate_resource(value)` without first calling `add_resource_events::<R>()` is a **user error** — we panic with a clear message rather than silently fall back to a non-functional Mode A.
3. The bevy-resource side is ALWAYS inserted on `commands.replicate_resource(value)`. No conditional clone, no `install_bevy_resource_mirror_if_present` early-return.
4. The "Mode A" term is removed from all docs and code comments. There's just **the mode**: Replicated Resources surface as `Res<R>`/`ResMut<R>` with full per-field diff via the mirror system.
5. Users who want a Component-only singleton (the use case Mode A was nominally serving) can just `commands.spawn(R::new(...)).enable_replication(server)` directly — that's what entities already do. Resources are explicitly the `Res<R>`/`ResMut<R>` story.

This resolves B7 by making it irrelevant: the conditional clone path goes away entirely.

### Effect on the audit items

- **B7 → resolved by Mode B exclusivity.** No conditional clone, no Mode A path.
- **C5 (stale comment)** → resolved as part of removing all "Mode A" language.
- **C9 (box-and-downcast vestige)** → resolved by passing `R` typed (no need to type-erase since we always do the bevy-Resource insert).

---

## Severity legend

| Mark | Meaning |
|---|---|
| 🚨 | Correctness or contract gap. Should be fixed before users notice. |
| ⚠️ | API consistency / elegance issue. Visible in user code; will compound over time. |
| 🧹 | Code quality / dead code / duplication. Internal only; mostly cleanup. |
| 💡 | Refactor opportunity. Not a bug — would improve architecture. |
| 📚 | Documentation / discoverability gap. |
| 🔬 | Test coverage gap. Functionality works but isn't pinned by a test. |

---

## A. Critical gaps (Mode B is server-side-only)

### A1. 🚨 Client-side `ResourceRegistry` does not exist

The server side has `ResourceRegistry` populated at `insert_resource` time. The client side has **no equivalent**. This means:

- `Res<R>` on the **client** is never auto-populated by the bevy adapter — there's no client-side mirror system, only a server-side one.
- `commands.request_resource_authority::<T, R>()` falls back to a world-scan (`find_resource_entity`) every call to locate the entity carrying `R`. O(n entities). Documented as V1 limitation in the commit message but never followed up.
- Client-side `InsertResourceEvent<T, R>` translation gates on `Messages<InsertResourceEvent<T, R>>` presence (which works), but the bevy `Resource<R>` is never inserted into the client's World. So a user system writing `fn sys(score: Res<Score>) { ... }` on the client would panic with "resource does not exist."

**What's needed:**

1. A client-side equivalent of `WorldServer::resource_registry`, populated by the `WorldReader` / `RemoteWorldManager` when a `SpawnWithComponents` arrives whose component kind is in `protocol.resource_kinds`. Mirror the server's bidirectional map.
2. A client-side mirror system in `adapters/bevy/client/src/resource_sync.rs` (currently missing) that, on `InsertComponentEvent` for a registered resource type, also inserts the bevy `Resource`. On `RemoveComponentEvent`, removes it. On `UpdateComponentEvent`, mirrors the entity-component value into the bevy `Resource` (using `bypass_change_detection` to avoid echo through the SyncMutator chain — note that for the client, the SyncMutator is wired only when the client holds authority on a delegated resource, so echo prevention is even more important here).
3. The client-side authority-after-grant flow needs the client's bevy `Resource` to also have its `Property` mutators wired to a client-side `SyncDirtyTracker<R>`, so that `*resmut.field = v` on the client triggers replication-back-to-server. Mode B isn't symmetric until this lands.

**Severity:** 🚨 because the spec advertises `Res<R>` on both sides. Right now only the server side fully delivers.

---

### A2. 🚨 `mirror_single_field` panics on type mismatch

`shared/derive/src/replicate.rs:622-625`:
```rust
} else {
    panic!("cannot mirror_single_field: other Component is of another type!");
}
```

This is the same pattern as the existing `mirror`, but the SyncMutator path is more user-driven than mirror is — a downcast failure here would mean a logic bug in the bevy adapter's `install_bevy_resource_mirror_if_present` rather than user error. It should be a `debug_assert_eq!` + silent no-op in release, OR return `Result<(), MirrorError>`. A panic in a hot per-tick sync system is hostile.

**Fix:** match the existing `mirror`'s behavior for consistency (panic) but add `debug_assert!` so the failure mode is visible during development; OR introduce `try_mirror_single_field` that returns `Result`.

---

### A3. 🚨 `WorldServer::resource_kinds` field is dead

`server/src/server/world_server.rs:117` — added with `#[allow(dead_code)]` and documented as "reserved for R5 (delegation) where the server must identify incoming resource updates from clients." It's never read. The R5 delegation flow ended up working without it because the entity-component machinery auto-handles the client→server resource update (the entity is delegated; the existing sync handles it).

**Decision needed:** either delete the field entirely, OR wire it into the receive path to enforce "this client just sent a Spawn for an entity carrying a resource-kind component — refuse / log / handle specially." Right now it's vestigial.

---

## B. API consistency / elegance

### B1. ⚠️ The `R: Replicate + Component<Mutability=Mutable> + Resource` bound is repeated **24 times**

In `adapters/bevy/server/src/`:
- `commands.rs`: 19 occurrences
- `server.rs`: 9 occurrences (with various subset bounds)
- `resource_sync.rs`: 3 occurrences
- `app_ext.rs`: 2 occurrences

This is a maintenance hazard and visually noisy. Every method signature spans 1–2 extra lines just for the bound.

**Fix:**

```rust
// In naia-bevy-shared (or naia-bevy-server):
pub trait ReplicatedResource:
    Replicate
    + bevy_ecs::component::Component<Mutability = bevy_ecs::component::Mutable>
    + bevy_ecs::resource::Resource
{
}

impl<T> ReplicatedResource for T
where
    T: Replicate
        + bevy_ecs::component::Component<Mutability = bevy_ecs::component::Mutable>
        + bevy_ecs::resource::Resource
{
}
```

Then every `R: Replicate + Component<Mutability=Mutable> + Resource` becomes `R: ReplicatedResource`. Trivial change, immediate ergonomic + maintenance win.

The internal-only sites that need just `Replicate + Component<Mutability=Mutable>` (no `Resource` requirement, e.g. for Mode A access) can keep the longer form or use a separate `ReplicatedResourceComponent` alias.

A `ResourceBound` trait already exists at `commands.rs:136` — dead code I left, intended for exactly this purpose. The implementation should be promoted to the public surface.

---

### B2. ⚠️ Trait-name asymmetry: `CommandsExt` (entity) vs `CommandsExtServer` (world)

The server adapter exposes:
- `CommandsExt for EntityCommands<'_>` — `enable_replication`, `give_authority`, etc.
- `CommandsExtServer for Commands<'_, '_>` — `replicate_resource`, etc.

Bevy convention is `EntityCommandsExt` and `CommandsExt` (named by receiver). My `CommandsExtServer` reads as if it's "extending Commands for Server-specific things" which is semantically right but breaks the receiver-naming convention.

**Fix options:**
- (a) Rename existing `CommandsExt` → `EntityCommandsExt` (breaking change to existing users)
- (b) Rename my `CommandsExtServer` → `ServerCommandsExt` (still uses "Server" qualifier, but at the start matches Bevy's `*Ext` pattern)
- (c) Leave as-is and document the asymmetry

Recommend (b). Same on the client side: `CommandsExtClient` → `ClientCommandsExt`.

---

### B3. ⚠️ `AuthorityError::NotInScope` overloaded as "resource not found"

`server/src/server/server.rs:375, 388`:
```rust
let entity = self
    .world_server
    .resource_entity::<R>()
    .ok_or(AuthorityError::NotInScope)?;
```

`NotInScope` is a real authority error meaning "the entity isn't in this user's scope." Using it for "resource of type R is not currently inserted" is semantically wrong and will confuse anyone debugging. Same in client harness `request_resource_authority`.

**Fix:** add a variant to `AuthorityError`:
```rust
pub enum AuthorityError {
    NotDelegated,
    NotAvailable,
    NotHolder,
    NotInScope,
    ResourceNotPresent,  // NEW
}
```
Or wrap with a new `ResourceAuthError` enum that contains `AuthorityError` plus `ResourceNotPresent`.

---

### B4. ⚠️ `InsertResourceEvent::new()` has `Default` on client but not server

Server `events.rs`: `InsertResourceEvent<R>::new(user_key)` only.
Client `events.rs`: `InsertResourceEvent<T, R>::new()` AND `Default::default()`.

Asymmetric. Server should also have `Default` (when there's no user_key context, e.g. server-side observers).

Same for `UpdateResourceEvent`. `RemoveResourceEvent` is asymmetric for a real reason (carries `R` which isn't `Default`), but the `Insert`/`Update` ones should match.

---

### B5. ⚠️ Test harness `resource(closure)` method duplicated 4× verbatim

`server_mutate_ctx.rs:235`, `server_expect_ctx.rs:111`, `client_mutate_ctx.rs:134`, `client_expect_ctx.rs:83` — all four files have the same body:

```rust
pub fn resource<R, F, T>(&self, f: F) -> Option<T>
where R: ReplicatedComponent, F: FnOnce(&R) -> T,
{
    let world_ref = ...;
    for e in world_ref.entities() {
        if let Some(comp) = world_ref.component::<R>(&e) { return Some(f(&*comp)); }
    }
    None
}
```

Same on `has_resource`. **All four should call into a shared `naia_test_harness::harness::resource_lookup::resource_in_world<R>(world, f)` helper.** Currently each ctx struct has the same 12-line body inlined.

---

### B6. ⚠️ Server `resource_release_authority::<R>()` takes `None` for `origin_user`

`server.rs:387`:
```rust
self.world_server.entity_release_authority(None, &entity)
```

The `None` here means "no originating user" (server-initiated release). The wrapped method's signature is `entity_release_authority(origin_user: Option<&UserKey>, world_entity: &E)`. Passing `None` is correct semantically but the API smells: the server-side `resource_release_authority` doesn't take a user key at all, so the user has no choice but `None`. Either:
- (a) Remove the `Option` and split into `server_release_authority` and `client_release_authority` on the underlying API
- (b) Document why `None` is always passed

This isn't a Resources-introduced wart — it predates the resource work — but my code propagates it.

---

### B7. ⚠️ `commands.replicate_resource` clones via `copy_to_box` even when Mode B isn't registered

`adapters/bevy/server/src/commands.rs:230`:
```rust
let snapshot_for_bevy = clone_via_replicate::<R>(&value);
// ... insert via server ...
crate::resource_sync::install_bevy_resource_mirror_if_present::<R>(
    world, snapshot_for_bevy,
);
```

`install_bevy_resource_mirror_if_present` early-returns if `SyncDirtyTracker<R>` isn't present (Mode A only). But the `copy_to_box()` clone happened unconditionally before that check. For Mode A users, this is a wasted allocation per `replicate_resource` call.

**Fix:** check tracker presence first; only clone when Mode B is active.

```rust
let mode_b_active = world.get_resource::<SyncDirtyTracker<R>>().is_some();
let snapshot_for_bevy = if mode_b_active { Some(value.copy_to_box()) } else { None };
// ... insert via server ...
if let Some(snap) = snapshot_for_bevy {
    install_bevy_resource_mirror_if_present::<R>(world, snap);
}
```

---

### B8. ⚠️ No `app.replicate_resource_at_startup(value)` shortcut

Current pattern requires:
```rust
app.add_systems(Startup, |mut commands: Commands| {
    commands.replicate_resource(Score::new(0, 0));
});
```

A common idiom would be:
```rust
app.replicate_resource_at_startup(Score::new(0, 0));
```

Modeled on `app.insert_resource(...)` (which Bevy provides). Could be added to `AppRegisterComponentEvents` (as a convenience that bundles `add_resource_events` + a `Startup` system call).

---

## C. Code quality / dead code / duplication

### C1. 🧹 Dead `ResourceBound` trait in `commands.rs:136`

Defined and `impl`'d but never used anywhere. Was placeholder for the trait alias work in B1. Either promote to fix B1 or delete.

---

### C2. 🧹 `ResourceRegistry::pick_resource_kind` is unused

`shared/src/world/resource/resource_registry.rs:142-167` — added speculatively for the receiver-side resource detection, never called. The actual resource-kind detection happens via `protocol.resource_kinds.is_resource(&kind)` inline in the consumers.

---

### C3. 🧹 `despawn_resource_world_entity` helper is empty

`server/src/server/world_server.rs` — I added this helper during a refactor, then inlined the logic and never deleted the empty function. Currently:
```rust
fn despawn_resource_world_entity<W: WorldMutType<E>>(&mut self, world: &mut W) {
    let _ = world;
}
```

It's actually been removed in the latest revision (verified absent from current code). **Cancelled.** Leaving the audit entry as a record that I checked.

---

### C4. 🧹 `ResourceKinds::register::<R>` requires `R: Replicate` but doesn't use R

`shared/src/world/resource/resource_kinds.rs:46`:
```rust
pub fn register<R: Replicate>(&mut self, kind: ComponentKind) {
    self.kinds.insert(kind);
    self.type_ids.insert(TypeId::of::<R>());
}
```

`R: Replicate` is unnecessary — `TypeId::of::<R>()` only requires `R: 'static`. The `Replicate` bound is performative.

Same `R: Replicate` on `kind_for::<R>()`.

**Fix:** weaken to `R: 'static`. Internal callers all have `Replicate` anyway, so no API change needed.

---

### C5. 🧹 Stale doc comment in `commands.rs`

`adapters/bevy/server/src/commands.rs:208-217` (in `ReplicateResourceCommand::apply`):
```rust
// We only install the bevy-Resource side if R: bevy::Resource —
// detected by attempting to insert the SyncDirtyTracker. The R
// bound on this Command requires Replicate + Component; the
// additional bevy::Resource bound is enforced at registration
// time via `add_resource_events::<R>()` (Mode B path) but we
// don't require it here so users who want Mode A only (no bevy
// Resource, just Query<&R>) are still supported.
```

This comment is **now wrong** — after the bound widening, `ReplicateResourceCommand` DOES require `R: bevy::Resource`. The Mode A escape hatch described here doesn't exist anymore. Either remove the comment OR re-introduce the Mode A escape (more effort but more elegant — let users register resources without `#[derive(Resource)]` if they don't want Mode B).

---

### C6. 🧹 Inline `bevy_ecs::component::Component<Mutability = bevy_ecs::component::Mutable>` is ugly

Even after the trait alias fix (B1), the underlying bound is quite verbose. Suggestion: re-export `Mutable` from `naia_bevy_shared` so users can write `Component<Mutability = Mutable>` instead of the full path.

Already done in `commands.rs` (uses `Mutable` directly via import) but inconsistent — `server.rs` and `resource_sync.rs` use the full path.

---

### C7. 🧹 `clone_via_replicate` is a one-line wrapper

`adapters/bevy/server/src/commands.rs:262-264`:
```rust
fn clone_via_replicate<R: Replicate>(value: &R) -> Box<dyn Replicate> {
    value.copy_to_box()
}
```

Just inline `value.copy_to_box()` at the call site.

---

### C8. 🧹 `_ = &mut world_mut` borrow-checker scaffold in `resource_sync.rs`

```rust
let mut world_mut = world.proxy_mut();
let _ = &mut world_mut; // borrow-checker scaffold
let Some(mut entity_comp) = world_mut.component_mut::<R>(&entity) else { ... };
```

The `let _ = &mut world_mut;` line was added to silence "variable does not need to be mutable" — a code smell. Either:
- (a) Remove the `mut` from `world_mut` (verify it's actually unused)
- (b) Restructure so the mut is necessary and obvious

---

### C9. 🧹 `resource_sync.rs::install_bevy_resource_mirror_if_present` boxes a snapshot to dyn-Replicate then immediately downcasts back

```rust
pub(crate) fn install_bevy_resource_mirror_if_present<R>(
    world: &mut World,
    snapshot: Box<dyn Replicate>,
) where R: Replicate + Resource + ... {
    // ...
    let any_box = snapshot.to_boxed_any();
    let mut value: Box<R> = any_box.downcast::<R>().ok().map(...).unwrap_or_else(...)?;
    // ...
}
```

Caller has `R` typed; we box-and-downcast purely to thread through a function boundary. Could pass `R` directly:

```rust
pub(crate) fn install_bevy_resource_mirror_if_present<R>(world: &mut World, mut value: R)
where R: ReplicatedResource
{
    if world.get_resource::<SyncDirtyTracker<R>>().is_none() { return; }
    let tracker = world.get_resource::<SyncDirtyTracker<R>>().unwrap();
    wire_sync_mutator(&mut value, tracker);
    world.insert_resource(value);
}
```

Avoids the box+downcast dance entirely. The reason I went through `Box<dyn Replicate>` was a vestige from the earlier "Mode A escape hatch" design (C5).

---

## D. Performance / efficiency

### D1. 💡 `SyncDirtyTracker<R>` uses `std::sync::Mutex` for a single-thread access pattern

`adapters/bevy/server/src/resource_sync.rs:81`:
```rust
inner: Arc<Mutex<Vec<u8>>>,
```

In Bevy's standard scheduler, `ResMut<R>` access is exclusive within the same system, and the sync system reads the tracker via `world.resource_scope`. Effective contention is zero. `std::sync::Mutex` has poisoning + lock fairness overhead that's unnecessary here.

**Options:**
- (a) `parking_lot::Mutex` — already in the dependency tree, no poisoning, faster.
- (b) `bevy_ecs` exposes `Mut<T>` change-detection wrappers that could replace the manual mutex entirely — push-style "dirty bit" with each Property having its own tracker.
- (c) Lock-free `crossbeam::queue::SegQueue<u8>` — overkill for this volume.

Not urgent; resources are typically tens of fields max, so the mutex is sub-microsecond.

---

### D2. 💡 Per-tick `world.resource_scope::<ServerImpl, _>` even when nothing dirty

`sync_resource_outgoing<R>` does the cheap check `if dirty.is_empty() { return; }` BEFORE the resource_scope, so this is fine. But `world.get_resource::<SyncDirtyTracker<R>>()` is called every tick per registered resource type, regardless. With many registered resources this adds up.

**Better:** a single `resource_sync_dispatcher` system that holds a registry of registered resource types and their dirty tracker handles, dispatching only the ones with dirty bits. Currently each resource type adds its own system to the schedule, which Bevy handles fine but isn't optimal at scale (>100 resources).

---

### D3. 💡 `commands.replicate_resource` queues a Command that runs deferred

Standard Bevy pattern. But it means `let entity = server.resource_entity::<R>()` immediately after `commands.replicate_resource(value)` returns `None` — the spawn hasn't applied yet. Documented behavior of Bevy Commands but a sharp edge for users who don't realize.

**Mitigation:** documentation example showing the correct ordering. Or provide a `commands.replicate_resource_now` that takes `&mut World` for immediate-mode use.

---

## E. Documentation / discoverability

### E1. 📚 No user-facing `_AGENTS/RESOURCES.md` README-style guide

`_AGENTS/RESOURCES_PLAN.md` is the design + implementation status doc — dense, internal-flavored. New users would benefit from a separate `RESOURCES.md` that's:
- One-page "how to use Replicated Resources" walkthrough
- Comparison table to Bevy's `Resource` (what's the same, what's different)
- Clear "Mode B requires `#[derive(Resource)]`" callout
- Code samples for the three lifecycle operations (insert, mutate, remove)
- Authority delegation walkthrough

Currently this content is scattered across commit messages and §10 of the plan.

---

### E2. 📚 The plan doc still references `_AGENTS/REPLICATED_RESOURCES.feature` as if it's authoritative

The feature file moved out of `test/specs/features/` because it lacked step bindings (would have failed namako lint). It's now in `_AGENTS/` as a planning artifact only. The plan doc should be clear: "this is reference Gherkin; bindings in `test/harness/tests/replicated_resources.rs` are the actual gate."

---

### E3. 📚 No worked example in any demo (`demos/bevy/`)

The shared demo protocol (`demos/bevy/shared/src/protocol.rs`) doesn't register any resource. The server demo doesn't insert one. Users discovering naia via the demo never see Replicated Resources.

**Fix:** add a `GameClock` or `Score` resource to the bevy demo suite — minimal additions to:
- `demos/bevy/shared/src/components/mod.rs` (add `ResourcesPlugin`)
- `demos/bevy/server/src/main.rs` (add `add_resource_events`, `commands.replicate_resource(GameClock::new(0))`)
- `demos/bevy/client/src/app.rs` (add `add_resource_events`, `fn read(clock: Res<GameClock>)`)

Doubles as a smoke test for the bevy adapter end-to-end.

---

### E4. 📚 `mirror_single_field` doctring doesn't mention the panic

`shared/src/world/component/replicate.rs:86` — the trait method documents the contract for valid use but doesn't mention that the codegen panics on type-mismatch (see A2). Users implementing `Replicate` by hand (rare but possible) need to know.

---

## F. Test coverage gaps

### F1. 🔬 No integration test for client-side `Res<R>` access

This is a direct consequence of A1 — the feature isn't implemented on the client side. No test would currently pass.

Once A1 lands, add: `bevy_app_resource_round_trip` test that stands up a Bevy `App` with the naia plugins, server inserts a resource, client connects, client system reads `Res<R>`, observes value.

The current `delegated_resource_supports_client_authority_request` test goes through the harness's scan-based lookup, not the bevy `Res<R>` path.

---

### F2. 🔬 Mode B per-field diff isn't asserted on the wire

The `mirror_single_field_copies_only_indexed_field` test verifies the codegen. The `server_mutation_replicates_to_client` test verifies updates propagate. But **no test asserts that mutating ONE field of a resource transmits ONLY that field** (the actual Mode B contract — the whole reason we did the per-field mirror work).

**What to add:** a test that mutates `Score.home`, captures the next outbound packet via the bench infrastructure, and asserts the packet bit-length is consistent with one-field-update (vs two-field-update). The existing `benches/tests/static_split_bytes.rs` shows the bandwidth-measurement pattern.

---

### F3. 🔬 No test for the disconnect-with-authority resource case

D15 says: client holding resource authority disconnects → authority reverts to Available, value persists. The underlying entity machinery does this; resources inherit it. But there's no integration test that exercises the resource-flavored path. Easy to add now that the delegation test infrastructure exists.

---

### F4. 🔬 No test for the D13 component→resource event translation

I added the translation logic in `component_event_registry.rs` (both server + client) but didn't write a test that:
1. Registers `add_resource_events::<R>()`
2. Inserts the resource on the server
3. Asserts client received `InsertResourceEvent<T, R>` and ZERO `InsertComponentEvent<T, R>`

This is a Bevy-app-level test (requires standing up a Bevy App with the naia plugins), which is why it was skipped — the existing harness uses `naia_demo_world` not Bevy. A dedicated `adapters/bevy/server/tests/` integration test directory would fill this gap.

---

### F5. 🔬 No test for Mode B echo prevention

When a delegated resource update arrives from the server, the entity-component is updated (no mutator fires — it's a `RemoteOwnedProperty`). The client-side mirror should propagate to the bevy `Resource` WITHOUT triggering the client's `SyncMutator` (which would echo the update back to the server). This is the echo-prevention contract documented in `resource_sync.rs`. **Untested.**

Will only matter once A1 (client-side mirror) lands.

---

## G. Broader naia architecture observations (out of feature scope)

These pre-existed; the Resources work surfaced them but didn't introduce them. Listed for awareness.

### G1. 💡 `ComponentKinds::add_component` panics at 64 components

`shared/src/world/component/component_kinds.rs:137-141`:
```rust
assert!(
    net_id < 64,
    "DirtySet bitset supports max 64 component kinds; ...",
);
```

The Resources feature adds 1 ComponentKind per `add_resource::<R>()` call. A protocol with 50 components + 30 resources blows the limit. Users won't know until they hit it.

**Fix paths:** documented in the assert (extend `DirtyQueue::dirty_bits` to two `u64`s). Worth doing proactively before the limit bites.

---

### G2. 💡 `WorldServer` is 3500+ lines

`server/src/server/world_server.rs` is the central god-object of the server. The Resources work added ~150 lines to it (insert/remove/registry/scope-bypass). Not a Resources problem per se but the file is over-stuffed and ripe for decomposition (resource methods → `world_server_resources.rs`, scope methods → `world_server_scope.rs`, etc.).

---

### G3. 💡 The `Replicate` trait has 25 methods

Most are auto-generated by the derive macro, but the trait surface is intimidating for anyone reading the source. The newly-added `mirror_single_field` brings it to 26.

**Refactor path:** split `Replicate` into:
- `ReplicateCore` (kind, dyn_ref, dyn_mut, copy_to_box) — minimal trait every Replicate impl needs
- `ReplicateWrite` (write, write_update)
- `ReplicateRead` (read_apply_update, read_apply_field_update)
- `ReplicateMirror` (mirror, mirror_single_field)
- `ReplicateAuthority` (publish/unpublish/enable_delegation/disable_delegation/localize)
- `ReplicateEntityRelations` (relations_waiting/relations_complete)

Then `Replicate = ReplicateCore + ReplicateWrite + ReplicateRead + ReplicateMirror + ReplicateAuthority + ReplicateEntityRelations` as a trait alias. Users still write `R: Replicate` everywhere; internal code can demand only what it needs.

Not urgent. Significant lift. Would make the codebase much more navigable.

---

### G4. 💡 Multiple `Command` boilerplate Commands across commands.rs files

`ConfigureReplicationCommand`, `LocalDuplicateComponents`, `ReplicateResourceCommand`, `RemoveReplicatedResourceCommand`, `ConfigureReplicatedResourceCommand`, `RequestResourceAuthorityCommand`, `ReleaseResourceAuthorityCommand` — each is ~30 lines of struct + new() + Command impl that all do the same `world.resource_scope` dance.

**Generic helper:**
```rust
pub(crate) struct WorldOpCommand<F>
where F: FnOnce(&mut World) + Send + 'static
{ op: F }

impl<F> Command for WorldOpCommand<F>
where F: FnOnce(&mut World) + Send + 'static
{
    fn apply(self, world: &mut World) { (self.op)(world); }
}
```

Then `commands.queue(WorldOpCommand { op: |world| { /* arbitrary world ops */ } })`. Most of the per-Command struct boilerplate disappears.

The downside: type erasure means every closure-using Command erases to a different type, slightly worse for compile time. Worth the tradeoff for the readability win.

---

## H. Suggested next sprint (priority-ordered)

1. **A1 (client-side mirror) — blocks the spec contract.** Without it, `Res<R>` on the client doesn't work; users get a panic. ~250 lines of new code mirroring the server-side resource_sync.rs pattern, plus a ClientResourceRegistry struct.
2. **B1 (sealed trait alias `ReplicatedResource`)** — promote the dead `ResourceBound` to public, remove ~24 verbose-bound repetitions across the bevy adapter. ~30 minutes.
3. **B3 (`AuthorityError::ResourceNotPresent` variant)** — semantic correctness. ~10 minutes.
4. **F1 + F4 (Bevy-app integration tests)** — stand up a `tests/` directory in `adapters/bevy/server/`. ~2-4 hours.
5. **E3 (demo update)** — add a `GameClock` resource to `demos/bevy/`. ~1 hour, doubles as smoke test.
6. **C5 (stale comment cleanup)** + **C6 (`Mutable` re-export consistency)** + **C7 (inline `clone_via_replicate`)** — drive-by cleanup. ~15 minutes.
7. **A3 (decision on `WorldServer::resource_kinds` field)** — delete or wire. ~30 minutes.
8. **F2 (per-field diff wire test)** — proves the Mode B promise. ~1 hour.
9. **G1 (raise 64-component limit)** — preventive maintenance. ~1 hour.
10. **A2 (`mirror_single_field` panic → debug_assert)** — robustness. ~10 minutes.
11. **G3 + G4 (broader refactors)** — substantial; defer until a quiet sprint.

---

## I. Bottom line

The Replicated Resources feature is functional and the underlying architecture (1-component-entity, per-field mirror via `mirror_single_field`, Commands-queue dispatch) is correct and forward-compatible. The user-facing API is clean for the **server side**. The **client side** has a real gap (A1) that the original plan promised but the implementation didn't deliver.

The duplications and verbose bounds (B1, B5) are mechanical fixes that would meaningfully improve the codebase's "pristine" quality. The broader naia patterns (G1-G4) pre-exist the Resources work and should be tackled in a dedicated cleanup sprint — they're not blockers but compound over time.

Total estimated effort to address the 🚨 + ⚠️ items: **1-2 focused engineering days**, mostly on A1.
