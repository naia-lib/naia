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
mod tests {
    use super::*;

    // Verify Send + Sync for Box<dyn CachedReplicate>.
    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn box_dyn_cached_replicate_is_send_sync() {
        assert_send_sync::<Box<dyn CachedReplicate>>();
    }

    #[test]
    fn blanket_impl_dispatches_to_replicate() {
        // We use an existing Replicate-implementing type from the
        // workspace's bench protocol or a stub in shared's test utils.
        // Since `shared` is the host crate of the trait + blanket impl,
        // we synthesise a trivial Replicate impl here just to verify
        // the blanket impl compiles and the trait object is callable.
        //
        // The full byte-identity confirmation is exercised in
        // `server/tests/snapshot_world_wire_identity.rs` (Commit 5),
        // which builds end-to-end packets with real Replicate types.

        // Compile-only assertion: a generic function taking a
        // Replicate impl and using it via CachedReplicate's dyn surface
        // must coerce without explicit casts.
        fn _coerce<R: Replicate>(value: R) -> Box<dyn CachedReplicate> {
            Box::new(value)
        }

        // No-op runtime check that the function pointer is exported.
        // (The unit test that actually exercises a write call lives in
        // Commit 2's snapshot_world tests, which use real Replicate types
        // from the bench protocol or the bevy_specs test scaffolding.)
        let _ = _coerce::<DummyImmutable>;
    }

    // A minimal Replicate stub used to verify the blanket impl
    // type-checks. We intentionally do NOT exercise write/write_update
    // here because doing so would require a ComponentKinds registry
    // populated with this type's NetId — that ergonomic burden belongs
    // to the integration tests, not the trait-existence unit test.
    struct DummyImmutable;

    use crate::named::Named;
    use crate::world::component::component_kinds::ComponentKind;
    use crate::world::component::property_mutate::PropertyMutator;
    use crate::world::component::replica_ref::{ReplicaDynMut, ReplicaDynRef};
    use crate::world::component::replicate::ReplicateBuilder;
    use crate::world::delegation::auth_channel::EntityAuthAccessor;
    use crate::world::entity::entity_converters::LocalEntityAndGlobalEntityConverter;
    use crate::world::update::component_update::{ComponentFieldUpdate, PendingComponentUpdate};
    use crate::RemoteEntity;
    use naia_serde::{BitReader, SerdeErr};
    use std::collections::HashSet;

    impl Named for DummyImmutable {
        fn protocol_name() -> &'static str {
            "DummyImmutable"
        }
        fn name(&self) -> String {
            "DummyImmutable".to_string()
        }
    }

    struct DummyBuilder;
    impl Named for DummyBuilder {
        fn protocol_name() -> &'static str {
            "DummyImmutable"
        }
        fn name(&self) -> String {
            "DummyImmutable".to_string()
        }
    }
    impl ReplicateBuilder for DummyBuilder {
        fn read(
            &self,
            _reader: &mut BitReader,
            _converter: &dyn LocalEntityAndGlobalEntityConverter,
        ) -> Result<Box<dyn Replicate>, SerdeErr> {
            Ok(Box::new(DummyImmutable))
        }
        fn read_create_update(
            &self,
            _reader: &mut BitReader,
        ) -> Result<PendingComponentUpdate, SerdeErr> {
            unreachable!("DummyImmutable does not exercise updates")
        }
        fn split_update(
            &self,
            _converter: &dyn LocalEntityAndGlobalEntityConverter,
            _update: PendingComponentUpdate,
        ) -> crate::world::component::replicate::SplitUpdateResult {
            unreachable!("DummyImmutable does not exercise updates")
        }
        fn box_clone(&self) -> Box<dyn ReplicateBuilder> {
            Box::new(DummyBuilder)
        }
    }

    impl Replicate for DummyImmutable {
        fn kind(&self) -> ComponentKind {
            ComponentKind::of::<DummyImmutable>()
        }
        fn to_any(&self) -> &dyn Any {
            self
        }
        fn to_any_mut(&mut self) -> &mut dyn Any {
            self
        }
        fn to_boxed_any(self: Box<Self>) -> Box<dyn Any> {
            self
        }
        fn copy_to_box(&self) -> Box<dyn Replicate> {
            Box::new(DummyImmutable)
        }
        fn create_builder() -> Box<dyn ReplicateBuilder>
        where
            Self: Sized,
        {
            Box::new(DummyBuilder)
        }
        fn diff_mask_size(&self) -> u8 {
            0
        }
        fn dyn_ref(&self) -> ReplicaDynRef<'_> {
            ReplicaDynRef::new(self)
        }
        fn dyn_mut(&mut self) -> ReplicaDynMut<'_> {
            ReplicaDynMut::new(self)
        }
        fn mirror(&mut self, _other: &dyn Replicate) {}
        fn mirror_single_field(&mut self, _field_index: u8, _other: &dyn Replicate) {}
        fn set_mutator(&mut self, _mutator: &PropertyMutator) {}
        fn write(
            &self,
            _component_kinds: &ComponentKinds,
            _writer: &mut dyn BitWrite,
            _converter: &mut dyn LocalEntityAndGlobalEntityConverterMut,
        ) {
        }
        fn write_update(
            &self,
            _diff_mask: &DiffMask,
            _writer: &mut dyn BitWrite,
            _converter: &mut dyn LocalEntityAndGlobalEntityConverterMut,
        ) {
        }
        fn write_fields_for_comparison(&self, _writer: &mut dyn BitWrite) {}
        fn read_apply_update(
            &mut self,
            _converter: &dyn LocalEntityAndGlobalEntityConverter,
            _update: PendingComponentUpdate,
        ) -> Result<(), SerdeErr> {
            Ok(())
        }
        fn read_apply_field_update(
            &mut self,
            _converter: &dyn LocalEntityAndGlobalEntityConverter,
            _update: ComponentFieldUpdate,
        ) -> Result<(), SerdeErr> {
            Ok(())
        }
        fn relations_waiting(&self) -> Option<HashSet<RemoteEntity>> {
            None
        }
        fn relations_complete(&mut self, _converter: &dyn LocalEntityAndGlobalEntityConverter) {}
        fn publish(&mut self, _mutator: &PropertyMutator) {}
        fn unpublish(&mut self) {}
        fn enable_delegation(
            &mut self,
            _accessor: &EntityAuthAccessor,
            _mutator_opt: Option<&PropertyMutator>,
        ) {
        }
        fn disable_delegation(&mut self) {}
        fn localize(&mut self) {}
    }

    #[test]
    fn dyn_any_downcast_recovers_concrete_type() {
        let boxed: Box<dyn CachedReplicate> = Box::new(DummyImmutable);
        let any: &dyn Any = boxed.as_ref().cached_as_any();
        assert!(
            any.is::<DummyImmutable>(),
            "downcast must recover concrete type"
        );
    }
}
