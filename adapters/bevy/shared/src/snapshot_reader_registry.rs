//! Per-`ComponentKind` read-and-box registry, captured at
//! [`Protocol::register_component` / `add_component`] time.
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

use naia_shared::{ComponentKind, Replicate, VecBitWriter};

type ReadAndBoxFn = dyn Fn(&EntityRef) -> Option<Box<dyn Replicate>> + Send + Sync;
type ReadToBytesFn = dyn Fn(&EntityRef) -> Option<Box<[u8]>> + Send + Sync;

/// Registry mapping `ComponentKind → read-and-box closure`.
///
/// Cloning the registry is `O(n)` in registered kinds (each `Arc` clone
/// is cheap).  All registered closures are `Send + Sync` so the registry
/// itself is `Send + Sync`.
///
/// Implements `Resource` so it can be installed on a Bevy world.
///
/// # Two read surfaces
///
/// - [`Self::read`] — reads the component and returns a boxed `Replicate`.
///   Used by the server's snapshot builder.
/// - [`Self::read_to_bytes`] — reads the component and serializes its field
///   values (via `write_update` with all-bits-set `DiffMask`) to a byte box.
///   Used by the `#9` desync harness to assemble registry-driven
///   `entity → {ComponentKind → bytes}` records for byte-exact comparison.
#[derive(Default, Resource)]
pub struct SnapshotReaderRegistry {
    readers: HashMap<ComponentKind, Arc<ReadAndBoxFn>>,
    bytes_readers: HashMap<ComponentKind, Arc<ReadToBytesFn>>,
}

impl Clone for SnapshotReaderRegistry {
    fn clone(&self) -> Self {
        Self {
            readers: self.readers.clone(),
            bytes_readers: self.bytes_readers.clone(),
        }
    }
}

impl SnapshotReaderRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Capture per-`C` read closures.  Called from
    /// `Protocol::register_component::<C>()`.
    pub(crate) fn register<C: Replicate + Component<Mutability = Mutable>>(&mut self) {
        let kind = ComponentKind::of::<C>();
        let reader: Arc<ReadAndBoxFn> =
            Arc::new(move |entity_ref: &EntityRef| {
                entity_ref.get::<C>().map(|c| c.copy_to_box())
            });
        self.readers.insert(kind, reader);

        // Bytes reader: serialize all field values (without the kind discriminant
        // header) via `write_fields_for_comparison`.  This new `Replicate` method
        // calls `Property::write_for_comparison` on every field — it never panics
        // for `LocalProperty` or `RemoteOwned` properties (unlike `Property::write`
        // which panics for those), so it works on both server-side and client-side
        // worlds.  Used by the `#9` desync harness for byte-exact comparison.
        let bytes_reader: Arc<ReadToBytesFn> =
            Arc::new(move |entity_ref: &EntityRef| {
                entity_ref.get::<C>().map(|c| {
                    let mut writer = VecBitWriter::new();
                    c.write_fields_for_comparison(&mut writer);
                    writer.to_bytes()
                })
            });
        self.bytes_readers.insert(kind, bytes_reader);
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

    /// Read a registered component off an entity, returning its field values
    /// serialized to bytes (no kind discriminant header).
    ///
    /// Returns `None` if the entity lacks the component or `kind` is not
    /// registered.  The byte layout is produced by `write_update` with all
    /// `DiffMask` bits set; it is deterministic and identical for equal values,
    /// making it suitable for byte-exact desync comparison.
    ///
    /// This is the **byte-comparison surface** the `#9` desync harness uses to
    /// assemble `entity → {ComponentKind → bytes}` records.
    pub fn read_to_bytes(
        &self,
        kind: &ComponentKind,
        entity_ref: &EntityRef,
    ) -> Option<Box<[u8]>> {
        let reader = self.bytes_readers.get(kind)?;
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
