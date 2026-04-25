//! Bit-budget audit for `Serde` derives + `SerdeInteger` family.
//!
//! Contract under test:
//!
//! > For every `T: Serde`, `T.bit_length()` returns exactly the number of
//! > bits `T.ser()` writes, and the `de(ser(x))` round-trip preserves equality.
//!
//! This harness pins that contract for every derive shape (struct, tuple
//! struct, enum) and for the `SerdeInteger` family (fixed and variable,
//! signed and unsigned). Both deterministic-boundary cases and
//! property-based fuzzing are used: boundaries to nail the off-by-one
//! corners (powers of two, zero, max), proptest to catch surprise
//! interactions across full input ranges.
//!
//! Phase 9.2 history: a 5-year-old off-by-one in derived-enum
//! `bits_needed_for(N)` was discovered + fixed during Phase 8.3. This
//! file exists to keep that class of bug from reappearing.

use naia_serde::{
    BitCounter, BitReader, BitWriter, ConstBitLength, Serde, SignedInteger, SignedVariableInteger,
    UnsignedInteger, UnsignedVariableInteger,
};
use naia_serde_derive::SerdeInternal;
use proptest::prelude::*;

/// Number of bits a fresh `BitCounter` records when `value.ser` runs against
/// it. The single source of truth for "what `ser` writes" — every assertion
/// in this file pivots on it.
fn bits_written_by_ser<T: Serde>(value: &T) -> u32 {
    let mut counter = BitCounter::new(0, 0, u32::MAX);
    value.ser(&mut counter);
    counter.bits_needed()
}

/// Roundtrip a value through a full `BitWriter` + `BitReader` and assert
/// it decodes equal. Returns the number of bits the writer actually
/// emitted, so callers can compare against `bit_length`.
fn round_trip<T: Serde + std::fmt::Debug + PartialEq>(value: &T) -> u32 {
    let mut writer = BitWriter::with_max_capacity();
    value.ser(&mut writer);
    // Capture the bit count *before* consuming the writer.
    let bits_via_counter = bits_written_by_ser(value);
    let bytes = writer.to_bytes();
    let mut reader = BitReader::new(&bytes);
    let decoded = T::de(&mut reader).expect("round-trip de should succeed");
    assert_eq!(*value, decoded, "round-trip preserved equality");
    bits_via_counter
}


// ----- Derive: struct (named fields) -----

#[derive(SerdeInternal, Clone, PartialEq, Debug)]
struct NamedFields {
    a: u8,
    b: u16,
    c: bool,
}

#[test]
fn struct_named_bit_length_matches_ser_and_round_trips() {
    let v = NamedFields { a: 7, b: 0xABCD, c: true };
    let bits = round_trip(&v);
    assert_eq!(v.bit_length(), bits, "struct bit_length tracks ser exactly");
    // Theoretical min: u8 + u16 + bool = 8 + 16 + 1 = 25 bits.
    assert_eq!(bits, 8 + 16 + 1, "no extra padding/tag on named struct");
}

// ----- Derive: tuple struct -----

#[derive(SerdeInternal, Clone, PartialEq, Debug)]
struct TupleFields(u32, i16, bool);

#[test]
fn struct_tuple_bit_length_matches_ser_and_round_trips() {
    let v = TupleFields(0xDEADBEEF, -123, false);
    let bits = round_trip(&v);
    assert_eq!(v.bit_length(), bits);
    // u32 (32) + i16 (16, raw bytes — no sign-bit prefix on plain ints) + bool (1) = 49.
    assert_eq!(bits, 32 + 16 + 1);
}

// ----- Derive: empty struct -----

#[derive(SerdeInternal, Clone, PartialEq, Debug)]
struct Empty;

#[test]
fn empty_struct_emits_zero_bits() {
    let v = Empty;
    assert_eq!(bits_written_by_ser(&v), 0);
    assert_eq!(v.bit_length(), 0);
}

// ----- Derive: enum at every power-of-two boundary -----

macro_rules! enum_with_n_variants {
    ($name:ident, $($variant:ident),+ $(,)?) => {
        #[derive(SerdeInternal, Clone, PartialEq, Debug)]
        enum $name {
            $($variant),+
        }
    };
}

enum_with_n_variants!(Enum1, V0);
enum_with_n_variants!(Enum2, V0, V1);
enum_with_n_variants!(Enum3, V0, V1, V2);
enum_with_n_variants!(Enum4, V0, V1, V2, V3);
enum_with_n_variants!(Enum5, V0, V1, V2, V3, V4);
enum_with_n_variants!(Enum7, V0, V1, V2, V3, V4, V5, V6);
enum_with_n_variants!(Enum8, V0, V1, V2, V3, V4, V5, V6, V7);
enum_with_n_variants!(Enum9, V0, V1, V2, V3, V4, V5, V6, V7, V8);
enum_with_n_variants!(
    Enum16, V0, V1, V2, V3, V4, V5, V6, V7, V8, V9, V10, V11, V12, V13, V14, V15
);
enum_with_n_variants!(
    Enum17, V0, V1, V2, V3, V4, V5, V6, V7, V8, V9, V10, V11, V12, V13, V14, V15, V16
);
enum_with_n_variants!(
    Enum32, V0, V1, V2, V3, V4, V5, V6, V7, V8, V9, V10, V11, V12, V13, V14, V15, V16, V17, V18,
    V19, V20, V21, V22, V23, V24, V25, V26, V27, V28, V29, V30, V31
);
enum_with_n_variants!(
    Enum33, V0, V1, V2, V3, V4, V5, V6, V7, V8, V9, V10, V11, V12, V13, V14, V15, V16, V17, V18,
    V19, V20, V21, V22, V23, V24, V25, V26, V27, V28, V29, V30, V31, V32
);

