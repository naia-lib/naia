//! Bevy-resource ↔ entity-component mirror for Replicated Resources.
//!
//! Implements **Mode B** of `_AGENTS/RESOURCES_PLAN.md` §4.5:
//! the user accesses replicated resources via standard `Res<R>` /
//! `ResMut<R>`, and a per-tick sync system bridges the bevy-resource
//! storage with the hidden 1-component entity that carries the wire
//! state. **No over-replication** — only fields the user actually
//! touched via `Property<T>::DerefMut` are synced.
//!
//! ## Architecture
//!
//! Each registered resource type `R` gets:
//!
//! 1. A bevy-side `R` value stored as a normal `Bevy Resource`. The
//!    user reads/writes it via `Res<R>` / `ResMut<R>`.
//! 2. A bevy-side `SyncDirtyTracker<R>` resource — a small lock-free
//!    ring of `u8` field indices. Append-only from `SyncMutator<R>`,
//!    drained by the per-tick sync system.
//! 3. A `SyncMutator<R>` value installed as the bevy-resource R's
//!    `PropertyMutator`. When the user mutates a single
//!    `Property<T>` field via `*resmut.field = v`, Property's
//!    `DerefMut` calls `SyncMutator::mutate(field_index)`, which
//!    pushes `field_index` into the `SyncDirtyTracker`.
//! 4. A hidden naia entity carrying `R` as its sole replicated
//!    component, with the standard naia `PropertyMutator` set
//!    (drives the normal `DirtyQueue` → outgoing replication path).
//! 5. A per-tick sync system that drains the `SyncDirtyTracker` and,
//!    for each dirty field index, calls
//!    `Replicate::mirror_single_field(idx, &bevy_value)` on the
//!    entity-component. That fires the entity-component's mutator
//!    for ONLY that field — `DirtyQueue` records exactly the diff
//!    bits the user touched, the next outbound packet sends only
//!    those fields. Per-field diff tracking is preserved end-to-end.
//!
//! ## Echo prevention
//!
//! For incoming updates (the delegated case where a remote authority
//! writes to the resource and the wire pushes the update into the
//! entity-component), the entity-component is updated by the existing
//! remote-apply path which uses `RemoteOwnedProperty::DerefMut` (does
//! NOT call `mutate()`), so the entity-component's dirty bits are not
//! set — no outgoing reflection. The bevy mirror then needs to copy
//! entity-component → bevy-resource WITHOUT touching the
//! `SyncDirtyTracker` (otherwise the next outgoing pass would echo
//! the just-received update back). The incoming-mirror system bypasses
//! the SyncMutator by mirroring fields with mutators temporarily
//! detached, then re-attaches them. (Server-side incoming-mirror is a
//! no-op for server-authoritative resources because the server IS the
//! authority; it kicks in for delegated resources held by clients.)

use std::{marker::PhantomData, sync::Arc};

use bevy_app::App;
use bevy_ecs::world::{Mut, World};
use parking_lot::Mutex;

use naia_bevy_shared::{
    PropertyMutate, PropertyMutator, Replicate, ReplicatedResource, WorldMutType, WorldProxyMut,
};

use crate::server::ServerImpl;

/// Lock-free-ish (Arc<Mutex>) accumulator of dirty Property field
/// indices for a single resource type `R`. The bevy `Resource`
/// inserted into the `World`. Read+drained once per tick by the sync
/// system; pushed into by `SyncMutator<R>::mutate`.
///
/// `Vec<u8>` of indices; duplicates are tolerated and de-duped at
/// drain time (mutating the same field twice in one tick collapses
/// into a single mirror call).
#[derive(bevy_ecs::resource::Resource)]
pub struct SyncDirtyTracker<R: Replicate> {
    pub(crate) inner: Arc<Mutex<Vec<u8>>>,
    _phantom: PhantomData<R>,
}

impl<R: Replicate> Default for SyncDirtyTracker<R> {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Vec::new())),
            _phantom: PhantomData,
        }
    }
}

