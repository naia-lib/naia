//! T1.3e — pin the absence of the historical 64-kind cap end-to-end.
//!
//! Pre-2026-05-05, `ComponentKinds::add_component` panicked once the
//! 65th distinct kind tried to register. The per-user `DirtyQueue`
//! stored dirty bits in a single `AtomicU64` per entity, which was the
//! real ceiling. Both the assertion and the storage layer have been
//! rewritten to scale with `kind_count`, so registering, encoding, and
//! decoding far past 64 must work.
//!
//! This test registers 70 distinct `Replicate` types (10 sets × 7),
//! verifies `ComponentKinds::kind_count() == 70`, that the wire encoding
//! widens to 7 bits (ceil(log2(70))), and that every kind round-trips
//! through ser/de. It also exercises the `DirtyQueue` directly with
//! `kind_bit` values past 64, since that data structure is what the
//! assertion was originally protecting.

use naia_shared::{
    BitCounter, BitReader, BitWriter, ComponentKind, ComponentKinds, EntityIndex, Property,
    Replicate,
};

// 70 distinct unit-struct components. The macro keeps the source small
// while producing the 70 distinct `TypeId`s the registry needs (the
// `ComponentKinds` map is keyed by `TypeId`, so each struct must be a
// distinct type).
macro_rules! kinds {
    ($($name:ident),* $(,)?) => {
        $(
            #[derive(Replicate)]
            pub struct $name {
                pub v: Property<u8>,
            }
        )*
    };
}

kinds!(
    K00, K01, K02, K03, K04, K05, K06, K07, K08, K09, K10, K11, K12, K13, K14, K15, K16, K17, K18,
    K19, K20, K21, K22, K23, K24, K25, K26, K27, K28, K29, K30, K31, K32, K33, K34, K35, K36, K37,
    K38, K39, K40, K41, K42, K43, K44, K45, K46, K47, K48, K49, K50, K51, K52, K53, K54, K55, K56,
    K57, K58, K59, K60, K61, K62, K63, K64, K65, K66, K67, K68, K69,
);

macro_rules! register_all {
    ($kinds:expr, $($name:ident),* $(,)?) => {
        $(
            $kinds.add_component::<$name>();
        )*
    };
}

macro_rules! all_kinds_vec {
    ($($name:ident),* $(,)?) => {
        vec![$( ComponentKind::of::<$name>() ),*]
    };
}

fn build_70_kind_registry() -> ComponentKinds {
    let mut kinds = ComponentKinds::new();
    register_all!(
        kinds, K00, K01, K02, K03, K04, K05, K06, K07, K08, K09, K10, K11, K12, K13, K14, K15,
        K16, K17, K18, K19, K20, K21, K22, K23, K24, K25, K26, K27, K28, K29, K30, K31, K32, K33,
        K34, K35, K36, K37, K38, K39, K40, K41, K42, K43, K44, K45, K46, K47, K48, K49, K50, K51,
        K52, K53, K54, K55, K56, K57, K58, K59, K60, K61, K62, K63, K64, K65, K66, K67, K68, K69,
    );
    kinds
}

fn all_70() -> Vec<ComponentKind> {
    all_kinds_vec!(
        K00, K01, K02, K03, K04, K05, K06, K07, K08, K09, K10, K11, K12, K13, K14, K15, K16, K17,
        K18, K19, K20, K21, K22, K23, K24, K25, K26, K27, K28, K29, K30, K31, K32, K33, K34, K35,
        K36, K37, K38, K39, K40, K41, K42, K43, K44, K45, K46, K47, K48, K49, K50, K51, K52, K53,
        K54, K55, K56, K57, K58, K59, K60, K61, K62, K63, K64, K65, K66, K67, K68, K69,
    )
}

#[test]
fn registering_70_kinds_does_not_panic_and_kind_count_reflects_it() {
    let kinds = build_70_kind_registry();
    assert_eq!(
        kinds.kind_count(),
        70,
        "all 70 distinct kinds should register past the historical 64 cap"
    );
}

#[test]
fn wire_tag_widens_to_7_bits_at_70_kinds() {
    let kinds = build_70_kind_registry();
    // ceil(log2(70)) == 7
    let mut counter = BitCounter::new(0, 0, u32::MAX);
    ComponentKind::of::<K00>().ser(&kinds, &mut counter);
    assert_eq!(counter.bits_needed(), 7, "70 kinds → 7-bit kind tag");
}

#[test]
fn every_kind_past_64_round_trips() {
    let kinds = build_70_kind_registry();
    for kind in all_70() {
        let mut writer = BitWriter::with_max_capacity();
        kind.ser(&kinds, &mut writer);
        let bytes = writer.to_bytes();
        let mut reader = BitReader::new(&bytes);
        let decoded = ComponentKind::de(&kinds, &mut reader)
            .expect("ComponentKind round-trip should not error past 64-kind boundary");
        assert_eq!(decoded, kind, "round-trip mismatch at kind past 64");
    }
}

#[test]
fn dirty_queue_handles_kind_bits_past_64() {
    use naia_shared::DirtyQueue;
    // 70 kinds → stride == ceil(70/64) == 2 (two AtomicU64 words per entity).
    let q = DirtyQueue::new(70);
    assert_eq!(q.stride(), 2, "70 kinds requires 2 words per entity");
    q.ensure_capacity(0);
    // Push bits straddling the 64-bit word boundary — these would have
    // overflowed the historical single-u64 storage.
    for kb in [0u16, 63, 64, 65, 69] {
        q.push(EntityIndex(0), kb);
    }
    let drained = q.drain();
    assert_eq!(drained.len(), 1, "single entity, single drained entry");
    let (idx, words) = &drained[0];
    assert_eq!(*idx, EntityIndex(0));
    assert_eq!(words.len(), 2);
    // word 0 has bits 0 and 63
    assert_eq!(words[0], (1u64 << 0) | (1u64 << 63));
    // word 1 has bits 0 (=64), 1 (=65), 5 (=69)
    assert_eq!(words[1], (1u64 << 0) | (1u64 << 1) | (1u64 << 5));
}
