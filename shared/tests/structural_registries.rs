//! Phase-3 structural registry oracles: what registration stores.
//!
//! Each test registers derived types and reads back the stored facts —
//! descriptors, component facts, request pairs, resource net-IDs —
//! asserting the values the fingerprint v2 will consume. These are the
//! positive controls; the falsification matrix (Phase 5) asserts that
//! wire-incompatible mutations of these same fixtures change the
//! fingerprint.

use std::{any::Any, collections::HashSet};

use naia_shared::{
    wire_schema_count, wire_schema_field, wire_schema_label, BitReader, BitWrite,
    ComponentFieldUpdate, ComponentKind, ComponentKinds, DiffMask, EntityAuthAccessor,
    EntityProperty, LocalEntityAndGlobalEntityConverter, LocalEntityAndGlobalEntityConverterMut,
    Message, MessageKinds, Named, PendingComponentUpdate, Property, PropertyMutator, RemoteEntity,
    ReplicaDynMut, ReplicaDynRef, Replicate, ReplicateBuilder, Request, ResourceKinds, Response,
    Serde, SerdeErr, WireSchema, WireSchemaContext, SCHEMA_TAG_BACKREF, SCHEMA_TAG_STRUCT,
    WIRE_SCHEMA_DOMAIN,
};

// --- fixtures --------------------------------------------------------------

#[derive(Message)]
struct Ping(u8);

#[derive(Message)]
struct Location {
    x: u16,
    y: u16,
}

#[derive(Message)]
struct GetState(u8);

#[derive(Message)]
struct State(u8);

impl Request for GetState {
    type Response = State;
}

impl Response for State {}

#[derive(Replicate)]
struct Position {
    x: Property<u8>,
    y: Property<u8>,
}

#[derive(Replicate)]
struct Interleaved {
    a: Property<u8>,
    plain: u16,
    b: Property<u8>,
}

#[derive(Replicate)]
struct WithRelation {
    x: Property<u8>,
    owner: EntityProperty,
}

#[derive(Replicate)]
#[replicate(immutable)]
struct Static {
    x: Property<u8>,
}

#[derive(Replicate)]
struct Score {
    value: Property<u32>,
}

// Same field names and types as `Position`, in the other order: the
// descriptors must differ, or field order is not covered.
#[derive(Replicate)]
struct Swapped {
    y: Property<u8>,
    x: Property<u8>,
}

// Same first field as `Position`, plus one more: adding a field must
// change the descriptor.
#[derive(Replicate)]
struct Extended {
    x: Property<u8>,
    y: Property<u8>,
    z: Property<u8>,
}

#[derive(Message)]
struct Pair {
    first: u8,
    second: u16,
}

// Self-containing generated types: the derives must root the traversal at
// `Self`, so the boxed self-edge folds to `BACKREF 0` with no spurious
// inlined level.
//
// Neither can derive the codec `Serde` (its derive emits its own support
// module and `Clone` impl, colliding with the registry derives), so both
// hand-implement it with unreachable bodies: these fixtures are only ever
// described, never serialized. `Serde` needs `Clone + PartialEq`; `Clone`
// comes from the registry derives themselves, `PartialEq` is derived.
#[derive(PartialEq, Message)]
#[allow(dead_code)]
struct ChainMsg {
    next: Box<ChainMsg>,
}

impl Serde for ChainMsg {
    fn ser(&self, _writer: &mut dyn BitWrite) {
        unreachable!("oracle fixture is described, never serialized");
    }
    fn de(_reader: &mut BitReader) -> Result<Self, SerdeErr> {
        unreachable!("oracle fixture is described, never serialized");
    }
    fn bit_length(&self) -> u32 {
        unreachable!("oracle fixture is described, never serialized");
    }
}

// Stands in for the codec `Serde` derive's `WireSchema` impl: the field
// path needs `Box<ChainMsg>: WireSchema`, which needs `ChainMsg` itself
// to implement it. Routes the single field through the canonical field
// path, exactly as generated codec impls do.
impl WireSchema for ChainMsg {
    fn wire_schema(ctx: &mut WireSchemaContext, out: &mut Vec<u8>) {
        out.push(SCHEMA_TAG_STRUCT);
        wire_schema_count(out, 1);
        wire_schema_label(out, "next");
        wire_schema_field::<Box<ChainMsg>>(ctx, out);
    }
}

// `Property` has no `PartialEq`, so equality is hand-written through the
// deref: comparing the innards is what the fixture needs, and the
// self-reference resolves through this very impl, the standard recursive
// manual-impl shape.
#[derive(Replicate)]
#[allow(dead_code)]
struct ChainComp {
    next: Property<Box<ChainComp>>,
}

