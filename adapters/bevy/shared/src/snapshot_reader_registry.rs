//! Per-`ComponentKind` read-and-box registry, captured at
//! [`Protocol::add_component`] time.
//!
//! This is the server-side snapshot-reader substrate: every component
//! registered via `Protocol::add_component::<C>` has its typed
//! `EntityRef → Option<Box<dyn Replicate>>` closure inserted here
//! automatically.  The registry is later consumed by the server's
//! `build_snapshot` helper (in `naia-bevy-server`) and by the `#9`
//! desync-harness in diax.
//!
//! # Clone / Send / Sync contract
//!
//! Each reader closure is `Box<dyn Fn(&EntityRef) -> … + Send + Sync>`
//! and is NOT `Clone`.  The registry wraps readers in `Arc<…>` so the
//! containing `Protocol` (which is `Clone`) can share them without
//! copying.

use std::{
    collections::HashMap,
    sync::Arc,
};

use bevy_ecs::{
    component::{Component, Mutable},
    resource::Resource,
    world::EntityRef,
};

use naia_shared::{ComponentKind, Replicate};

type ReadAndBoxFn = dyn Fn(&EntityRef) -> Option<Box<dyn Replicate>> + Send + Sync;

/// Registry mapping `ComponentKind → read-and-box closure`.
///
/// Cloning the registry is `O(n)` in registered kinds (each `Arc` clone
/// is cheap).  All registered closures are `Send + Sync` so the registry
/// itself is `Send + Sync`.
///
/// Implements `Resource` so it can be installed on a Bevy world.
#[derive(Default, Resource)]
pub struct SnapshotReaderRegistry {
    readers: HashMap<ComponentKind, Arc<ReadAndBoxFn>>,
}

impl Clone for SnapshotReaderRegistry {
    fn clone(&self) -> Self {
        Self {
            readers: self.readers.clone(),
        }
    }
}

impl SnapshotReaderRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Capture the per-`C` read-and-box closure.  Called from
    /// `Protocol::add_component::<C>()`.
    pub(crate) fn register<C: Replicate + Component<Mutability = Mutable>>(&mut self) {
        let kind = ComponentKind::of::<C>();
        let reader: Arc<ReadAndBoxFn> =
            Arc::new(move |entity_ref: &EntityRef| {
                entity_ref.get::<C>().map(|c| c.copy_to_box())
            });
        self.readers.insert(kind, reader);
    }

    /// Read a registered component off an entity, returning a type-erased
    /// box.  Returns `None` if the entity lacks the component OR if `kind`
    /// is not registered (the latter cannot happen after `#1` lands, but
    /// callers may still see it during transition).
    ///
    /// This is the **reusable read surface** `#9` (desync harness) and any
    /// future per-`ComponentKind` consumer will call.
    pub fn read(
        &self,
        kind: &ComponentKind,
        entity_ref: &EntityRef,
    ) -> Option<Box<dyn Replicate>> {
        let reader = self.readers.get(kind)?;
        reader(entity_ref)
    }

    /// Iterate over all registered `ComponentKind`s.  Used for completeness
    /// assertions.
    pub fn registered_kinds(&self) -> impl Iterator<Item = &ComponentKind> {
        self.readers.keys()
    }

    pub fn len(&self) -> usize {
        self.readers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.readers.is_empty()
    }

    pub fn contains(&self, kind: &ComponentKind) -> bool {
        self.readers.contains_key(kind)
    }
}