#[test]
fn enum_tag_widths_match_ceil_log2_at_every_boundary() {
    macro_rules! check {
        ($variant:expr, $expected_bits:expr) => {{
            let v = $variant;
            let bits = round_trip(&v);
            assert_eq!(v.bit_length(), bits, "bit_length tracks ser");
            assert_eq!(
                bits, $expected_bits,
                "tag width at boundary: got {}, want {}",
                bits, $expected_bits
            );
        }};
    }
    // Floor of 1 bit at N <= 2 (UnsignedInteger<0> doesn't exist).
    check!(Enum1::V0, 1);
    check!(Enum2::V1, 1);
    // 3..=4 -> 2 bits, 5..=8 -> 3 bits, 9..=16 -> 4 bits, etc.
    check!(Enum3::V2, 2);
    check!(Enum4::V3, 2);
    check!(Enum5::V4, 3);
    check!(Enum7::V6, 3);
    check!(Enum8::V7, 3);
    check!(Enum9::V8, 4);
    check!(Enum16::V15, 4);
    check!(Enum17::V16, 5);
    check!(Enum32::V31, 5);
    check!(Enum33::V32, 6);
}

// ----- Derive: enum with payloads -----

#[derive(SerdeInternal, Clone, PartialEq, Debug)]
enum WithPayloads {
    Empty,
    OneByte(u8),
    Pair(u16, bool),
    Named { x: i8, y: u32 },
}

#[test]
fn enum_with_payloads_bit_length_matches_ser_and_round_trips() {
    // 4 variants -> 2-bit tag.
    let cases: Vec<WithPayloads> = vec![
        WithPayloads::Empty,
        WithPayloads::OneByte(0xAB),
        WithPayloads::Pair(0xBEEF, true),
        WithPayloads::Named { x: -1, y: 0xDEADBEEF },
    ];
    for v in &cases {
        let bits = round_trip(v);
        assert_eq!(v.bit_length(), bits, "{:?}", v);
    }
}

// ----- Number primitives: fixed-width -----

#[test]
fn unsigned_fixed_width_round_trips_and_matches_const_bit_length() {
    let v = UnsignedInteger::<7>::new(123u8);
    let bits = round_trip(&v);
    assert_eq!(bits, 7);
    assert_eq!(v.bit_length(), 7);
    assert_eq!(<UnsignedInteger<7> as ConstBitLength>::const_bit_length(), 7);
}

#[test]
fn signed_fixed_width_round_trips_and_matches_const_bit_length() {
    let v = SignedInteger::<10>::new(-668i16);
    let bits = round_trip(&v);
    // sign bit + 10 magnitude bits.
    assert_eq!(bits, 11);
    assert_eq!(v.bit_length(), 11);
    assert_eq!(<SignedInteger<10> as ConstBitLength>::const_bit_length(), 11);
}

// ----- Number primitives: variable-width -----

fn variable_unsigned_expected_bits<const B: u8>(value: u64) -> u32 {
    // Mirror SerdeNumberInner::bit_length exactly.
    let mut bits: u32 = 0;
    let mut value = value;
    loop {
        let proceed = value >= 2u64.pow(B as u32);
        bits += 1; // proceed bit
        bits += B as u32;
        value >>= B;
        if !proceed {
            break;
        }
    }
    bits
}

#[test]
fn unsigned_variable_bit_length_matches_ser_at_known_boundaries() {
    // Block-size 3 → covers 3, 6, 9, 12, ... block-bit positions.
    macro_rules! check_uvar {
        ($bits:literal, $value:expr) => {{
            let raw: u64 = $value as u64;
            let v = UnsignedVariableInteger::<$bits>::new(raw);
            let bits = round_trip(&v);
            assert_eq!(v.bit_length(), bits, "uvar<{}>({})", $bits, raw);
            let expected = variable_unsigned_expected_bits::<$bits>(raw);
            assert_eq!(bits, expected, "uvar<{}>({}) bit count", $bits, raw);
        }};
    }
    check_uvar!(3, 0u32);
    check_uvar!(3, 7u32); // last value fitting in 3 bits → 4 bits total
    check_uvar!(3, 8u32); // first value forcing a continuation
    check_uvar!(3, 63u32);
    check_uvar!(3, 64u32);
    check_uvar!(3, 1_000_000u32);
    check_uvar!(5, 0u32);
    check_uvar!(5, 31u32);
    check_uvar!(5, 32u32);
    check_uvar!(5, u16::MAX as u32);
}