// Identity comparison: a structural `==` would recurse forever on a
// self-containing value (comparing innards compares the whole value
// again). Fixtures are never compared — the bound exists only to satisfy
// `Serde` — so address identity is the honest finite choice.
impl PartialEq for ChainComp {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(&*self.next, &*other.next)
    }
}

// Codec-level `WireSchema` stand-in, as for `ChainMsg`: the Replicate
// descriptor's field path needs the inner `Box<ChainComp>` describable.
impl WireSchema for ChainComp {
    fn wire_schema(ctx: &mut WireSchemaContext, out: &mut Vec<u8>) {
        out.push(SCHEMA_TAG_STRUCT);
        wire_schema_count(out, 1);
        wire_schema_label(out, "next");
        wire_schema_field::<Box<ChainComp>>(ctx, out);
    }
}

impl Serde for ChainComp {
    fn ser(&self, _writer: &mut dyn BitWrite) {
        unreachable!("oracle fixture is described, never serialized");
    }
    fn de(_reader: &mut BitReader) -> Result<Self, SerdeErr> {
        unreachable!("oracle fixture is described, never serialized");
    }
    fn bit_length(&self) -> u32 {
        unreachable!("oracle fixture is described, never serialized");
    }
}

#[derive(Message)]
struct PairSwapped {
    second: u16,
    first: u8,
}

// --- message descriptors ---------------------------------------------------

#[test]
fn message_descriptors_are_stored_in_net_id_order() {
    let mut kinds = MessageKinds::new();
    kinds.add_message::<Ping>();
    kinds.add_message::<Location>();

    let entries = kinds.schema_descriptor_entries();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].0, 0);
    assert_eq!(entries[1].0, 1);
    // Same-type reregistration would collide; distinct types differ.
    assert_ne!(entries[0].1, entries[1].1);
    // Tuple vs struct node: first byte after the domain tag differs.
    assert_ne!(
        entries[0].1[naia_shared::WIRE_SCHEMA_DOMAIN.len()],
        entries[1].1[naia_shared::WIRE_SCHEMA_DOMAIN.len()]
    );
    // Both end with the two zero fact bytes (not fragment, not request).
    for (_, descriptor) in &entries {
        let tail = &descriptor[descriptor.len() - 2..];
        assert_eq!(tail, &[0, 0]);
    }
}

// --- request pairing --------------------------------------------------------

#[test]
fn request_response_pairing_is_recorded_as_net_ids() {
    let mut kinds = MessageKinds::new();
    kinds.add_message::<Ping>();
    kinds.add_request::<GetState>();

    // Ping = 0, GetState = 1, State = 2.
    assert_eq!(kinds.request_response_pairs(), &[(1, 2)]);
    assert_eq!(kinds.schema_descriptor_entries().len(), 3);
}

// --- component facts --------------------------------------------------------

#[test]
fn component_facts_capture_labels_indices_and_mask() {
    let mut kinds = ComponentKinds::new();
    kinds.add_component::<Position>();
    kinds.add_component::<Interleaved>();

    let entries = kinds.schema_fact_entries();
    assert_eq!(entries.len(), 2);

    let (_, position) = &entries[0];
    assert_eq!(position.name, "Position");
    assert_eq!(position.property_labels, vec!["x", "y"]);
    assert_eq!(position.mask_indices, vec![0, 1]);
    assert_eq!(position.mask_size_bytes, 1);
    assert!(!position.immutable);
    assert!(!position.has_entity_properties);
    assert!(position.entity_property_labels.is_empty());
    assert!(position.authority_delegable);
    assert!(!position.descriptor.is_empty());

    // The plain `u16` consumes mask position 1: sparse, not dense.
    let (_, interleaved) = &entries[1];
    assert_eq!(interleaved.property_labels, vec!["a", "b"]);
    assert_eq!(interleaved.mask_indices, vec![0, 2]);
    assert_eq!(interleaved.mask_size_bytes, 1);
}

#[test]
fn entity_and_immutable_facts_are_recorded() {
    let mut kinds = ComponentKinds::new();
    kinds.add_component::<WithRelation>();
    kinds.add_component::<Static>();

    let entries = kinds.schema_fact_entries();
    let (_, related) = &entries[0];
    assert!(related.has_entity_properties);
    assert_eq!(related.entity_property_labels, vec!["owner"]);
    assert_eq!(related.property_labels, vec!["x", "owner"]);
    assert!(related.authority_delegable);

    // Immutable components are never diff-tracked: nothing to delegate.
    let (_, statik) = &entries[1];
    assert!(statik.immutable);
    assert!(!statik.authority_delegable);
}

