//! Client-side bevy-resource ↔ entity-component mirror.
//!
//! Mirror of `adapters/bevy/server/src/resource_sync.rs` for the
//! client side. Two paths:
//!
//! ## Incoming (server → client)
//!
//! When the naia client's remote-apply path delivers an
//! `InsertComponent` for a component kind in `protocol.resource_kinds`,
//! the client populates its `ResourceRegistry` (in `client.rs`). The
//! per-tick **incoming** sync system here mirrors the entity-component
//! value into the bevy `Resource<R>` storage so user systems read via
//! `Res<R>`.
//!
//! Echo prevention: the bevy-side write uses
//! `bypass_change_detection`-equivalent semantics so the user's
//! outgoing `SyncMutator<R>` chain isn't triggered. Otherwise an
//! incoming server update would be re-replicated back as a client write.
//!
//! ## Outgoing (client → server, only after authority granted)
//!
//! Same as the server side: `ResMut<R>` mutations push field indices
//! into a `SyncDirtyTracker<R>`; the per-tick outgoing sync drains and
//! mirrors via `Replicate::mirror_single_field` into the entity-
//! component, which fires its naia `PropertyMutator` and replicates
//! back to the server. Client-side authority check is built into the
//! entity-component side (HostOwnedProperty mutates; RemoteOwnedProperty
//! is the no-op silent-rejection path D18).
//!
//! ## Removal
//!
//! Despawn of the resource entity (server removed it OR scope exit)
//! triggers the client-side `ResourceRegistry::remove_by_entity`
//! (in client.rs) and the per-tick incoming sync drops the bevy
//! `Resource<R>` storage on the next pass.

use std::{marker::PhantomData, sync::Arc};

use bevy_app::App;
use bevy_ecs::{
    change_detection::DetectChangesMut,
    world::{Mut, World},
};
use parking_lot::Mutex;

use naia_bevy_shared::{
    PropertyMutate, PropertyMutator, Replicate, ReplicatedResource, WorldMutType, WorldProxy,
    WorldProxyMut,
};

use crate::client::ClientWrapper;

/// Per-resource-type dirty-field tracker on the client side. Pushed
/// to by `SyncMutator<T, R>` when the user mutates a Property field
/// via `ResMut<R>`; drained by the per-tick outgoing sync system.
#[derive(bevy_ecs::resource::Resource)]
pub struct SyncDirtyTracker<T: Send + Sync + 'static, R: Replicate> {
    pub(crate) inner: Arc<Mutex<Vec<u8>>>,
    pub(crate) _phantom_t: PhantomData<T>,
    pub(crate) _phantom_r: PhantomData<R>,
}

impl<T: Send + Sync + 'static, R: Replicate> Default for SyncDirtyTracker<T, R> {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Vec::new())),
            _phantom_t: PhantomData,
            _phantom_r: PhantomData,
        }
    }
}

impl<T: Send + Sync + 'static, R: Replicate> SyncDirtyTracker<T, R> {
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

/// `PropertyMutate` impl wired into the bevy-resource side of `R` on
/// the client. Pushes touched field indices into the tracker.
pub(crate) struct SyncMutator<T: Send + Sync + 'static, R: Replicate> {
    inner: Arc<Mutex<Vec<u8>>>,
    _phantom_t: PhantomData<T>,
    _phantom_r: PhantomData<R>,
}

impl<T: Send + Sync + 'static, R: Replicate> Clone for SyncMutator<T, R> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            _phantom_t: PhantomData,
            _phantom_r: PhantomData,
        }
    }
}

impl<T: Send + Sync + 'static, R: Replicate> SyncMutator<T, R> {
    pub(crate) fn new(tracker: &SyncDirtyTracker<T, R>) -> Self {
        Self {
            inner: Arc::clone(&tracker.inner),
            _phantom_t: PhantomData,
            _phantom_r: PhantomData,
        }
    }
}

impl<T: Send + Sync + 'static, R: Replicate> PropertyMutate for SyncMutator<T, R> {
    fn mutate(&mut self, property_index: u8) -> bool {
        self.inner.lock().push(property_index);
        true
    }
}

/// Type-erased per-resource sync hook (one for incoming, one for
/// outgoing per type). Stored in the dispatcher.
type SyncHook = Box<dyn Fn(&mut World) + Send + Sync + 'static>;

/// Single dispatcher Bevy `Resource` holding per-type incoming +
/// outgoing sync hooks for the client side. One Bevy system runs all
/// hooks each tick.
#[derive(bevy_ecs::resource::Resource)]
pub(crate) struct ResourceSyncDispatcher<T: Send + Sync + 'static> {
    incoming_hooks: Vec<SyncHook>,
    outgoing_hooks: Vec<SyncHook>,
    _phantom_t: PhantomData<T>,
}

