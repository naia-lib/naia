// Wire format tests for OwnedLocalEntity with is_static bit.

use naia_shared::{BitCounter, BitReader, BitWriter, OwnedLocalEntity};

fn bit_length_of(entity: OwnedLocalEntity) -> u32 {
    let mut counter = BitCounter::new(0, 0, u32::MAX);
    entity.ser(&mut counter);
    counter.bits_needed()
}

fn round_trip(entity: OwnedLocalEntity) -> OwnedLocalEntity {
    let mut writer = BitWriter::with_max_capacity();
    entity.ser(&mut writer);
    let bytes = writer.to_bytes();
    let mut reader = BitReader::new(&bytes);
    OwnedLocalEntity::de(&mut reader).expect("round-trip de failed")
}

#[test]
fn dynamic_host_id_lt_128_costs_10_bits() {
    // is_host=1, is_static=0, varint<7>(5)=8 bits → total 10
    let e = OwnedLocalEntity::new_host_dynamic(5);
    assert_eq!(bit_length_of(e), 10, "dynamic host id=5 should cost 10 bits");
}

#[test]
fn static_host_id_lt_128_costs_10_bits() {
    // is_host=1, is_static=1, varint<7>(5)=8 bits → total 10
    let e = OwnedLocalEntity::new_host_static(5);
    assert_eq!(bit_length_of(e), 10, "static host id=5 should cost 10 bits");
}

#[test]
fn dynamic_host_id_in_128_to_16383_range_costs_18_bits() {
    // is_host=1, is_static=0, varint<7>(1000)=16 bits → total 18
    let e = OwnedLocalEntity::new_host_dynamic(1000);
    assert_eq!(bit_length_of(e), 18);
}

#[test]
fn remote_entity_costs_9_bits_no_is_static_bit() {
    // is_host=0, varint<7>(42)=8 bits → total 9 (no is_static bit for Remote)
    let e = OwnedLocalEntity::Remote(42);
    assert_eq!(bit_length_of(e), 9, "remote entity id=42 should cost 9 bits");
}

#[test]
fn round_trips_all_host_combinations() {
    for is_static in [false, true] {
        for id in [0u16, 1, 127, 128, 16_383, 16_384, 65_535] {
            let original = if is_static {
                OwnedLocalEntity::new_host_static(id)
            } else {
                OwnedLocalEntity::new_host_dynamic(id)
            };
            let decoded = round_trip(original);
            assert_eq!(decoded, original,
                "round-trip failed for Host {{ id: {id}, is_static: {is_static} }}");
        }
    }
}

#[test]
fn round_trips_remote_entities() {
    for id in [0u16, 1, 127, 128, 16_383, 65_535] {
        let original = OwnedLocalEntity::Remote(id);
        let decoded = round_trip(original);
        assert_eq!(decoded, original, "round-trip failed for Remote({id})");
    }
}

#[test]
fn static_split_saves_8_bits_per_dynamic_ref_when_tiles_push_ids_to_10k() {
    // Without split: tiles take IDs 0..9999, units get IDs 10_000+
    // → 1 is_host + 1 is_static + 16-bit varint = 18 bits per unit ref.
    // With split: units always start at 0
    // → 1 is_host + 1 is_static + 8-bit varint = 10 bits per unit ref.
    // Net: 8 bits saved per dynamic entity reference.
    let without_split = OwnedLocalEntity::new_host_dynamic(10_000);
    let with_split    = OwnedLocalEntity::new_host_dynamic(0);
    let saved = bit_length_of(without_split) - bit_length_of(with_split);
    assert_eq!(saved, 8,
        "splitting pools saves 8 bits/dynamic-ref when tiles push IDs to 10K+");
}