// --- descriptor precision ---------------------------------------------------
//
// The fingerprint can only distinguish what the descriptors distinguish.
// These pin that field order, field addition, and entity-relation changes
// each move the descriptor bytes: same-name fixtures are expressible here
// (unlike across distinct protocol builds, where the type name always
// differs too), so this is where precision — not just detection — lives.

#[test]
fn field_reorder_changes_the_component_descriptor() {
    let mut kinds = ComponentKinds::new();
    kinds.add_component::<Position>();
    kinds.add_component::<Swapped>();

    let entries = kinds.schema_fact_entries();
    assert_ne!(entries[0].1.descriptor, entries[1].1.descriptor);
    // The facts say why: same labels as sets, different order and order
    // is what the mask walks.
    assert_eq!(entries[1].1.property_labels, vec!["y", "x"]);
}

#[test]
fn adding_a_field_changes_the_component_descriptor() {
    let mut kinds = ComponentKinds::new();
    kinds.add_component::<Position>();
    kinds.add_component::<Extended>();

    let entries = kinds.schema_fact_entries();
    assert_ne!(entries[0].1.descriptor, entries[1].1.descriptor);
    assert_eq!(entries[1].1.property_labels, vec!["x", "y", "z"]);
    assert_eq!(entries[1].1.mask_indices, vec![0, 1, 2]);
}

#[test]
fn field_reorder_changes_the_message_descriptor() {
    let mut kinds = MessageKinds::new();
    kinds.add_message::<Pair>();
    kinds.add_message::<PairSwapped>();

    let entries = kinds.schema_descriptor_entries();
    assert_ne!(entries[0].1, entries[1].1);
}

#[test]
fn adding_an_entity_relation_changes_facts_and_descriptor() {
    let mut kinds = ComponentKinds::new();
    kinds.add_component::<Position>();
    kinds.add_component::<WithRelation>();

    let entries = kinds.schema_fact_entries();
    assert_ne!(entries[0].1.descriptor, entries[1].1.descriptor);
    assert!(!entries[0].1.has_entity_properties);
    assert!(entries[1].1.has_entity_properties);
}

// --- generated root seeding -------------------------------------------------
//
// The derives start their standalone descriptors from a traversal rooted
// at `Self`. These pin the exact bytes for self-containing types: one
// STRUCT level, then `BACKREF 0` — no spurious inlined level. An unseeded
// (`new()`) context would inline one full level before anything folds;
// direct-child dispatch in `Box` would nest a level and fold to
// `BACKREF 1`. Both failure shapes fail these assertions.

fn backref_zero_struct(label: &str, tail: &[u8]) -> Vec<u8> {
    let mut expected = Vec::new();
    expected.extend_from_slice(WIRE_SCHEMA_DOMAIN);
    expected.push(SCHEMA_TAG_STRUCT);
    let count: u32 = 1;
    expected.extend_from_slice(&count.to_le_bytes());
    let label_bytes = label.as_bytes();
    let label_len: u32 = label_bytes.len() as u32;
    expected.extend_from_slice(&label_len.to_le_bytes());
    expected.extend_from_slice(label_bytes);
    expected.push(SCHEMA_TAG_BACKREF);
    expected.extend_from_slice(&0u32.to_le_bytes());
    expected.extend_from_slice(tail);
    expected
}

#[test]
fn generated_message_roots_its_descriptor_at_self() {
    assert_eq!(
        <ChainMsg as Message>::wire_schema(),
        backref_zero_struct("next", &[0, 0])
    );
}

#[test]
fn generated_replicate_roots_its_descriptor_at_self() {
    assert_eq!(
        <ChainComp as Replicate>::wire_schema(),
        backref_zero_struct("next", &[])
    );
}

// --- malformed facts are refused --------------------------------------------
//
// Hand-written `Replicate` impls whose schema methods disagree about their
// own layout: label/index length mismatch, and an entity label naming no
// wired property. Registration must panic in release mode (plain
// `assert!`, never `debug_assert!`), before anything is stored — a
// registered lie would frame a truncated fingerprint section.