impl<R: Replicate> SyncDirtyTracker<R> {
    /// Drain the dirty index buffer. Sorts + dedupes so the sync system
    /// performs at most one mirror per Property field per tick even if
    /// the user touched it multiple times.
    pub(crate) fn drain_unique(&self) -> Vec<u8> {
        let mut g = self.inner.lock();
        if g.is_empty() {
            return Vec::new();
        }
        let mut out: Vec<u8> = g.drain(..).collect();
        out.sort_unstable();
        out.dedup();
        out
    }
}

/// `PropertyMutate` impl wired into the bevy-resource side of `R`.
/// Each `Property<T>` field of the bevy-resource holds one of these
/// (cloned from a single canonical instance) and pushes its index
/// into the shared `SyncDirtyTracker<R>` on `mutate()`.
pub(crate) struct SyncMutator<R: Replicate> {
    inner: Arc<Mutex<Vec<u8>>>,
    _phantom: PhantomData<R>,
}

// Manual Clone impl avoids requiring `R: Clone` (PropertyMutate's
// blanket impl chain requires Clone, but we only need to clone the
// `Arc<Mutex<...>>` handle, never `R` itself).
impl<R: Replicate> Clone for SyncMutator<R> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            _phantom: PhantomData,
        }
    }
}

impl<R: Replicate> SyncMutator<R> {
    pub(crate) fn new(tracker: &SyncDirtyTracker<R>) -> Self {
        Self {
            inner: Arc::clone(&tracker.inner),
            _phantom: PhantomData,
        }
    }
}

impl<R: Replicate> PropertyMutate for SyncMutator<R> {
    fn mutate(&mut self, property_index: u8) -> bool {
        // parking_lot's Mutex doesn't poison, so this can't fail.
        self.inner.lock().push(property_index);
        true
    }
}

/// Per-resource sync hook: a type-erased closure that knows how to
/// drain the `SyncDirtyTracker<R>` for a specific R and mirror dirty
/// fields into the entity-component. Stored in
/// `ResourceSyncDispatcher` and invoked by the single dispatcher
/// system.
type SyncHook = Box<dyn Fn(&mut World) + Send + Sync + 'static>;

/// Single dispatcher Bevy `Resource` holding the per-resource sync
/// hooks. One Bevy system runs all hooks each tick (D2 of
/// RESOURCES_AUDIT.md), avoiding `add_systems` proliferation across
/// many registered resource types.
#[derive(bevy_ecs::resource::Resource, Default)]
pub(crate) struct ResourceSyncDispatcher {
    hooks: Vec<SyncHook>,
}

impl ResourceSyncDispatcher {
    fn run_all(world: &mut World) {
        // Take ownership of the hooks vec briefly so we can call each
        // with &mut World without holding the dispatcher Resource borrow.
        let hooks: Vec<SyncHook> = world
            .resource_mut::<ResourceSyncDispatcher>()
            .hooks
            .drain(..)
            .collect();
        for hook in &hooks {
            hook(world);
        }
        // Restore. Hooks are static (registered once), so this is
        // semantically a no-op — we just put them back where they live.
        world.resource_mut::<ResourceSyncDispatcher>().hooks = hooks;
    }
}

/// Install the dispatcher system once on first call. Idempotent.
fn ensure_dispatcher_system_installed(app: &mut App) {
    if app
        .world()
        .get_resource::<ResourceSyncDispatcher>()
        .is_none()
    {
        app.insert_resource(ResourceSyncDispatcher::default());
        app.add_systems(bevy_app::Update, ResourceSyncDispatcher::run_all);
    }
}

