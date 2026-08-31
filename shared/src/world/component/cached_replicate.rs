//! `CachedReplicate` — narrow serialization surface for the pipelined send path.
//!
//! Cyberlith (and any other pipelined consumer) ships
//! `Box<dyn CachedReplicate>` values per tick via `SnapshotWorld<E>`. The
//! blanket impl below means every `Replicate` is automatically a
//! `CachedReplicate`, so typed values coerce for free.
//!
//! See `SPEC_IRIS_2_NAIA.md` §1.1 (cyberlith repo) for design rationale.

use std::any::Any;

use naia_serde::BitWrite;

use crate::world::component::component_kinds::ComponentKinds;
use crate::world::component::replicate::Replicate;
use crate::world::entity::entity_converters::LocalEntityAndGlobalEntityConverterMut;
use crate::world::update::diff_mask::DiffMask;

/// Minimal serialization surface naia's Iris send-stage needs to write a
/// replicated component to the wire. Narrower than the full [`Replicate`]
/// trait — covers only the two methods invoked during
/// `send_all_packets`.
///
/// Every type implementing [`Replicate`] automatically implements
/// `CachedReplicate` via the blanket impl below, so cyberlith's typed
/// values coerce for free into `Box<dyn CachedReplicate>` storage in
/// `SnapshotWorld<E>`.
///
/// The `Any` supertrait enables downcasting from `&dyn CachedReplicate`
/// to the concrete type, mirroring [`Replicate::to_any`]. This is how
/// `SnapshotWorld<E>`'s `WorldRefType::component` impl recovers the
/// typed reference required by [`crate::ReplicaRefWrapper`].
pub trait CachedReplicate: Send + Sync + Any {
    /// Serialize the full component state. Called by Iris for
    /// `EntityCommand::SpawnWithComponents` (scope-enter) and
    /// `EntityCommand::InsertComponent` (mid-life component insert).
    fn cached_write(
        &self,
        component_kinds: &ComponentKinds,
        writer: &mut dyn BitWrite,
        converter: &mut dyn LocalEntityAndGlobalEntityConverterMut,
    );

    /// Serialize a partial state filtered by `diff_mask`. Called by
    /// Iris's PATH A wire-cache miss path and fallback two-pass.
    fn cached_write_update(
        &self,
        diff_mask: &DiffMask,
        writer: &mut dyn BitWrite,
        converter: &mut dyn LocalEntityAndGlobalEntityConverterMut,
    );

    /// `&dyn Any` for downcasting to the concrete type. Mirrors
    /// [`Replicate::to_any`].
    fn cached_as_any(&self) -> &dyn Any;
}

/// Blanket impl. Every [`Replicate`] is a [`CachedReplicate`].
impl<R: Replicate> CachedReplicate for R {
    fn cached_write(
        &self,
        component_kinds: &ComponentKinds,
        writer: &mut dyn BitWrite,
        converter: &mut dyn LocalEntityAndGlobalEntityConverterMut,
    ) {
        Replicate::write(self, component_kinds, writer, converter)
    }

    fn cached_write_update(
        &self,
        diff_mask: &DiffMask,
        writer: &mut dyn BitWrite,
        converter: &mut dyn LocalEntityAndGlobalEntityConverterMut,
    ) {
        Replicate::write_update(self, diff_mask, writer, converter)
    }

    fn cached_as_any(&self) -> &dyn Any {
        Replicate::to_any(self)
    }
}

#[cfg(test)]
mod cached_replicate_tests {
    use naia_serde::BitWriter;

    use crate::{
        world::{
            component::component_kinds::ComponentKinds,
            entity::entity_converters::FakeEntityConverter, update::diff_mask::DiffMask,
        },
        CachedReplicate, ComponentKind, Property, Replicate,
    };

    #[derive(Replicate)]
    struct Ghost {
        pale: Property<u8>,
        cold: Property<u8>,
    }

    #[derive(Replicate)]
    struct Wraith {
        value: Property<u8>,
    }