macro_rules! malformed_component {
    ($name:ident, $builder:ident, $labels:expr, $indices:expr, $entity:expr) => {
        #[allow(dead_code)]
        struct $name;
        struct $builder;
        impl Named for $name {
            fn name(&self) -> String {
                stringify!($name).to_string()
            }
            fn protocol_name() -> &'static str {
                stringify!($name)
            }
        }
        impl Named for $builder {
            fn name(&self) -> String {
                stringify!($builder).to_string()
            }
            fn protocol_name() -> &'static str {
                stringify!($builder)
            }
        }
        impl ReplicateBuilder for $builder {
            fn read(
                &self,
                _reader: &mut BitReader,
                _converter: &dyn LocalEntityAndGlobalEntityConverter,
            ) -> Result<Box<dyn Replicate>, SerdeErr> {
                unreachable!("refused before any read")
            }
            fn read_create_update(
                &self,
                _reader: &mut BitReader,
            ) -> Result<PendingComponentUpdate, SerdeErr> {
                unreachable!("refused before any read")
            }
            fn split_update(
                &self,
                _converter: &dyn LocalEntityAndGlobalEntityConverter,
                _update: PendingComponentUpdate,
            ) -> Result<
                (
                    Option<Vec<(RemoteEntity, ComponentFieldUpdate)>>,
                    Option<PendingComponentUpdate>,
                ),
                SerdeErr,
            > {
                unreachable!("refused before any read")
            }
            fn box_clone(&self) -> Box<dyn ReplicateBuilder> {
                Box::new($builder)
            }
        }
        impl Replicate for $name {
            fn kind(&self) -> ComponentKind {
                ComponentKind::of::<$name>()
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
                unreachable!("refused before any copy")
            }
            fn create_builder() -> Box<dyn ReplicateBuilder>
            where
                Self: Sized,
            {
                Box::new($builder)
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
            fn mirror(&mut self, _other: &dyn Replicate) {
                unreachable!("refused before any mirror")
            }
            fn mirror_single_field(&mut self, _index: u8, _other: &dyn Replicate) {
                unreachable!("refused before any mirror")
            }
            fn set_mutator(&mut self, _mutator: &PropertyMutator) {}
            fn write(
                &self,
                _kinds: &ComponentKinds,
                _writer: &mut dyn BitWrite,
                _converter: &mut dyn LocalEntityAndGlobalEntityConverterMut,
            ) {
                unreachable!("refused before any write")
            }
            fn write_update(
                &self,
                _mask: &DiffMask,
                _writer: &mut dyn BitWrite,
                _converter: &mut dyn LocalEntityAndGlobalEntityConverterMut,
            ) {
                unreachable!("refused before any write")
            }
            fn read_apply_update(
                &mut self,
                _converter: &dyn LocalEntityAndGlobalEntityConverter,
                _update: PendingComponentUpdate,
            ) -> Result<(), SerdeErr> {
                unreachable!("refused before any read")
            }
            fn read_apply_field_update(
                &mut self,
                _converter: &dyn LocalEntityAndGlobalEntityConverter,
                _update: ComponentFieldUpdate,
            ) -> Result<(), SerdeErr> {
                unreachable!("refused before any read")
            }
            fn relations_waiting(&self) -> Option<HashSet<RemoteEntity>> {
                None
            }
            fn relations_complete(
                &mut self,
                _converter: &dyn LocalEntityAndGlobalEntityConverter,
            ) -> bool {
                true
            }
            fn publish(&mut self, _mutator: &PropertyMutator) {}
            fn unpublish(&mut self) {}
            fn enable_delegation(
                &mut self,
                _accessor: &EntityAuthAccessor,
                _mutator: Option<&PropertyMutator>,
            ) {
            }
            fn disable_delegation(&mut self) {}
            fn localize(&mut self) {}
            fn wire_schema() -> Vec<u8>
            where
                Self: Sized,
            {
                Vec::new()
            }
            fn replicated_property_labels() -> Vec<&'static str>
            where
                Self: Sized,
            {
                $labels
            }
            fn property_mask_indices() -> Vec<u8>
            where
                Self: Sized,
            {
                $indices
            }
            fn mask_size_bytes() -> u8
            where
                Self: Sized,
            {
                0
            }
            fn entity_property_labels() -> Vec<&'static str>
            where
                Self: Sized,
            {
                $entity
            }
        }
    };
}

malformed_component!(Ragged, RaggedBuilder, vec!["a", "b"], vec![0], vec![]);
malformed_component!(Stray, StrayBuilder, vec!["a"], vec![0], vec!["ghost"]);

#[test]
#[should_panic(expected = "property labels")]
fn mismatched_label_and_index_lengths_are_refused() {
    ComponentKinds::new().add_component::<Ragged>();
}

#[test]
#[should_panic(expected = "entity labels")]
fn entity_label_outside_wired_properties_is_refused() {
    ComponentKinds::new().add_component::<Stray>();
}

// --- resources by net ID ----------------------------------------------------

#[test]
fn resources_resolve_to_component_net_ids_numerically() {
    let mut components = ComponentKinds::new();
    components.add_component::<Position>();
    components.add_component::<Score>();

    let mut resources = ResourceKinds::new();
    resources.register::<Score>(ComponentKind::of::<Score>());

    // Score is component net-ID 1; membership is by ID, not by name.
    assert_eq!(resources.member_net_ids(&components), vec![1]);
}