/// Register the per-tick outgoing sync hook for resource type `R`.
/// Called from `add_resource_events::<R>()`. Idempotent — guarded by
/// a `ResourceSyncInstalled<R>` marker so re-registering the same `R`
/// doesn't double-install the hook.
pub(crate) fn install_resource_sync_system<R: ReplicatedResource>(app: &mut App) {
    ensure_dispatcher_system_installed(app);
    if app
        .world()
        .get_resource::<ResourceSyncInstalled<R>>()
        .is_some()
    {
        return;
    }
    app.insert_resource(ResourceSyncInstalled::<R> {
        _phantom: PhantomData,
    });
    let hook: SyncHook = Box::new(sync_resource_outgoing::<R>);
    app.world_mut()
        .resource_mut::<ResourceSyncDispatcher>()
        .hooks
        .push(hook);
}

#[derive(bevy_ecs::resource::Resource)]
struct ResourceSyncInstalled<R: Replicate> {
    _phantom: PhantomData<R>,
}

/// Per-tick outgoing sync for a specific `R`: drain its
/// `SyncDirtyTracker` and mirror each dirty Property field from
/// bevy-resource → entity-component via
/// `Replicate::mirror_single_field`. O(dirty fields). No reflection
/// of unchanged fields. Called by the single dispatcher system.
fn sync_resource_outgoing<R: ReplicatedResource>(world: &mut World) {
    let dirty: Vec<u8> = match world.get_resource::<SyncDirtyTracker<R>>() {
        Some(t) => t.drain_unique(),
        None => return,
    };
    if dirty.is_empty() {
        return;
    }
    world.resource_scope::<ServerImpl, _>(|world, server: Mut<ServerImpl>| {
        let Some(entity) = server.resource_entity::<R>() else {
            return;
        };
        let snapshot: Box<dyn Replicate> = match world.get_resource::<R>() {
            Some(r) => r.copy_to_box(),
            None => return,
        };
        let mut world_mut = world.proxy_mut();
        let Some(mut entity_comp) = world_mut.component_mut::<R>(&entity) else {
            return;
        };
        for &idx in &dirty {
            entity_comp.mirror_single_field(idx, snapshot.as_ref());
        }
    });
}

// =====================================================================
// Public helper: wire the SyncMutator into a freshly-prepared R value.
// Called from ReplicateResourceCommand right before inserting R as a
// bevy Resource. The PropertyMutator chain inside R must be set so
// that any future *resmut.field = v call records the field index.
// =====================================================================

/// Set up `value`'s Property mutators to point at `tracker`, so that
/// future `Property<T>::DerefMut` operations on this value (via bevy
/// `ResMut<R>`) push their field indices into the `SyncDirtyTracker`.
///
/// Mirrors `naia_shared::Replicate::set_mutator` semantics — uses the
/// same trait method, just with a `SyncMutator<R>` instead of the
/// usual NaiaPropertyMutator that the entity-component side uses.
pub(crate) fn wire_sync_mutator<R: Replicate>(
    value: &mut R,
    tracker: &SyncDirtyTracker<R>,
) {
    let sync = SyncMutator::<R>::new(tracker);
    let mutator = PropertyMutator::new(sync);
    value.set_mutator(&mutator);
}

/// Insert `R` as a Bevy `Resource` mirror with the `SyncMutator<R>`
/// wired into its Property fields. Called from
/// `ReplicateResourceCommand::apply` AFTER the entity-component side
/// is in place.
///
/// Mode B is the only mode (per RESOURCES_AUDIT.md decisions). This
/// function panics if `add_resource_events::<R>()` wasn't called first
/// — the missing `SyncDirtyTracker<R>` indicates the user forgot to
/// register, and we'd silently fail otherwise.
pub(crate) fn install_bevy_resource_mirror<R: ReplicatedResource>(
    world: &mut World,
    mut value: R,
) {
    let tracker = world.get_resource::<SyncDirtyTracker<R>>().unwrap_or_else(|| {
        panic!(
            "naia replicate_resource: missing SyncDirtyTracker<{0}>. \
             You must call `app.add_resource_events::<{0}>()` before \
             `commands.replicate_resource(...)`.",
            std::any::type_name::<R>()
        )
    });
    wire_sync_mutator::<R>(&mut value, tracker);
    world.insert_resource(value);
}
