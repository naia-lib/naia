//! Guard: no two DISTINCT element types in one unlabeled surface may compute
//! byte-identical [`WireSchema`] descriptors (Cyrus seq2247).
//!
//! Background. Usher's audit (Nydus seq2242/2244) swept naia/slag/cyberlith
//! for same-shape unlabeled composites whose fields could be transposed on
//! the wire without detection, and came back EMPTY: the only multi-element
//! tuple struct in all three repos is a naia test fixture, and the six real
//! unnamed enum variants all mix distinct descriptor kinds. His sweep had
//! three stated limits: regex-not-AST, descriptors read-not-computed, and
//! snapshot-not-guard. This test removes the second and third limits for the
//! in-naia population: it calls the real `wire_schema_bytes()` impls
//! (including derive-generated ones, which a regex sweep structurally cannot
//! see) and it fails closed, so a future collision breaks the build instead
//! of sitting in a report.
//!
//! What is covered. Every multi-element UNLABELED surface in naia today:
//!
//! - `SomeStruct(String, i16, bool)` -- tuple struct,
//!   `shared/tests/derive_tuple_struct.rs:4`.
//! - `SomeEnum::Variant3(u16, String)` -- unnamed enum variant,
//!   `shared/tests/derive_enum.rs:9`.
//! - `TestEnumMessage::Move(u32, u32)` -- unnamed enum variant,
//!   `shared/tests/derive_message_enum.rs:17`.
//!
//! Single-element unnamed variants (`Variant2(bool)`, the handshake and
//! request-sender enums) cannot collide with anything inside their own
//! surface and are excluded by construction. Named (labeled) surfaces are a
//! different hazard class and out of scope.
//!
//! Same-type exemption. `Move(u32, u32)` holds the same type twice: swapping
//! the two fields is order-insensitive by construction, and no schema can
//! distinguish two identical types. The predicate below therefore fails only
//! when two DISTINCT types compute equal bytes. A future edit that changes
//! one `u32` to a different-but-identical-descriptor type trips the guard.
//!
//! Review rule. This test pins today's population, not tomorrow's: adding a
//! new multi-element tuple struct or multi-element unnamed enum variant to
//! naia MUST extend this file with that surface's element list, or the new
//! surface is unguarded. Downstream mirrors (slag, cyberlith) own their own
//! populations -- in particular the `MessageId` (`UnsignedInteger<9>`) vs
//! `ChatChannelId` (`UnsignedInteger<12>`) near-miss pair, which shares the
//! field label `id` and is separated today only by bit count, must be pinned
//! downstream: widening `MessageId` to `<12>` would make the two descriptors
//! identical, and neither type lives in naia so this file cannot pin them.

use naia_shared::WireSchema;

/// One element of an unlabeled surface: its type name and its COMPUTED
/// descriptor bytes (never hand-transcribed).
fn element<T: WireSchema>() -> (&'static str, Vec<u8>) {
    (std::any::type_name::<T>(), T::wire_schema_bytes())
}

/// Fails closed if two DISTINCT element types in one surface compute
/// byte-identical descriptors -- the exact condition under which transposing
/// those two fields is invisible on the wire.
fn assert_no_distinct_duplicate(surface: &str, elements: Vec<(&'static str, Vec<u8>)>) {
    for (i, (name_a, bytes_a)) in elements.iter().enumerate() {
        for (name_b, bytes_b) in elements.iter().skip(i + 1) {
            if bytes_a == bytes_b && name_a != name_b {
                panic!(
                    "unlabeled surface `{surface}` has a transposition-invisible pair: \
                     distinct types `{name_a}` and `{name_b}` compute identical \
                     wire_schema_bytes ({bytes_a:?}); field order between them \
                     cannot be checked on the wire"
                );
            }
        }
    }
}

/// Tuple-struct surface: `SomeStruct(String, i16, bool)`
/// (`shared/tests/derive_tuple_struct.rs:4`).
#[test]
fn tuple_struct_surface_elements_are_pairwise_distinct() {
    assert_no_distinct_duplicate(
        "SomeStruct(String, i16, bool)",
        vec![element::<String>(), element::<i16>(), element::<bool>()],
    );
}

/// Unnamed-variant surface: `SomeEnum::Variant3(u16, String)`
/// (`shared/tests/derive_enum.rs:9`).
#[test]
fn unnamed_enum_variant_surface_elements_are_pairwise_distinct() {
    assert_no_distinct_duplicate(
        "SomeEnum::Variant3(u16, String)",
        vec![element::<u16>(), element::<String>()],
    );
}

/// Unnamed-variant surface: `TestEnumMessage::Move(u32, u32)`
/// (`shared/tests/derive_message_enum.rs:17`).
///
/// Same type twice: passes under the same-type exemption, and pins the
/// predicate so a future edit to a distinct-but-identical type trips it.
#[test]
fn same_type_twice_surface_passes_same_type_exemption() {
    assert_no_distinct_duplicate(
        "TestEnumMessage::Move(u32, u32)",
        vec![element::<u32>(), element::<u32>()],
    );
}
