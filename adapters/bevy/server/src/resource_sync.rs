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

use std::{
    marker::PhantomData,
    sync::{Arc, Mutex},
};

use bevy_app::App;
use bevy_ecs::{
    component::Mutable,
    world::{Mut, World},
};
use naia_bevy_shared::{
    PropertyMutate, PropertyMutator, Replicate, WorldMutType, WorldProxyMut,
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
        let mut g = self.inner.lock().expect("SyncDirtyTracker mutex poisoned");
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
        // Best-effort push. If the lock is somehow poisoned, we lose
        // this dirty bit — acceptable because lock poisoning indicates
        // a panic in another thread and the world is already in an
        // unrecoverable state.
        if let Ok(mut g) = self.inner.lock() {
            g.push(property_index);
        }
        true
    }
}

/// Install the per-tick outgoing sync system for resource type `R`.
/// Called from `add_resource_events::<R>()` so users get the mirror
/// installed automatically when they register the events.
///
/// Idempotent — Bevy de-dupes systems by function pointer + type
/// parameters, so re-registering the same `R` is a no-op.
///
/// `R` must additionally implement `bevy_ecs::resource::Resource`
/// for Mode B. If `R` is registered for events but not as a Resource,
/// the system is a no-op (it just doesn't find the bevy-resource and
/// returns early); users can still access `R` via `Query<&R>` over
/// the resource entity (Mode A).
pub(crate) fn install_resource_sync_system<R>(app: &mut App)
where
    R: Replicate
        + bevy_ecs::resource::Resource
        + bevy_ecs::component::Component<Mutability = Mutable>,
{
    if app
        .world()
        .get_resource::<ResourceSyncInstalled<R>>()
        .is_none()
    {
        app.insert_resource(ResourceSyncInstalled::<R> {
            _phantom: PhantomData,
        });
        // SyncDirtyTracker<R> is inserted lazily on first
        // replicate_resource call; the sync system tolerates absence.
        app.add_systems(bevy_app::Update, sync_resource_outgoing::<R>);
    }
}

#[derive(bevy_ecs::resource::Resource)]
struct ResourceSyncInstalled<R: Replicate> {
    _phantom: PhantomData<R>,
}

/// Per-tick outgoing sync: drain the `SyncDirtyTracker<R>` and
/// mirror each dirty Property field from bevy-resource → entity-
/// component using `Replicate::mirror_single_field`.
///
/// O(dirty fields) per tick. No reflection of unchanged fields.
fn sync_resource_outgoing<R>(world: &mut World)
where
    R: Replicate
        + bevy_ecs::resource::Resource
        + bevy_ecs::component::Component<Mutability = Mutable>,
{
    // Cheap pre-check — drain only happens if there's anything to do.
    let dirty: Vec<u8> = match world.get_resource::<SyncDirtyTracker<R>>() {
        Some(t) => t.drain_unique(),
        None => return,
    };
    if dirty.is_empty() {
        return;
    }
    world.resource_scope::<ServerImpl, _>(|world, mut server: Mut<ServerImpl>| {
        let Some(entity) = server.resource_entity::<R>() else {
            return;
        };
        // Snapshot the bevy-resource value via the Replicate trait's
        // own copy path (no `R: Clone` bound required). The result is
        // a Box<dyn Replicate> that we pass to `mirror_single_field`.
        let snapshot: Box<dyn Replicate> = match world.get_resource::<R>() {
            Some(r) => r.copy_to_box(),
            None => return,
        };
        let mut world_mut = world.proxy_mut();
        let _ = &mut world_mut; // borrow-checker scaffold
        let Some(mut entity_comp) = world_mut.component_mut::<R>(&entity) else {
            return;
        };
        for &idx in &dirty {
            // mirror_single_field copies just the one field from
            // snapshot → entity_comp and fires the entity-component's
            // PropertyMutator for that one index. The DirtyQueue picks
            // it up; only that field gets serialized in the next
            // outgoing packet.
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

/// Mode-B install hook called from `ReplicateResourceCommand::apply`
/// AFTER the entity-component side is in place.
///
/// Behaviour:
/// - If a `SyncDirtyTracker<R>` is present in the World (= the user
///   called `add_resource_events::<R>()`), wire the snapshot's
///   `PropertyMutator`s to the tracker and insert the snapshot as a
///   bevy `Resource`. The user can now read/write via `Res<R>` /
///   `ResMut<R>`, and the per-tick sync system will mirror touched
///   fields to the entity-component.
/// - If no tracker is present (Mode A only — user didn't register
///   for event/Mode B support), the snapshot is dropped and the
///   user accesses R via `Query<&R>` over the resource entity.
///
/// Takes the snapshot as a `Box<dyn Replicate>` so the call site
/// doesn't need to know whether `R: bevy::Resource` (this function
/// downcasts and applies the bevy-Resource insert only for the right
/// type — Mode A users skip the cast).
pub(crate) fn install_bevy_resource_mirror_if_present<R>(
    world: &mut World,
    snapshot: Box<dyn Replicate>,
) where
    R: Replicate
        + bevy_ecs::resource::Resource
        + bevy_ecs::component::Component<Mutability = Mutable>,
{
    // Only proceed if Mode B was registered for this type.
    let tracker_present = world.get_resource::<SyncDirtyTracker<R>>().is_some();
    if !tracker_present {
        return;
    }
    // Downcast the boxed Replicate back to R. The Box was created
    // via `value.copy_to_box()` on a value of type R, so this is
    // infallible by construction; we still tolerate failure gracefully.
    let any_box = snapshot.to_boxed_any();
    let mut value: Box<R> = match any_box.downcast::<R>() {
        Ok(v) => v,
        Err(_) => {
            log::warn!(
                "naia install_bevy_resource_mirror: downcast failed; skipping bevy-Resource insert"
            );
            return;
        }
    };
    // Wire the SyncMutator into the snapshot value's Property fields,
    // then insert as bevy Resource. The mutator handle clones from the
    // tracker we just verified exists.
    {
        let tracker = world
            .get_resource::<SyncDirtyTracker<R>>()
            .expect("checked above");
        wire_sync_mutator::<R>(&mut *value, tracker);
    }
    world.insert_resource(*value);
}
