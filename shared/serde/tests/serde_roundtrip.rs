//! Property-based roundtrip suite for all `Serde` impls.
//!
//! Invariant: for any value `v` of type `T: Serde`, `T::de(T::ser(v)) == v`.
//! Covers scalars, Option, Vec, String, and the SerdeInteger family.
//! Runs under `cargo test -p naia-serde` — no special flags required.

use naia_serde::{
    BitReader, BitWriter, Serde, SignedInteger, SignedVariableInteger, UnsignedInteger,
    UnsignedVariableInteger,
};
use proptest::prelude::*;

fn roundtrip<T: Serde + PartialEq + std::fmt::Debug>(value: &T) -> T {
    let mut writer = BitWriter::with_max_capacity();
    value.ser(&mut writer);
    let bytes = writer.to_bytes();
    let mut reader = BitReader::new(&bytes);
    T::de(&mut reader).expect("roundtrip de must succeed on self-serialized bytes")
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    // ── Scalars ───────────────────────────────────────────────────────────

    #[test]
    fn prop_bool_roundtrips(v in any::<bool>()) {
        prop_assert_eq!(v, roundtrip(&v));
    }

    #[test]
    fn prop_u8_roundtrips(v in any::<u8>()) {
        prop_assert_eq!(v, roundtrip(&v));
    }

    #[test]
    fn prop_u16_roundtrips(v in any::<u16>()) {
        prop_assert_eq!(v, roundtrip(&v));
    }

    #[test]
    fn prop_u32_roundtrips(v in any::<u32>()) {
        prop_assert_eq!(v, roundtrip(&v));
    }

    #[test]
    fn prop_u64_roundtrips(v in any::<u64>()) {
        prop_assert_eq!(v, roundtrip(&v));
    }

    #[test]
    fn prop_i8_roundtrips(v in any::<i8>()) {
        prop_assert_eq!(v, roundtrip(&v));
    }

    #[test]
    fn prop_i16_roundtrips(v in any::<i16>()) {
        prop_assert_eq!(v, roundtrip(&v));
    }

    #[test]
    fn prop_i32_roundtrips(v in any::<i32>()) {
        prop_assert_eq!(v, roundtrip(&v));
    }

    #[test]
    fn prop_i64_roundtrips(v in any::<i64>()) {
        prop_assert_eq!(v, roundtrip(&v));
    }

    // Float roundtrip: NaN != NaN under IEEE 754, so compare via bits.
    #[test]
    fn prop_f32_roundtrips(bits in any::<u32>()) {
        let v = f32::from_bits(bits);
        let v2 = roundtrip(&v);
        prop_assert_eq!(v.to_bits(), v2.to_bits(), "f32 bit pattern must survive roundtrip");
    }

    #[test]
    fn prop_f64_roundtrips(bits in any::<u64>()) {
        let v = f64::from_bits(bits);
        let v2 = roundtrip(&v);
        prop_assert_eq!(v.to_bits(), v2.to_bits(), "f64 bit pattern must survive roundtrip");
    }

    // ── Option<T> ─────────────────────────────────────────────────────────

    #[test]
    fn prop_option_u32_roundtrips(opt in proptest::option::of(any::<u32>())) {
        prop_assert_eq!(opt, roundtrip(&opt));
    }

    #[test]
    fn prop_option_bool_roundtrips(opt in proptest::option::of(any::<bool>())) {
        prop_assert_eq!(opt, roundtrip(&opt));
    }

    // ── Vec<T> ────────────────────────────────────────────────────────────

    #[test]
    fn prop_vec_u8_roundtrips(v in proptest::collection::vec(any::<u8>(), 0..32)) {
        prop_assert_eq!(v.clone(), roundtrip(&v));
    }

    #[test]
    fn prop_vec_i32_roundtrips(v in proptest::collection::vec(any::<i32>(), 0..16)) {
        prop_assert_eq!(v.clone(), roundtrip(&v));
    }

    // ── String ────────────────────────────────────────────────────────────

    #[test]
    fn prop_string_roundtrips(s in "\\PC{0,64}") {
        let decoded = roundtrip(&s);
        prop_assert_eq!(s, decoded);
    }

    // ── SerdeInteger: fixed-width ─────────────────────────────────────────

    #[test]
    fn prop_unsigned_integer_8_roundtrips(v in 0u32..(1 << 8)) {
        let val = UnsignedInteger::<8>::new(v);
        prop_assert_eq!(val, roundtrip(&val));
    }

    #[test]
    fn prop_unsigned_integer_16_roundtrips(v in 0u32..(1 << 16)) {
        let val = UnsignedInteger::<16>::new(v);
        prop_assert_eq!(val, roundtrip(&val));
    }

    #[test]
    fn prop_signed_integer_12_roundtrips(v in -(1i32 << 11)..(1i32 << 11)) {
        let val = SignedInteger::<12>::new(v);
        prop_assert_eq!(val, roundtrip(&val));
    }

    // ── SerdeInteger: variable-width ──────────────────────────────────────

    #[test]
    fn prop_unsigned_variable_3_roundtrips(v in 0u32..1_000_000) {
        let val = UnsignedVariableInteger::<3>::new(v);
        prop_assert_eq!(val, roundtrip(&val));
    }

    #[test]
    fn prop_unsigned_variable_5_roundtrips(v in 0u64..(1 << 32)) {
        let val = UnsignedVariableInteger::<5>::new(v);
        prop_assert_eq!(val, roundtrip(&val));
    }

    #[test]
    fn prop_signed_variable_4_roundtrips(v in -500_000i32..500_000) {
        let val = SignedVariableInteger::<4>::new(v);
        prop_assert_eq!(val, roundtrip(&val));
    }
}