impl<T: Send + Sync + 'static> Default for ResourceSyncDispatcher<T> {
    fn default() -> Self {
        Self {
            incoming_hooks: Vec::new(),
            outgoing_hooks: Vec::new(),
            _phantom_t: PhantomData,
        }
    }
}

impl<T: Send + Sync + 'static> ResourceSyncDispatcher<T> {
    fn run_all(world: &mut World) {
        // Take ownership briefly so each hook gets &mut World.
        let (incoming, outgoing) = {
            let mut d = world.resource_mut::<ResourceSyncDispatcher<T>>();
            (
                std::mem::take(&mut d.incoming_hooks),
                std::mem::take(&mut d.outgoing_hooks),
            )
        };
        // Incoming first: server → bevy resource.
        for hook in &incoming {
            hook(world);
        }
        // Then outgoing: bevy resource → entity-component → wire.
        for hook in &outgoing {
            hook(world);
        }
        let mut d = world.resource_mut::<ResourceSyncDispatcher<T>>();
        d.incoming_hooks = incoming;
        d.outgoing_hooks = outgoing;
    }
}

fn ensure_dispatcher_system_installed<T: Send + Sync + 'static>(app: &mut App) {
    if app
        .world()
        .get_resource::<ResourceSyncDispatcher<T>>()
        .is_none()
    {
        app.insert_resource(ResourceSyncDispatcher::<T>::default());
        app.add_systems(bevy_app::Update, ResourceSyncDispatcher::<T>::run_all);
    }
}

/// Install the per-resource sync hooks for type `R` on the client
/// side. Called from `add_resource_events::<T, R>()`.
pub(crate) fn install_resource_sync_system<T, R>(app: &mut App)
where
    T: Send + Sync + 'static,
    R: ReplicatedResource,
{
    ensure_dispatcher_system_installed::<T>(app);
    if app
        .world()
        .get_resource::<ResourceSyncInstalled<T, R>>()
        .is_some()
    {
        return;
    }
    app.insert_resource(ResourceSyncInstalled::<T, R> {
        _phantom_t: PhantomData,
        _phantom_r: PhantomData,
    });
    // Tracker for outgoing client mutations.
    if app
        .world()
        .get_resource::<SyncDirtyTracker<T, R>>()
        .is_none()
    {
        app.insert_resource(SyncDirtyTracker::<T, R>::default());
    }
    let incoming: SyncHook = Box::new(sync_resource_incoming::<T, R>);
    let outgoing: SyncHook = Box::new(sync_resource_outgoing::<T, R>);
    let mut dispatcher = app.world_mut().resource_mut::<ResourceSyncDispatcher<T>>();
    dispatcher.incoming_hooks.push(incoming);
    dispatcher.outgoing_hooks.push(outgoing);
}

#[derive(bevy_ecs::resource::Resource)]
struct ResourceSyncInstalled<T: Send + Sync + 'static, R: Replicate> {
    _phantom_t: PhantomData<T>,
    _phantom_r: PhantomData<R>,
}