// ----- Property-based fuzzing -----

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn prop_named_struct_roundtrips(a in any::<u8>(), b in any::<u16>(), c in any::<bool>()) {
        let v = NamedFields { a, b, c };
        let bits = round_trip(&v);
        prop_assert_eq!(v.bit_length(), bits);
        prop_assert_eq!(bits, 8 + 16 + 1);
    }

    #[test]
    fn prop_tuple_struct_roundtrips(a in any::<u32>(), b in any::<i16>(), c in any::<bool>()) {
        let v = TupleFields(a, b, c);
        let bits = round_trip(&v);
        prop_assert_eq!(v.bit_length(), bits);
        // u32 raw (32) + i16 raw (16) + bool (1) = 49.
        prop_assert_eq!(bits, 32 + 16 + 1);
    }

    #[test]
    fn prop_enum4_tag_is_2_bits(idx in 0u8..4) {
        // Fanout to all four variants.
        let v = match idx {
            0 => Enum4::V0,
            1 => Enum4::V1,
            2 => Enum4::V2,
            _ => Enum4::V3,
        };
        let bits = round_trip(&v);
        prop_assert_eq!(v.bit_length(), bits);
        prop_assert_eq!(bits, 2);
    }

    #[test]
    fn prop_enum_with_payloads_roundtrips(
        case in 0u8..4,
        x in any::<i8>(),
        y in any::<u32>(),
        b in any::<u16>(),
        flag in any::<bool>(),
    ) {
        let v = match case {
            0 => WithPayloads::Empty,
            1 => WithPayloads::OneByte(x as u8),
            2 => WithPayloads::Pair(b, flag),
            _ => WithPayloads::Named { x, y },
        };
        let bits = round_trip(&v);
        prop_assert_eq!(v.bit_length(), bits);
    }

    #[test]
    fn prop_unsigned_fixed_width_roundtrips(value in 0u32..(1 << 16)) {
        let v = UnsignedInteger::<16>::new(value);
        let bits = round_trip(&v);
        prop_assert_eq!(v.bit_length(), bits);
        prop_assert_eq!(bits, 16);
    }

    #[test]
    fn prop_signed_fixed_width_roundtrips(value in -((1i32 << 14) - 1)..((1i32 << 14) - 1)) {
        let v = SignedInteger::<16>::new(value);
        let bits = round_trip(&v);
        prop_assert_eq!(v.bit_length(), bits);
        // SignedInteger<16> = sign bit + 16 magnitude bits.
        prop_assert_eq!(bits, 17);
    }

    #[test]
    fn prop_unsigned_variable_3_roundtrips(value in 0u32..1_000_000) {
        let v = UnsignedVariableInteger::<3>::new(value);
        let bits = round_trip(&v);
        prop_assert_eq!(v.bit_length(), bits);
        let expected = variable_unsigned_expected_bits::<3>(value as u64);
        prop_assert_eq!(bits, expected);
    }

    #[test]
    fn prop_signed_variable_4_roundtrips(value in -1_000_000i32..1_000_000) {
        let v = SignedVariableInteger::<4>::new(value);
        let bits = round_trip(&v);
        prop_assert_eq!(v.bit_length(), bits);
    }
}

// ----- ConstBitLength stays consistent with `bit_length()` for fixed types -----

#[test]
fn const_bit_length_agrees_with_bit_length_for_fixed_types() {
    macro_rules! check {
        ($t:ty, $value:expr) => {{
            let v: $t = $value;
            let const_bits = <$t as ConstBitLength>::const_bit_length();
            let inst_bits = v.bit_length();
            assert_eq!(
                const_bits, inst_bits,
                "{} ConstBitLength must match instance bit_length",
                stringify!($t)
            );
            // And that's the bits ser writes.
            assert_eq!(inst_bits, bits_written_by_ser(&v));
        }};
    }
    check!(UnsignedInteger<1>, UnsignedInteger::new(1u8));
    check!(UnsignedInteger<8>, UnsignedInteger::new(0xFFu8));
    check!(UnsignedInteger<16>, UnsignedInteger::new(0xFFFFu16));
    check!(SignedInteger<1>, SignedInteger::new(0i8));
    check!(SignedInteger<8>, SignedInteger::new(-1i8));
    check!(SignedInteger<16>, SignedInteger::new(-32_000i16));
}

// Compile-time witness that fixed-width number types implement
// `ConstBitLength`. The variable-width forms are correctly missing this
// impl — uncommenting the third line would fail to compile.
#[allow(dead_code)]
fn _const_bit_length_only_on_fixed() {
    fn requires_const<T: ConstBitLength>() {}
    requires_const::<UnsignedInteger<8>>();
    requires_const::<SignedInteger<8>>();
    // requires_const::<UnsignedVariableInteger<8>>();
}