    fn kinds() -> ComponentKinds {
        let mut kinds = ComponentKinds::new();
        kinds.add_component::<Ghost>();
        kinds
    }

    fn a_ghost() -> Ghost {
        Ghost::new_complete(7, 9)
    }

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn the_boxed_trait_object_can_cross_a_thread_boundary() {
        // SnapshotWorld ships these between the sim and send stages.
        assert_send_sync::<Box<dyn CachedReplicate>>();
    }

    #[test]
    fn a_full_write_through_the_cached_surface_is_byte_identical_to_replicate() {
        let ghost = a_ghost();
        let kinds = kinds();

        let mut direct = BitWriter::new();
        Replicate::write(&ghost, &kinds, &mut direct, &mut FakeEntityConverter);

        let cached: Box<dyn CachedReplicate> = Box::new(a_ghost());
        let mut through_trait = BitWriter::new();
        cached.cached_write(&kinds, &mut through_trait, &mut FakeEntityConverter);

        let direct = direct.to_bytes();
        assert_eq!(direct, through_trait.to_bytes());
        assert!(!direct.is_empty());
    }

    #[test]
    fn an_update_write_through_the_cached_surface_is_byte_identical_to_replicate() {
        let ghost = a_ghost();
        let mut diff_mask = DiffMask::new(ghost.diff_mask_size());
        diff_mask.set_bit(1, true);

        let mut direct = BitWriter::new();
        Replicate::write_update(&ghost, &diff_mask, &mut direct, &mut FakeEntityConverter);

        let cached: Box<dyn CachedReplicate> = Box::new(a_ghost());
        let mut through_trait = BitWriter::new();
        cached.cached_write_update(&diff_mask, &mut through_trait, &mut FakeEntityConverter);

        assert_eq!(direct.to_bytes(), through_trait.to_bytes());
    }

    #[test]
    fn an_update_write_carries_only_the_fields_the_diff_mask_names() {
        let ghost = a_ghost();
        let cached: Box<dyn CachedReplicate> = Box::new(a_ghost());

        let write_with = |set: &[u8]| {
            let mut mask = DiffMask::new(ghost.diff_mask_size());
            for bit in set {
                mask.set_bit(*bit, true);
            }
            let mut writer = BitWriter::new();
            cached.cached_write_update(&mask, &mut writer, &mut FakeEntityConverter);
            writer.to_bytes()
        };

        let nothing = write_with(&[]);
        let pale_only = write_with(&[0]);
        let cold_only = write_with(&[1]);
        let both = write_with(&[0, 1]);

        // Each named field adds its own payload, and the two fields hold
        // different values, so naming one is not the same as naming the other.
        assert!(pale_only.len() > nothing.len());
        assert!(both.len() > pale_only.len());
        assert_ne!(pale_only, cold_only);
        assert_eq!(pale_only.len(), cold_only.len());
    }

    #[test]
    fn the_component_kind_written_is_the_one_the_registry_holds() {
        let kinds = kinds();
        let cached: Box<dyn CachedReplicate> = Box::new(a_ghost());

        let mut writer = BitWriter::new();
        cached.cached_write(&kinds, &mut writer, &mut FakeEntityConverter);
        let bytes = writer.to_bytes();

        let mut expected = BitWriter::new();
        ComponentKind::of::<Ghost>().ser(&kinds, &mut expected);
        let prefix = expected.to_bytes();

        assert_eq!(&bytes[..prefix.len()], &prefix[..]);
    }

    #[test]
    fn the_any_surface_recovers_the_concrete_type() {
        let cached: Box<dyn CachedReplicate> = Box::new(a_ghost());

        let recovered = cached
            .cached_as_any()
            .downcast_ref::<Ghost>()
            .expect("downcast must recover the concrete type");

        assert_eq!(*recovered.pale, 7);
        assert_eq!(*recovered.cold, 9);
    }

    #[test]
    fn the_any_surface_refuses_a_type_that_is_not_there() {
        let cached: Box<dyn CachedReplicate> = Box::new(a_ghost());

        assert!(cached.cached_as_any().downcast_ref::<Wraith>().is_none());
    }
}
