//! Phase-3 structural registry oracles: what registration stores.
//!
//! Each test registers derived types and reads back the stored facts —
//! descriptors, component facts, request pairs, resource net-IDs —
//! asserting the values the fingerprint v2 will consume. These are the
//! positive controls; the falsification matrix (Phase 5) asserts that
//! wire-incompatible mutations of these same fixtures change the
//! fingerprint.

use naia_shared::{
    ComponentKind, ComponentKinds, EntityProperty, Message, MessageKinds, Property, Replicate,
    Request, ResourceKinds, Response,
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