/// Per-tick incoming sync: entity-component R → bevy `Resource<R>`.
///
/// Always runs (cheap when no resource is in scope). When the
/// `client.resource_entity::<R>()` lookup returns an entity:
/// - If bevy `Resource<R>` is absent → insert (Mode B's first-arrival
///   path; mirrors a fresh `InsertResourceEvent`).
/// - If present → overwrite by mirroring all fields from the entity-
///   component into the bevy resource. Uses `bypass_change_detection`
///   so we don't re-trigger the outgoing SyncMutator chain.
///
/// When the lookup returns None and bevy `Resource<R>` is present
/// → remove (mirrors a `RemoveResourceEvent`).
fn sync_resource_incoming<T, R>(world: &mut World)
where
    T: Send + Sync + 'static,
    R: ReplicatedResource,
{
    // Read the resource entity from the client wrapper.
    let world_entity_opt: Option<bevy_ecs::entity::Entity> =
        world.resource_scope::<ClientWrapper<T>, _>(|_, client| {
            client.client.resource_entity::<R>()
        });

    match world_entity_opt {
        Some(entity) => {
            // Snapshot the entity-component value (Box<dyn Replicate>
            // via the trait's own clone path — no R: Clone bound).
            let snapshot: Option<Box<dyn Replicate>> = {
                let world_ref = WorldProxy::proxy(&*world);
                use naia_bevy_shared::WorldRefType;
                world_ref.component::<R>(&entity).map(|c| (*c).copy_to_box())
            };
            let Some(snapshot) = snapshot else {
                return;
            };
            if world.get_resource::<R>().is_some() {
                // Already present — overwrite via bypass_change_detection
                // so the outgoing SyncMutator chain does not re-pick-up
                // these mutations as user writes (echo prevention).
                //
                // Process: temporarily detach the SyncMutator from the
                // bevy-resource Properties (point them at a throwaway
                // tracker), do the mirror, then re-attach to the real
                // tracker. The throwaway absorbs the spurious dirty
                // bits the mirror would otherwise push.
                let throwaway = SyncDirtyTracker::<T, R>::default();
                {
                    let mut bevy_res = world.resource_mut::<R>();
                    let bevy_res_ref: &mut R = bevy_res.bypass_change_detection();
                    wire_sync_mutator::<T, R>(bevy_res_ref, &throwaway);
                    bevy_res_ref.mirror(snapshot.as_ref());
                }
                // Re-attach to the real tracker.
                {
                    // Snapshot the Arc so we can drop the immutable
                    // borrow before taking the mutable one.
                    let real_arc = world
                        .get_resource::<SyncDirtyTracker<T, R>>()
                        .expect("tracker installed by add_resource_events")
                        .inner
                        .clone();
                    let real_tracker = SyncDirtyTracker::<T, R> {
                        inner: real_arc,
                        _phantom_t: PhantomData,
                        _phantom_r: PhantomData,
                    };
                    let mut bevy_res = world.resource_mut::<R>();
                    let bevy_res_ref: &mut R = bevy_res.bypass_change_detection();
                    wire_sync_mutator::<T, R>(bevy_res_ref, &real_tracker);
                    bevy_res.set_changed();
                }
            } else {
                // First arrival: insert. Downcast back to R then wire
                // the SyncMutator into its Properties so subsequent
                // ResMut<R> writes record dirty bits.
                let any_box = snapshot.to_boxed_any();
                if let Ok(boxed_r) = any_box.downcast::<R>() {
                    let mut value = *boxed_r;
                    let tracker = world
                        .get_resource::<SyncDirtyTracker<T, R>>()
                        .expect("tracker installed by add_resource_events");
                    wire_sync_mutator::<T, R>(&mut value, tracker);
                    world.insert_resource(value);
                }
            }
        }
        None => {
            if world.contains_resource::<R>() {
                world.remove_resource::<R>();
            }
        }
    }
}

/// Per-tick outgoing sync: drain client-side `SyncDirtyTracker<T, R>`
/// and mirror touched fields into the entity-component (which fires
/// its PropertyMutator → DirtyQueue → wire).
///
/// Authority gate (D18 soft rejection): if the client doesn't hold
/// authority on the resource entity, drop the drained dirty indices
/// silently. The entity-component's `Property` is `RemoteOwnedProperty`
/// in that case and calling `mirror`/`mirror_single_field` on it
/// would panic ("Remote Property should never be set manually"). The
/// user's local bevy `Resource<R>` mutation is preserved (soft local
/// modification) but does not propagate; the next incoming server
/// update will overwrite it.
fn sync_resource_outgoing<T, R>(world: &mut World)
where
    T: Send + Sync + 'static,
    R: ReplicatedResource,
{
    let dirty: Vec<u8> = match world.get_resource::<SyncDirtyTracker<T, R>>() {
        Some(t) => t.drain_unique(),
        None => return,
    };
    if dirty.is_empty() {
        return;
    }
    world.resource_scope::<ClientWrapper<T>, _>(|world, client: Mut<ClientWrapper<T>>| {
        let Some(entity) = client.client.resource_entity::<R>() else {
            return;
        };
        // Authority gate. Only Granted (we hold authority) and the
        // server-owned-public case (where the client is the server's
        // mirror) accept writes. Anything else: drop silently.
        let auth = client.client.entity_authority_status(&entity);
        let writable = matches!(
            auth,
            Some(naia_bevy_shared::EntityAuthStatus::Granted)
        );
        if !writable {
            return;
        }
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

/// Wire `value`'s Property mutators to push field indices into
/// `tracker`. Same semantics as the server-side `wire_sync_mutator`.
pub(crate) fn wire_sync_mutator<T, R>(value: &mut R, tracker: &SyncDirtyTracker<T, R>)
where
    T: Send + Sync + 'static,
    R: Replicate,
{
    let sync = SyncMutator::<T, R>::new(tracker);
    let mutator = PropertyMutator::new(sync);
    value.set_mutator(&mutator);
}
